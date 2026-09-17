//! Engine-owned evidence, qualification and conservative paired decisions.
use crate::{
    config::{CacheMode, CommandSpec, NetworkPolicy},
    measurement::{self, Metrics, Summary},
    session::{self, Decision, SessionError, SessionGuard, SessionStore},
    supervisor::{self, Cancellation, ProcessOutcome, ProcessReport},
    workspace::{self, Contents},
    SessionConfig,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, SessionError>;
const REPORT_RESERVE: u64 = 4 * 1024 * 1024;
const REPORT_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub phase: String,
    pub side: String,
    pub index: u32,
    pub seed: Option<u64>,
    pub metrics: Metrics,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    pub name: String,
    pub side: String,
    pub stdout: String,
    pub stderr: String,
    pub process: ProcessReport,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationReport {
    pub format_version: u32,
    pub operation_id: String,
    pub candidate: Option<String>,
    pub parent: String,
    pub config_sha256: String,
    pub parent_sha256: String,
    pub candidate_sha256: Option<String>,
    pub method_sha256: String,
    pub decision: Decision,
    pub reason: String,
    pub stages: Vec<Stage>,
    pub samples: Vec<Sample>,
    pub accepted_summary: Option<Summary>,
    pub paired_gain: Option<f64>,
    pub noise_margin: Option<f64>,
    pub after_hook_failed: bool,
    pub started_unix_ms: u64,
    pub finished_unix_ms: u64,
}
#[derive(Debug, Serialize)]
pub struct Evaluation {
    pub report: EvaluationReport,
    pub sha256: String,
    pub already_applied: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Active {
    format_version: u32,
    token: String,
    operation_id: String,
}

impl SessionStore {
    /// Enqueue cancellation without taking the writer lock held by the supervisor.
    pub fn request_stop(&self, session_id: &str, operation_id: &str) -> Result<String> {
        session::identifier(session_id)?;
        session::identifier(operation_id)?;
        let path = self.session_path(session_id)?;
        let active: Active =
            serde_json::from_slice(&session::read_bounded(&path.join("active.json"), 4096)?)
                .map_err(|_| error("recovery_required", "invalid active execution marker"))?;
        session::identifier(&active.token)?;
        if active.format_version != 1 {
            return Err(error(
                "recovery_required",
                "unsupported active execution marker",
            ));
        }
        let requests = path.join("cancellations");
        workspace::owned_directory(&requests)?;
        let receipt = requests.join(format!("{}.json", session::hash(operation_id.as_bytes())));
        let mut request = Active {
            format_version: 1,
            token: active.token.clone(),
            operation_id: operation_id.into(),
        };
        match session::write_new(&receipt, &serde_json::to_vec(&request).unwrap()) {
            Ok(()) => session::sync_directory(&requests)?,
            Err(issue) => {
                if !receipt.try_exists()? {
                    return Err(issue);
                }
                request = serde_json::from_slice(&session::read_bounded(&receipt, 4096)?)
                    .map_err(|_| error("recovery_required", "invalid cancellation receipt"))?;
                if request.format_version != 1 || request.operation_id != operation_id {
                    return Err(error("conflict", "cancellation operation ID differs"));
                }
                session::identifier(&request.token)?;
            }
        }
        // A retry remains bound to the original execution, even if a new one is running.
        if request.token == active.token {
            session::atomic_replace(
                &path.join("cancel.json"),
                &serde_json::to_vec(&request).unwrap(),
            )?;
        }
        Ok(request.token)
    }
}

impl SessionGuard {
    pub fn baseline(&mut self, operation: &str) -> Result<Evaluation> {
        self.execute_evaluation(operation, None)
    }
    pub fn evaluate_candidate(&mut self, operation: &str, candidate: &str) -> Result<Evaluation> {
        session::identifier(candidate)?;
        self.execute_evaluation(operation, Some(format!("sealed-{candidate}")))
    }
    pub fn evaluation(&self, key: &str) -> Result<Evaluation> {
        session::identifier(key)?;
        let bytes = session::read_bounded(
            &self
                .owned_path()
                .join("executions")
                .join(key)
                .join("report.json"),
            REPORT_LIMIT,
        )?;
        let digest = session::hash(&bytes);
        if self.state().artifacts.get(key) != Some(&digest) {
            return Err(error(
                "corrupt_artifact",
                "evaluation fingerprint differs from the journal",
            ));
        }
        let report: EvaluationReport = serde_json::from_slice(&bytes)
            .map_err(|_| error("corrupt_artifact", "invalid evaluation report"))?;
        if report.format_version != 1 || report.config_sha256 != self.state().config_sha256 {
            return Err(error(
                "corrupt_artifact",
                "evaluation belongs to another contract",
            ));
        }
        Ok(Evaluation {
            report,
            sha256: digest,
            already_applied: true,
        })
    }
    fn finish_control(&mut self, token: &str, decision: Decision) -> Result<()> {
        let path = self.owned_path().join("active.json");
        if !path.try_exists()? {
            return Ok(());
        }
        let active: Active = serde_json::from_slice(&session::read_bounded(&path, 4096)?)
            .map_err(|_| error("recovery_required", "invalid active execution marker"))?;
        if active.token != token {
            return Ok(());
        }
        if decision == Decision::Cancelled {
            self.stop(&format!("stop-{token}"), supervisor::now_ms()?)?;
        }
        fs::remove_file(path)?;
        session::sync_directory(self.owned_path())
    }
    fn execute_evaluation(
        &mut self,
        operation: &str,
        candidate: Option<String>,
    ) -> Result<Evaluation> {
        session::identifier(operation)?;
        let token = session::hash(operation.as_bytes());
        let key = format!("run-{token}");
        if self.state().artifacts.contains_key(&key) {
            let result = self.evaluation(&key)?;
            if result.report.operation_id != operation || result.report.candidate != candidate {
                return Err(error(
                    "conflict",
                    "operation ID refers to another evaluation",
                ));
            }
            self.finish_execution(
                &format!("finish-{token}"),
                &key,
                &result.sha256,
                result.report.decision,
                invalidates_reference(&result.report),
                supervisor::now_ms()?,
            )?;
            self.finish_control(&token, result.report.decision)?;
            return Ok(result);
        }
        supervisor::check_recovery(&self.owned_path().join("process.json"))?;
        let parent = self
            .state()
            .accepted
            .clone()
            .unwrap_or_else(|| "workspace".into());
        let (base, base_hash, base_path) = self.load_artifact(&parent)?;
        let base_files = workspace::verify_snapshot(&base_path, &base.files, self.config())?;
        let (candidate_hash, candidate_files) = if let Some(candidate) = &candidate {
            let (sealed, digest, path) = self.load_artifact(candidate)?;
            if sealed.base_sha256.as_ref() != Some(&base_hash) {
                return Err(error(
                    "conflict",
                    "candidate was sealed against a different accepted reference",
                ));
            }
            workspace::check_scope(&base.files, &sealed.files, self.config())?;
            (
                Some(digest),
                Some(workspace::verify_snapshot(
                    &path,
                    &sealed.files,
                    self.config(),
                )?),
            )
        } else {
            (None, None)
        };
        let run_parent = self.owned_path().join("executions");
        workspace::owned_directory(&run_parent)?;
        let path = run_parent.join(&key);
        // Complete evidence written before the final journal event can be reconciled.
        if path.join("report.json").symlink_metadata().is_ok() {
            let bytes = session::read_bounded(&path.join("report.json"), REPORT_LIMIT)?;
            let report: EvaluationReport = serde_json::from_slice(&bytes)
                .map_err(|_| error("corrupt_artifact", "invalid pending evaluation report"))?;
            if report.format_version != 1
                || report.operation_id != operation
                || report.candidate != candidate
                || report.parent != parent
                || report.candidate_sha256 != candidate_hash
                || report.parent_sha256 != base_hash
                || report.config_sha256 != self.state().config_sha256
                || self
                    .state()
                    .execution
                    .as_ref()
                    .map(|e| e.operation_id.as_str())
                    != Some(operation)
            {
                return Err(error(
                    "recovery_required",
                    "uncommitted evidence does not match the active execution",
                ));
            }
            let digest = session::hash(&bytes);
            self.finish_execution(
                &format!("finish-{token}"),
                &key,
                &digest,
                report.decision,
                invalidates_reference(&report),
                supervisor::now_ms()?,
            )?;
            self.finish_control(&token, report.decision)?;
            return Ok(Evaluation {
                report,
                sha256: digest,
                already_applied: true,
            });
        }
        let config = self.config().clone();
        if config.execution.network != NetworkPolicy::Allowed {
            return Err(error("unsupported","network=disabled requires an isolation backend; this runner supports only explicitly allowed, trusted local execution"));
        }
        if config.sampling.runs < 5 {
            return Err(error(
                "invalid_protocol",
                "execution requires at least five paired samples",
            ));
        }
        if config.sampling.cache.mode == CacheMode::Warm && config.sampling.warmup == 0 {
            return Err(error(
                "invalid_protocol",
                "warm cache mode requires warmups",
            ));
        }
        let environment = declared_environment(&config)?;
        let method = method_hash(&config, &environment)?;
        let previous = if candidate.is_some() {
            let qualification = self.state().qualification.as_ref().ok_or_else(|| {
                error(
                    "baseline_required",
                    "qualify the accepted reference before evaluating",
                )
            })?;
            let prior = self.evaluation(qualification)?.report;
            if prior.method_sha256 != method {
                return Err(error(
                    "baseline_stale",
                    "declared environment or executable changed; requalify the reference",
                ));
            }
            Some(prior.accepted_summary.ok_or_else(|| {
                error(
                    "baseline_required",
                    "reference has no qualified measurements",
                )
            })?)
        } else {
            None
        };
        if workspace::directory_bytes(self.owned_path())?.saturating_add(REPORT_RESERVE)
            >= config.budget.max_artifact_bytes
        {
            return Err(error(
                "storage_limit",
                "insufficient room for execution evidence",
            ));
        }
        let _benchmark_lock = session::acquire(
            &self
                .owned_path()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("benchmark.lock"),
            true,
        )?;
        let cancellation = Cancellation::install()?;
        let start = supervisor::now_ms()?;
        self.begin_execution(operation, candidate.clone(), parent.clone(), start)?;
        workspace::owned_directory(&path)?;
        workspace::owned_directory(&path.join("stages"))?;
        session::atomic_replace(
            &self.owned_path().join("active.json"),
            &serde_json::to_vec(&Active {
                format_version: 1,
                token: token.clone(),
                operation_id: operation.into(),
            })
            .unwrap(),
        )?;
        let report = EvaluationReport {
            format_version: 1,
            operation_id: operation.into(),
            candidate: candidate.clone(),
            parent,
            config_sha256: self.state().config_sha256.clone(),
            parent_sha256: base_hash,
            candidate_sha256: candidate_hash,
            method_sha256: method,
            decision: Decision::Failed,
            reason: "execution_incomplete".into(),
            stages: Vec::new(),
            samples: Vec::new(),
            accepted_summary: None,
            paired_gain: None,
            noise_margin: None,
            after_hook_failed: false,
            started_unix_ms: start,
            finished_unix_ms: start,
        };
        let mut engine = Engine {
            guard: self,
            config,
            environment,
            path,
            token,
            cancellation,
            report,
            next_stage: 0,
            reference: base_files,
            candidate: candidate_files,
        };
        let pipeline = engine.pipeline(previous.as_ref());
        if let Err(issue) = pipeline {
            engine.report.decision = match issue.code {
                "cancelled" => Decision::Cancelled,
                "budget_exhausted" => Decision::Inconclusive,
                "constraint_failed" => Decision::Discarded,
                _ => Decision::Failed,
            };
            engine.report.reason = issue.code.into();
            engine.report.accepted_summary = None;
        }
        // A still-owned or uncertain process must not be reconciled as a completed result.
        if engine
            .guard
            .owned_path()
            .join("process.json")
            .symlink_metadata()
            .is_ok()
        {
            return Err(error(
                "recovery_required",
                "process cleanup is incomplete; execution remains reserved",
            ));
        }
        if engine.guard.state().in_flight.is_some() {
            return Err(error(
                "recovery_required",
                "stage accounting is incomplete; resume before any new work",
            ));
        }
        // After-hook failures are evidence; they do not replace a completed verdict.
        let sides = if engine.candidate.is_some() {
            vec!["reference", "candidate"]
        } else {
            vec!["reference"]
        };
        let hooks = engine.config.execution.hooks.after.clone();
        for side in sides {
            for spec in &hooks {
                if engine.stage(side, "after", spec, 0).is_err() {
                    engine.report.after_hook_failed = true;
                    break;
                }
            }
        }
        // The immutable inputs are checked again before the atomic decision event.
        let (base, _, base_path) = engine.guard.load_artifact(&engine.report.parent)?;
        workspace::verify_snapshot(&base_path, &base.files, &engine.config)?;
        if let Some(key) = &candidate {
            let (sealed, _, path) = engine.guard.load_artifact(key)?;
            workspace::verify_snapshot(&path, &sealed.files, &engine.config)?;
        }
        if method_hash(&engine.config, &declared_environment(&engine.config)?)?
            != engine.report.method_sha256
        {
            engine.report.decision = Decision::Failed;
            engine.report.reason = "environment_changed".into();
            engine.report.accepted_summary = None;
        }
        if engine.cancelled()? {
            engine.report.decision = Decision::Cancelled;
            engine.report.reason = "cancelled".into();
            engine.report.accepted_summary = None;
        }
        if engine.guard.state().in_flight.is_some()
            || engine
                .guard
                .owned_path()
                .join("process.json")
                .symlink_metadata()
                .is_ok()
        {
            return Err(error(
                "recovery_required",
                "after-hook cleanup or accounting remains incomplete",
            ));
        }
        engine.report.finished_unix_ms = supervisor::now_ms()?;
        let bytes = serde_json::to_vec_pretty(&engine.report)
            .map_err(|_| error("invalid_metric", "cannot serialize finite evidence"))?;
        if bytes.len() > REPORT_LIMIT {
            return Err(error(
                "storage_limit",
                "execution report exceeds its reserved space",
            ));
        }
        session::sync_directory(&engine.path.join("stages"))?;
        session::write_new(&engine.path.join("report.json"), &bytes)?;
        session::sync_directory(&engine.path)?;
        let digest = session::hash(&bytes);
        engine.guard.finish_execution(
            &format!("finish-{}", engine.token),
            &key,
            &digest,
            engine.report.decision,
            invalidates_reference(&engine.report),
            engine.report.finished_unix_ms,
        )?;
        engine
            .guard
            .finish_control(&engine.token, engine.report.decision)?;
        Ok(Evaluation {
            report: engine.report,
            sha256: digest,
            already_applied: false,
        })
    }
}

fn invalidates_reference(report: &EvaluationReport) -> bool {
    matches!(
        report.reason.as_str(),
        "baseline_stale" | "reference_constraint_failed" | "environment_changed"
    )
}

struct Engine<'a> {
    guard: &'a mut SessionGuard,
    config: SessionConfig,
    environment: BTreeMap<String, String>,
    path: PathBuf,
    token: String,
    cancellation: Cancellation,
    report: EvaluationReport,
    next_stage: usize,
    reference: Contents,
    candidate: Option<Contents>,
}
impl Engine<'_> {
    fn cancelled(&self) -> Result<bool> {
        if self.cancellation.cancelled() {
            return Ok(true);
        }
        let path = self.guard.owned_path().join("cancel.json");
        match path.symlink_metadata() {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
            Ok(_) => {
                let request: Active = serde_json::from_slice(&session::read_bounded(&path, 4096)?)
                    .map_err(|_| error("corrupt_session", "invalid cancellation request"))?;
                Ok(request.format_version == 1 && request.token == self.token)
            }
        }
    }
    fn expected(&self, side: &str) -> &Contents {
        if side == "reference" {
            &self.reference
        } else {
            self.candidate.as_ref().expect("candidate side exists")
        }
    }
    fn pipeline(&mut self, previous: Option<&Summary>) -> Result<()> {
        let sides = if self.candidate.is_some() {
            vec!["reference", "candidate"]
        } else {
            vec!["reference"]
        };
        for side in &sides {
            workspace::write_tree(&self.path.join(side), self.expected(side))?;
            workspace::owned_directory(&self.path.join(format!("{side}-home")))?;
            workspace::owned_directory(&self.path.join(format!("{side}-tmp")))?;
            for (label, specs) in [
                ("setup", self.config.execution.setup.clone()),
                ("before", self.config.execution.hooks.before.clone()),
                ("check", self.config.checks.clone()),
            ] {
                for spec in specs {
                    self.stage(side, label, &spec, 0)?;
                }
            }
        }
        if let Some(previous) = previous {
            let (reference, candidate) = self.pairs("exploration")?;
            let reference_stats = measurement::summarize(&reference)?;
            if reference_stats.noise > self.config.metric.minimum_improvement
                || (reference_stats.median - previous.median).abs()
                    > previous.noise.max(self.config.metric.minimum_improvement)
            {
                return Err(error(
                    "baseline_stale",
                    "reference measurements drifted; requalify before accepting a result",
                ));
            }
            let margin = previous.noise.max(reference_stats.noise);
            let gain =
                measurement::paired_gain(&reference, &candidate, self.config.metric.direction)?;
            self.report.paired_gain = Some(gain);
            self.report.noise_margin = Some(margin);
            if gain <= self.config.metric.minimum_improvement + margin {
                self.report.decision = if gain < -margin {
                    Decision::Discarded
                } else {
                    Decision::Inconclusive
                };
                self.report.reason = "gain_below_required_margin".into();
                return Ok(());
            }
            // Exactly one predefined, distinct confirmation series; no optional stopping.
            let (reference, candidate) = self.pairs("confirmation")?;
            let reference_stats = measurement::summarize(&reference)?;
            let candidate_stats = measurement::summarize(&candidate)?;
            let margin = margin.max(reference_stats.noise);
            let gain =
                measurement::paired_gain(&reference, &candidate, self.config.metric.direction)?;
            self.report.paired_gain = Some(gain);
            self.report.noise_margin = Some(margin);
            if reference_stats.noise > self.config.metric.minimum_improvement
                || (reference_stats.median - previous.median).abs()
                    > previous.noise.max(self.config.metric.minimum_improvement)
            {
                return Err(error(
                    "baseline_stale",
                    "reference drifted during confirmation",
                ));
            }
            if candidate_stats.noise > self.config.metric.minimum_improvement
                || gain <= self.config.metric.minimum_improvement + margin
            {
                self.report.decision = Decision::Inconclusive;
                self.report.reason = "confirmation_failed".into();
                return Ok(());
            }
            for spec in self.config.checks.clone() {
                self.stage("candidate", "final_check", &spec, 0)?;
            }
            self.report.accepted_summary = Some(candidate_stats);
            self.report.decision = Decision::Kept;
            self.report.reason = "checks_and_independent_confirmation_passed".into();
        } else {
            let mut values = Vec::new();
            for round in 0..self.config.sampling.baseline_rounds {
                self.warmup("reference", round * self.config.sampling.runs)?;
                for sample in 0..self.config.sampling.runs {
                    let index = round * self.config.sampling.runs + sample;
                    values.push(self.measure("qualification", "reference", index)?);
                }
            }
            let summary = measurement::summarize(&values)?;
            if summary.noise > self.config.metric.minimum_improvement {
                self.report.decision = Decision::Inconclusive;
                self.report.reason = "reference_noise_exceeds_useful_improvement".into();
            } else {
                self.report.accepted_summary = Some(summary);
                self.report.decision = Decision::Qualified;
                self.report.reason = "reference_checks_and_stability_passed".into();
            }
        }
        Ok(())
    }
    fn warmup(&mut self, side: &str, offset: u32) -> Result<()> {
        for index in 0..self.config.sampling.warmup {
            self.measure("warmup", side, offset + index)?;
        }
        Ok(())
    }
    fn pairs(&mut self, phase: &str) -> Result<(Vec<f64>, Vec<f64>)> {
        self.warmup("reference", 0)?;
        self.warmup("candidate", 0)?;
        let mut reference = Vec::new();
        let mut candidate = Vec::new();
        for index in 0..self.config.sampling.runs {
            if index % 2 == 0 {
                reference.push(self.measure(phase, "reference", index)?);
                candidate.push(self.measure(phase, "candidate", index)?);
            } else {
                candidate.push(self.measure(phase, "candidate", index)?);
                reference.push(self.measure(phase, "reference", index)?);
            }
        }
        Ok((reference, candidate))
    }
    fn measure(&mut self, phase: &str, side: &str, index: u32) -> Result<f64> {
        if self.config.sampling.cache.mode == CacheMode::Cold {
            // Validate the owned tree before deleting any declared private cache path.
            self.check_tree(side)?;
            for path in &self.config.sampling.cache.paths {
                let path = self.path.join(side).join(path);
                match path.symlink_metadata() {
                    Ok(meta) if meta.is_dir() => fs::remove_dir_all(path)?,
                    Ok(meta) if meta.is_file() => fs::remove_file(path)?,
                    Ok(_) => {
                        return Err(error(
                            "unsafe_path",
                            "cache path is not owned regular storage",
                        ))
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                    Err(e) => return Err(e.into()),
                }
            }
        }
        let spec = self.config.benchmark.clone();
        let output = self.stage(side, phase, &spec, index)?;
        let metrics = measurement::parse(&output, &self.config)?;
        let valid = measurement::constraints(&metrics, &self.config);
        let primary = metrics[&self.config.metric.name];
        self.report.samples.push(Sample {
            phase: phase.into(),
            side: side.into(),
            index,
            seed: self.seed(index),
            metrics,
        });
        if !valid {
            return Err(error(
                if side == "candidate" {
                    "constraint_failed"
                } else {
                    "reference_constraint_failed"
                },
                "a secondary constraint was exceeded",
            ));
        }
        Ok(primary)
    }
    fn seed(&self, index: u32) -> Option<u64> {
        if self.config.sampling.seeds.is_empty() {
            None
        } else {
            Some(self.config.sampling.seeds[index as usize % self.config.sampling.seeds.len()])
        }
    }
    fn check_tree(&self, side: &str) -> Result<()> {
        let files = workspace::scan(&self.path.join(side), &self.config, true)?;
        if workspace::inventory(&files) != workspace::inventory(self.expected(side)) {
            return Err(error(
                "integrity_failed",
                "an executed command modified code outside generated paths",
            ));
        }
        Ok(())
    }
    fn stage(&mut self, side: &str, name: &str, spec: &CommandSpec, index: u32) -> Result<Vec<u8>> {
        if self.cancelled()? {
            return Err(error("cancelled", "stop was requested"));
        }
        self.check_tree(side)?;
        let now = supervisor::now_ms()?;
        let number = self.next_stage;
        self.next_stage += 1;
        let reservation = format!("stage-{}-{number}", self.token);
        let root = self.path.join(side);
        let cwd = working_directory(&root, &spec.cwd)?;
        let mut environment = self.environment.clone();
        environment.insert(
            "HOME".into(),
            self.path
                .join(format!("{side}-home"))
                .to_string_lossy()
                .into(),
        );
        environment.insert(
            "TMPDIR".into(),
            self.path
                .join(format!("{side}-tmp"))
                .to_string_lossy()
                .into(),
        );
        environment.insert("AUTORESEARCH_SAMPLE_INDEX".into(), index.to_string());
        environment.insert(
            "AUTORESEARCH_INPUT_SHA256".into(),
            self.config.sampling.input_sha256.clone(),
        );
        if let Some(seed) = self.seed(index) {
            environment.insert("AUTORESEARCH_SEED".into(), seed.to_string());
        }
        let mut spec = spec.clone();
        spec.executable = resolve_executable(&spec.executable, &cwd, &environment)?
            .to_string_lossy()
            .into();
        let used = workspace::directory_bytes(self.guard.owned_path())?;
        let remaining = self
            .config
            .budget
            .max_artifact_bytes
            .saturating_sub(used)
            .saturating_sub(REPORT_RESERVE);
        if remaining == 0 {
            return Err(error(
                "storage_limit",
                "no storage remains for stage output",
            ));
        }
        let reserved = self.guard.reserve_stage(&reservation, now)?;
        let marker = self.guard.owned_path().join("process.json");
        let output = supervisor::run(
            &spec,
            &cwd,
            &environment,
            supervisor::Limits {
                timeout_ms: reserved,
                max_output_bytes: self.config.budget.max_output_bytes.min(remaining),
            },
            &marker,
            &reservation,
            || {
                if workspace::directory_bytes(self.guard.owned_path())?
                    > self
                        .config
                        .budget
                        .max_artifact_bytes
                        .saturating_sub(REPORT_RESERVE)
                {
                    return Err(error("storage_limit", "generated storage limit exceeded"));
                }
                Ok(self.cancelled()?
                    || supervisor::now_ms()? >= self.config.budget.deadline_unix_ms)
            },
        )?;
        if output.report.outcome == ProcessOutcome::CleanupIncomplete {
            return Err(error(
                "recovery_required",
                "owned process cleanup could not be confirmed",
            ));
        }
        self.guard.settle_stage(
            &format!("settle-{}-{number}", self.token),
            &reservation,
            output.report.elapsed_ms,
            supervisor::now_ms()?,
        )?;
        let stdout = format!("stages/{number:04}.stdout");
        let stderr = format!("stages/{number:04}.stderr");
        session::write_new(&self.path.join(&stdout), &output.stdout)?;
        session::write_new(&self.path.join(&stderr), &output.stderr)?;
        let outcome = output.report.outcome.clone();
        self.report.stages.push(Stage {
            name: name.into(),
            side: side.into(),
            stdout,
            stderr,
            process: output.report,
        });
        self.check_tree(side)?;
        match outcome {
            ProcessOutcome::Passed => Ok(output.stdout),
            ProcessOutcome::Cancelled => Err(error("cancelled", "command cancelled")),
            ProcessOutcome::TimedOut => Err(error(
                "command_timeout",
                "command exceeded its reserved timeout",
            )),
            ProcessOutcome::OutputLimit | ProcessOutcome::StorageLimit => Err(error(
                "storage_limit",
                "command exceeded output or storage limit",
            )),
            _ => Err(error(
                "command_failed",
                "command did not complete successfully",
            )),
        }
    }
}
fn declared_environment(config: &SessionConfig) -> Result<BTreeMap<String, String>> {
    let mut env = config.execution.environment.set.clone();
    for name in &config.execution.environment.inherit {
        env.insert(
            name.clone(),
            std::env::var(name).map_err(|_| {
                error(
                    "invalid_environment",
                    "declared inherited variable is absent or not UTF-8",
                )
            })?,
        );
    }
    if env
        .keys()
        .any(|name| name == "HOME" || name == "TMPDIR" || name.starts_with("AUTORESEARCH_"))
    {
        return Err(error(
            "invalid_environment",
            "HOME, TMPDIR and AUTORESEARCH_* are reserved private execution variables",
        ));
    }
    Ok(env)
}
fn method_hash(config: &SessionConfig, environment: &BTreeMap<String, String>) -> Result<String> {
    let mut tools = BTreeMap::new();
    for spec in config
        .checks
        .iter()
        .chain(std::iter::once(&config.benchmark))
        .chain(&config.execution.setup)
        .chain(&config.execution.hooks.before)
        .chain(&config.execution.hooks.after)
    {
        if spec.executable.contains('/') && !Path::new(&spec.executable).is_absolute() {
            continue;
        }
        let path = resolve_executable(&spec.executable, Path::new("/"), environment)?;
        if tools.contains_key(&path.to_string_lossy().into_owned()) {
            continue;
        }
        let mut file = fs::File::open(&path)?;
        let mut hash = Sha256::new();
        let mut bytes = [0_u8; 65536];
        loop {
            let n = file.read(&mut bytes)?;
            if n == 0 {
                break;
            }
            hash.update(&bytes[..n]);
        }
        tools.insert(
            path.to_string_lossy().into_owned(),
            format!("{:x}", hash.finalize()),
        );
    }
    Ok(session::hash(
        &serde_json::to_vec(&(
            config,
            environment,
            tools,
            std::env::consts::OS,
            std::env::consts::ARCH,
        ))
        .unwrap(),
    ))
}
fn working_directory(root: &Path, relative: &str) -> Result<PathBuf> {
    let mut path = root.to_path_buf();
    if relative != "." {
        for part in relative.trim_end_matches('/').split('/') {
            path.push(part);
            session::check_directory(&path)?;
        }
    }
    session::check_directory(&path)?;
    Ok(path)
}
fn resolve_executable(
    name: &str,
    cwd: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<PathBuf> {
    let candidates: Vec<PathBuf> = if name.contains('/') {
        vec![cwd.join(name)]
    } else {
        let path = environment
            .get("PATH")
            .ok_or_else(|| error("invalid_environment", "declare PATH for executable lookup"))?;
        std::env::split_paths(path)
            .map(|dir| {
                if dir.is_absolute() {
                    dir.join(name)
                } else {
                    cwd.join(dir).join(name)
                }
            })
            .collect()
    };
    for path in candidates {
        if let Ok(meta) = path.metadata() {
            use std::os::unix::fs::PermissionsExt;
            if meta.is_file() && meta.permissions().mode() & 0o111 != 0 {
                return path.canonicalize().map_err(Into::into);
            }
        }
    }
    Err(error(
        "command_failed",
        "declared executable is unavailable",
    ))
}
fn error(code: &'static str, message: &str) -> SessionError {
    SessionError::new(code, message)
}
