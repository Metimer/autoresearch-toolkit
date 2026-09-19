//! Selective result bundles. Preview and export share the exact same inventory.
use crate::{
    session::{self, SessionError, SessionGuard},
    workspace::{self, Contents, FileEntry, Inventory},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};
type Result<T> = std::result::Result<T, SessionError>;
const LIMIT: usize = 128 * 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub code: bool,
    pub protocol: bool,
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleManifest {
    pub format_version: u32,
    pub evidence_sha256: String,
    pub selection: Selection,
    pub files: Inventory,
}

#[derive(Debug, Serialize)]
pub struct Preview {
    pub manifest: BundleManifest,
    pub sha256: String,
    pub total_bytes: u64,
    pub report: Value,
}

pub struct PreparedResult {
    files: Contents,
    preview: Preview,
}

#[derive(Debug, Serialize)]
pub struct ExportedResult {
    pub path: PathBuf,
    pub sha256: String,
    pub already_applied: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    format_version: u32,
    operation: String,
    output: PathBuf,
    bundle_sha256: String,
}

impl PreparedResult {
    pub fn preview(&self) -> &Preview {
        &self.preview
    }
}

fn add(files: &mut Contents, path: &str, bytes: Vec<u8>) {
    files.insert(
        path.into(),
        (
            FileEntry {
                sha256: session::hash(&bytes),
                bytes: bytes.len() as u64,
                mode: 0o100644,
            },
            bytes,
        ),
    );
}
fn add_json(files: &mut Contents, path: &str, value: &impl Serialize) {
    add(files, path, serde_json::to_vec_pretty(value).unwrap());
}
fn tree_hash(files: &Contents) -> String {
    session::hash(&serde_json::to_vec(&workspace::inventory(files)).unwrap())
}
fn markdown(value: &str) -> String {
    value
        .chars()
        .flat_map(|c| match c {
            '<' => "&lt;".chars().collect::<Vec<_>>(),
            '>' => "&gt;".chars().collect(),
            '&' => "&amp;".chars().collect(),
            '\n' | '\r' => vec![' '],
            '\\' | '`' | '*' | '_' | '[' | ']' | '|' | '#' => vec!['\\', c],
            _ => vec![c],
        })
        .collect()
}

impl SessionGuard {
    pub fn prepare_result(
        &self,
        evaluation: &str,
        mut selection: Selection,
    ) -> Result<PreparedResult> {
        if !evaluation.starts_with("run-") {
            return Err(SessionError::new(
                "report_not_found",
                "select a native evaluation from history",
            ));
        }
        selection.logs.sort();
        if selection.logs.len() > 1024 || selection.logs.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(SessionError::new(
                "invalid_protocol",
                "select at most 1024 distinct stage logs",
            ));
        }
        let evidence = self.evaluation(evaluation)?;
        let report = &evidence.report;
        let (parent_manifest, parent_digest, _) = self.load_artifact(&report.parent)?;
        if parent_digest != report.parent_sha256 {
            return Err(SessionError::new(
                "corrupt_artifact",
                "report reference mismatch",
            ));
        }
        let parent = self.snapshot(&report.parent)?;
        let candidate = if let Some(key) = &report.candidate {
            let (_, digest, _) = self.load_artifact(key)?;
            if Some(digest) != report.candidate_sha256 {
                return Err(SessionError::new(
                    "corrupt_artifact",
                    "report candidate mismatch",
                ));
            }
            self.snapshot(key)?
        } else {
            parent.clone()
        };
        workspace::check_scope(
            &parent_manifest.files,
            &workspace::inventory(&candidate),
            self.config(),
        )?;
        let base = self.snapshot("workspace")?;
        let samples: Vec<Value> = report
            .samples
            .iter()
            .map(|s| {
                json!({
                    "phase": s.phase, "side": s.side, "index": s.index, "metrics": s.metrics,
                })
            })
            .collect();
        let shared = json!({
            "format_version": 1, "trust": "engine_recorded", "evidence_sha256": evidence.sha256,
            "config_sha256": report.config_sha256, "method_sha256": report.method_sha256,
            "protocol_sha256": report.protocol_sha256,
            "decision": report.decision, "reason": report.reason, "metric": self.config().metric,
            "secondary_constraints": self.config().secondary_constraints,
            "samples": samples, "accepted_summary": report.accepted_summary,
            "paired_gain": report.paired_gain, "noise_margin": report.noise_margin,
            "after_hook_failed": report.after_hook_failed,
            "command_timeout_seconds": self.config().budget.command_timeout_seconds,
            "max_output_bytes": self.config().budget.max_output_bytes,
            "max_artifact_bytes": self.config().budget.max_artifact_bytes,
            "active_seconds_budget": self.config().budget.active_seconds,
            "base_tree_sha256": tree_hash(&base), "reference_tree_sha256": tree_hash(&parent),
            "candidate_tree_sha256": tree_hash(&candidate),
            "limitations": ["Recorded measurements are not a guarantee of future performance.",
                "Exploration and confirmation are separate phases; this is not a confidence interval.",
                "The trusted-process runner is not a sandbox; external and transitive inputs are not captured.",
                "Integrity hashes are not a remote signature or independent attestation."]
        });
        let mut files = Contents::new();
        add_json(&mut files, "report.json", &shared);
        let mut text = format!("# Optimization result\n\nDecision: **{}**. Reason: {}.\n\nMetric: {} ({}).\n\nEvidence SHA-256: `{}`.\n\n## Measurements\n\nExploration and confirmation are reported separately below. Only the engine's recorded verdict determines acceptance.\n\n| Phase | Side | Sample | Primary value |\n| --- | --- | ---: | ---: |\n",
            shared["decision"].as_str().unwrap(), markdown(&report.reason), markdown(&self.config().metric.name), markdown(&self.config().metric.unit), evidence.sha256);
        for sample in &report.samples {
            text.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                markdown(&sample.phase),
                markdown(&sample.side),
                sample.index,
                sample
                    .metrics
                    .get(&self.config().metric.name)
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "missing".into())
            ));
        }
        text.push_str("\n## Limits\n\nMeasurements describe this recorded run, not a future-performance guarantee or a confidence interval. External dependencies are not bundled. Hashes verify integrity, not independent authenticity. See report.json for noise, constraints-related verdicts and selected evidence; REPRODUCE.md describes the reproduction steps.\n");
        add(&mut files, "report.md", text.into_bytes());
        if selection.code {
            for (prefix, tree) in [("base", &base), ("reference", &parent)] {
                for (path, value) in tree {
                    files.insert(format!("{prefix}/{path}"), value.clone());
                }
            }
            add(
                &mut files,
                "candidate.patch",
                workspace::verified_patch(&base, &candidate)?,
            );
            add_json(
                &mut files,
                "candidate-files.json",
                &workspace::inventory(&candidate),
            );
        }
        if selection.protocol {
            let mut config = self.config().clone();
            config.session_id = "reproduction".into();
            config.source.repository = ".".into();
            config.source.commit = "0000000000000000000000000000000000000000".into();
            config.goal = "Reproduce the selected recorded evaluation".into();
            // Commands and environment are explicitly selected private evidence.
            // Preserve them faithfully; do not pretend arbitrary argv is sanitizable.
            add_json(&mut files, "protocol.template.json", &config);
        }
        for selected in &selection.logs {
            if selected.split('/').count() != 2
                || !selected.starts_with("stages/")
                || Path::new(selected)
                    .components()
                    .any(|p| !matches!(p, Component::Normal(_)))
            {
                return Err(SessionError::new(
                    "unsafe_path",
                    "select a stages/<file> path from the evaluation report",
                ));
            }
            let (digest, length) = report
                .stages
                .iter()
                .find_map(|stage| {
                    if &stage.stdout == selected {
                        Some((&stage.process.stdout_sha256, stage.process.stdout_bytes))
                    } else if &stage.stderr == selected {
                        Some((&stage.process.stderr_sha256, stage.process.stderr_bytes))
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    SessionError::new(
                        "report_not_found",
                        "selected log is not evidence of this evaluation",
                    )
                })?;
            let used = files.values().map(|(entry, _)| entry.bytes).sum::<u64>();
            if used.saturating_add(length as u64)
                > self.config().budget.max_artifact_bytes.min(LIMIT as u64)
            {
                return Err(SessionError::new(
                    "storage_limit",
                    "selected logs exceed the result bundle budget",
                ));
            }
            let mut directory = self.owned_path().to_path_buf();
            for part in ["executions", evaluation, "stages"] {
                directory.push(part);
                session::check_directory(&directory)?;
            }
            let bytes = session::read_bounded(
                &directory.join(selected.strip_prefix("stages/").unwrap()),
                length.min(64 * 1024 * 1024),
            )?;
            if session::hash(&bytes) != *digest {
                return Err(SessionError::new(
                    "corrupt_artifact",
                    "selected stage log differs from recorded evidence",
                ));
            }
            add(
                &mut files,
                &format!("logs/{}", selected.strip_prefix("stages/").unwrap()),
                bytes,
            );
        }
        add(&mut files, "REPRODUCE.md", REPRODUCE.as_bytes().to_vec());
        let manifest = BundleManifest {
            format_version: 1,
            evidence_sha256: evidence.sha256,
            selection,
            files: workspace::inventory(&files),
        };
        let bytes = serde_json::to_vec_pretty(&manifest).unwrap();
        let sha256 = session::hash(&bytes);
        add(&mut files, "manifest.json", bytes);
        let total_bytes = files.values().map(|(entry, _)| entry.bytes).sum::<u64>();
        if total_bytes > self.config().budget.max_artifact_bytes.min(LIMIT as u64)
            || files.len() > 16384
        {
            return Err(SessionError::new(
                "storage_limit",
                "result bundle exceeds artifact budget or bundle limits",
            ));
        }
        Ok(PreparedResult {
            files,
            preview: Preview {
                manifest,
                sha256,
                total_bytes,
                report: shared,
            },
        })
    }

    pub fn export_result(
        &mut self,
        operation: &str,
        evaluation: &str,
        selection: Selection,
        output: &Path,
        now: u64,
    ) -> Result<ExportedResult> {
        session::identifier(operation)?;
        if now < self.state().last_event_unix_ms {
            return Err(SessionError::new(
                "clock_regressed",
                "clock moved backwards",
            ));
        }
        let key = format!("result-{}", session::hash(operation.as_bytes()));
        let recorded = self.artifact_operation(operation, &key)?;
        if recorded.is_none() && self.state().artifacts.len() >= 256 {
            return Err(SessionError::new(
                "storage_limit",
                "session artifact count limit reached",
            ));
        }
        let prepared = self.prepare_result(evaluation, selection)?;
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .canonicalize()?;
        let output = parent.join(
            output
                .file_name()
                .ok_or_else(|| SessionError::new("unsafe_path", "output must name a directory"))?,
        );
        if output.to_str().is_none() {
            return Err(SessionError::new(
                "unsafe_path",
                "result destination must be UTF-8",
            ));
        }
        if output.starts_with(self.source_root()?)
            || output.components().any(|p| {
                p.as_os_str().to_str().is_some_and(|s| {
                    s.eq_ignore_ascii_case(".git") || s.eq_ignore_ascii_case(".auto")
                })
            })
        {
            return Err(SessionError::new(
                "unsafe_path",
                "result must be outside source and engine/Git metadata",
            ));
        }
        let receipt = Receipt {
            format_version: 1,
            operation: operation.into(),
            output: output.clone(),
            bundle_sha256: prepared.preview.sha256.clone(),
        };
        let bytes = serde_json::to_vec(&receipt).unwrap();
        let digest = session::hash(&bytes);
        if recorded.as_ref().is_some_and(|old| old != &digest) {
            return Err(SessionError::new(
                "conflict",
                "operation ID belongs to another result selection or destination",
            ));
        }
        let receipts = self.owned_path().join("results");
        workspace::owned_directory(&receipts)?;
        let receipt_path = receipts.join(format!("{key}.json"));
        if receipt_path.symlink_metadata().is_ok() {
            if session::read_bounded(&receipt_path, 16384)? != bytes {
                return Err(SessionError::new(
                    "conflict",
                    "result receipt differs from request",
                ));
            }
        } else if recorded.is_some() {
            return Err(SessionError::new(
                "corrupt_artifact",
                "recorded result receipt is missing",
            ));
        }
        if !receipt_path.exists()
            && workspace::directory_bytes(self.owned_path())?.saturating_add(bytes.len() as u64)
                > self.config().budget.max_artifact_bytes
        {
            return Err(SessionError::new(
                "storage_limit",
                "no space for result receipt",
            ));
        }
        if output.symlink_metadata().is_ok() {
            verify_files(&output, &prepared.files)?;
        } else {
            if recorded.is_some() {
                return Err(SessionError::new(
                    "corrupt_artifact",
                    "recorded result bundle is missing",
                ));
            }
            let staging = tempfile::Builder::new()
                .prefix(".result-")
                .tempdir_in(&parent)?;
            workspace::write_tree(&staging.path().join("bundle"), &prepared.files)?;
            workspace::sync_tree(&staging.path().join("bundle"))?;
            workspace::publish_directory(&staging.path().join("bundle"), &output)?;
            session::sync_directory(&parent)?;
        }
        if !receipt_path.exists() {
            session::write_new(&receipt_path, &bytes)?;
            session::sync_directory(&receipts)?;
        }
        self.publish_artifact(operation, &key, &digest, now)?;
        Ok(ExportedResult {
            path: output,
            sha256: prepared.preview.sha256,
            already_applied: recorded.is_some(),
        })
    }
}

fn verify_files(root: &Path, expected: &Contents) -> Result<()> {
    session::check_directory(root)?;
    if workspace::directory_bytes(root)? > LIMIT as u64 {
        return Err(SessionError::new(
            "storage_limit",
            "result directory exceeds 128 MiB",
        ));
    }
    let mut paths = BTreeMap::new();
    fn walk(
        root: &Path,
        relative: &Path,
        paths: &mut BTreeMap<String, PathBuf>,
        depth: usize,
    ) -> Result<()> {
        if depth > 70 || paths.len() > 16384 {
            return Err(SessionError::new(
                "storage_limit",
                "result inventory exceeds limits",
            ));
        }
        session::check_directory(&root.join(relative))?;
        for entry in fs::read_dir(root.join(relative))? {
            let entry = entry?;
            let path = relative.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                walk(root, &path, paths, depth + 1)?;
            } else {
                paths.insert(
                    path.to_str()
                        .ok_or_else(|| {
                            SessionError::new("unsafe_path", "result path is not UTF-8")
                        })?
                        .into(),
                    root.join(path),
                );
            }
        }
        Ok(())
    }
    walk(root, Path::new(""), &mut paths, 0)?;
    if paths.len() != expected.len() {
        return Err(SessionError::new(
            "conflict",
            "destination inventory differs from result",
        ));
    }
    for (name, (entry, _)) in expected {
        let path = paths
            .get(name)
            .ok_or_else(|| SessionError::new("corrupt_artifact", "result file is missing"))?;
        let bytes = session::read_bounded(path, entry.bytes.min(LIMIT as u64) as usize)?;
        if bytes.len() as u64 != entry.bytes || session::hash(&bytes) != entry.sha256 {
            return Err(SessionError::new(
                "corrupt_artifact",
                "result file differs from inventory",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if (fs::metadata(path)?.permissions().mode() & 0o111 != 0) != (entry.mode == 0o100755) {
                return Err(SessionError::new(
                    "corrupt_artifact",
                    "result executable mode differs",
                ));
            }
        }
    }
    Ok(())
}

const REPRODUCE: &str = "# Reproduce a recorded result\n\nDefault exports contain reports only. Code, protocol and raw logs require explicit selection. No command is executed by preview or export. Inspect manifest.json before sharing. Metric labels and measurements are included; no automatic secret detector is promised.\n\n## Reconstruct code (when code was selected)\n\nCopy base/ into a new directory outside this bundle. In that copy run:\n\n```sh\ngit -c core.autocrlf=false init --template=\ngit -c core.autocrlf=false add --all\ngit -c core.autocrlf=false apply --index --binary /absolute/path/to/bundle/candidate.patch\n```\n\nSkip apply for an empty patch. candidate-files.json records the expected SHA-256, size and executable mode of every resulting file. The exporter already verified the binary patch against the sealed candidate in an independent Git index. reference/ contains the actual comparison reference; it may differ from the initial base after earlier accepted trials. Empty directories and other filesystem metadata are not captured.\n\n## Repeat measurements\n\nIf selected, protocol.template.json preserves commands, budgets, environment and inputs declarations, including potentially private argument/environment values. Source location, session ID, goal and commit are replaced. Review all commands; provision the declared toolchain, external dependencies and inputs yourself. Initialize and commit a fresh repository containing reference/, set source.repository and source.commit accordingly, choose an explicit new deadline/budget and unused session ID, then run autoresearch init, workspace and baseline. Prepare a candidate, replace its tree with the reconstructed candidate, seal and evaluate. Do not reuse the recorded verdict without this new qualification and confirmation. Without a selected protocol, obtain the measurement contract separately. No implicit network access, installation or experiment is performed by opening this bundle.\n";
