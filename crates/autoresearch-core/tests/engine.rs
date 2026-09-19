use autoresearch_core::{
    config::{CommandSpec, NetworkPolicy},
    session::{Decision, SessionGuard, SessionStore},
    workspace::LocalChanges,
    SessionConfig, ValidatedConfig,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    config: ValidatedConfig,
}
fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Metimer")
        .env("GIT_AUTHOR_EMAIL", "metinamerwane@gmail.com")
        .env("GIT_COMMITTER_NAME", "Metimer")
        .env("GIT_COMMITTER_EMAIL", "metinamerwane@gmail.com")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout).unwrap().trim().into()
}
impl Fixture {
    fn new(bench: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("pilot");
        let source = temp.path().join("source");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&source).unwrap();
        fs::create_dir(source.join("src")).unwrap();
        fs::write(source.join("src/value"), b"10").unwrap();
        fs::write(
            source.join("check.py"),
            b"from pathlib import Path\nassert int(Path('src/value').read_text()) > 0\n",
        )
        .unwrap();
        fs::write(source.join("bench.py"), bench).unwrap();
        git(&source, &["init", "--template="]);
        git(&source, &["add", "--all"]);
        git(&source, &["commit", "-m", "engine fixture"]);
        let mut config: SessionConfig =
            serde_json::from_str(include_str!("../../../examples/session.json")).unwrap();
        config.session_id = "test".into();
        config.source.repository = source.to_string_lossy().into();
        config.source.commit = git(&source, &["rev-parse", "HEAD"]);
        config.budget.deadline_unix_ms = 4_000_000_000_000;
        config.scope.protected_paths = vec!["check.py".into(), "bench.py".into()];
        config.execution.network = NetworkPolicy::Allowed;
        config.checks = vec![CommandSpec {
            executable: "python3".into(),
            args: vec!["check.py".into()],
            cwd: ".".into(),
        }];
        config.benchmark = CommandSpec {
            executable: "python3".into(),
            args: vec!["bench.py".into()],
            cwd: ".".into(),
        };
        config.sampling.warmup = 0;
        config.sampling.runs = 5;
        config.sampling.baseline_rounds = 3;
        Self {
            _temp: temp,
            root,
            config: ValidatedConfig::try_from(config).unwrap(),
        }
    }
    fn session(&self) -> SessionGuard {
        let mut session = SessionStore::new(&self.root)
            .unwrap()
            .init(self.config.clone(), "init", 1)
            .unwrap();
        session
            .create_workspace("workspace", LocalChanges::Exclude, 2)
            .unwrap();
        session
    }
    fn reopen(&self) -> SessionGuard {
        SessionStore::new(&self.root)
            .unwrap()
            .open("test", false)
            .unwrap()
    }
}
const BENCH:&str="import json\nfrom pathlib import Path\nvalue=int(Path('src/value').read_text())\nprint('METRIC '+json.dumps(dict(name='bench_ms',value=value,unit='ms')))\n";

#[test]
fn hooks_receive_versioned_context_and_cannot_supply_a_verdict() {
    let mut f = Fixture::new(BENCH);
    let mut config = f.config.get().clone();
    let hook = CommandSpec {
        executable: "/bin/sh".into(),
        args: vec!["-c".into(), "printf '%s:%s:%s:%s:%s:%s\\n' \"$AUTORESEARCH_HOOK_VERSION\" \"$AUTORESEARCH_HOOK_PHASE\" \"$AUTORESEARCH_HOOK_SIDE\" \"$AUTORESEARCH_SESSION_ID\" \"$AUTORESEARCH_OPERATION_ID\" \"$AUTORESEARCH_DECISION\"; printf 'METRIC {\"name\":\"bench_ms\",\"value\":0,\"unit\":\"ms\"}\\n'; test \"$AUTORESEARCH_HOOK_SIDE\" != candidate".into()],
        cwd: ".".into(),
    };
    config.execution.hooks.before.push(hook.clone());
    config.execution.hooks.after.push(hook);
    f.config = ValidatedConfig::try_from(config).unwrap();
    let mut session = f.session();
    let baseline = session.baseline("baseline").unwrap();
    assert_eq!(baseline.report.decision, Decision::Qualified);
    assert!(baseline
        .report
        .samples
        .iter()
        .all(|s| s.metrics["bench_ms"] == 10.0));
    candidate(&mut session, "fast", "5");
    let result = session.evaluate_candidate("trial", "fast").unwrap();
    assert_eq!(result.report.decision, Decision::Failed);
    assert!(result.report.after_hook_failed);
    assert_eq!(session.state().accepted.as_deref(), Some("workspace"));
    let stages = f.root.join(".auto/engine/sessions/test/executions");
    let mut output = String::new();
    for execution in fs::read_dir(stages).unwrap() {
        for stage in fs::read_dir(execution.unwrap().path().join("stages")).unwrap() {
            let path = stage.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "stdout") {
                output.push_str(&fs::read_to_string(path).unwrap());
            }
        }
    }
    assert!(output.contains("1:before:reference:test:baseline:pending"));
    assert!(output.contains("1:after:reference:test:baseline:qualified"));
    assert!(output.contains("1:before:candidate:test:trial:pending"));
    assert!(output.contains("1:after:candidate:test:trial:failed"));
}

#[test]
fn direct_hook_scripts_must_be_protected_before_snapshot_publication() {
    let mut f = Fixture::new(BENCH);
    let mut config = f.config.get().clone();
    config
        .scope
        .protected_paths
        .retain(|path| path != "check.py");
    config.execution.hooks.before.push(CommandSpec {
        executable: "python3".into(),
        args: vec!["./check.py".into()],
        cwd: ".".into(),
    });
    f.config = ValidatedConfig::try_from(config).unwrap();
    let mut session = SessionStore::new(&f.root)
        .unwrap()
        .init(f.config.clone(), "init", 1)
        .unwrap();
    let error = session
        .create_workspace("workspace", LocalChanges::Exclude, 2)
        .unwrap_err();
    assert_eq!(error.code, "scope_violation");
    assert!(session.state().artifacts.is_empty());
}

#[test]
fn before_hook_timeout_blocks_sampling_and_charges_the_shared_budget() {
    let mut f = Fixture::new(BENCH);
    let mut config = f.config.get().clone();
    config.budget.command_timeout_seconds = 1;
    config.execution.hooks.before.push(CommandSpec {
        executable: "/bin/sh".into(),
        args: vec!["-c".into(), "sleep 30 & wait".into()],
        cwd: ".".into(),
    });
    f.config = ValidatedConfig::try_from(config).unwrap();
    let mut session = f.session();
    let result = session.baseline("baseline").unwrap();
    assert_eq!(result.report.decision, Decision::Failed);
    assert_eq!(result.report.reason, "command_timeout");
    assert!(result.report.samples.is_empty());
    assert!(session.state().active_ms_used >= 1000);
    assert!(session.state().qualification.is_none());
    assert!(!f
        .root
        .join(".auto/engine/sessions/test/process.json")
        .exists());
}
fn candidate(session: &mut SessionGuard, id: &str, value: &str) {
    let now = autoresearch_core::supervisor::now_ms().unwrap();
    let prepared = session
        .prepare_candidate(&format!("prepare-{id}"), id, "reduce work", now)
        .unwrap();
    fs::write(prepared.path.join("src/value"), value).unwrap();
    session
        .seal_candidate(&format!("seal-{id}"), id, now + 1)
        .unwrap();
}
#[test]
fn qualify_compare_confirm_promote_and_replay_without_rerunning() {
    let f = Fixture::new(BENCH);
    let mut session = f.session();
    let baseline = session.baseline("baseline").unwrap();
    assert_eq!(
        baseline.report.decision,
        Decision::Qualified,
        "{}",
        baseline.report.reason
    );
    assert_eq!(session.state().attempts_used, 0);
    assert_eq!(baseline.report.samples.len(), 15);
    assert!(session.state().active_ms_used > 0);
    candidate(&mut session, "fast", "5");
    let improved = session.evaluate_candidate("evaluate-fast", "fast").unwrap();
    assert_eq!(
        improved.report.decision,
        Decision::Kept,
        "{}",
        improved.report.reason
    );
    assert_eq!(session.state().accepted.as_deref(), Some("sealed-fast"));
    assert_eq!(session.state().attempts_used, 1);
    assert_eq!(
        improved
            .report
            .samples
            .iter()
            .filter(|s| s.phase == "confirmation")
            .count(),
        10
    );
    let used = session.state().active_ms_used;
    let repeat = session.evaluate_candidate("evaluate-fast", "fast").unwrap();
    assert!(repeat.already_applied);
    assert_eq!(session.state().active_ms_used, used);
    assert!(session
        .evaluate_candidate("evaluate-fast-again", "fast")
        .is_err());
    drop(session);
    let mut session = f.reopen();
    let prepared = session
        .prepare_candidate(
            "prepare-next",
            "next",
            "new accepted base",
            autoresearch_core::supervisor::now_ms().unwrap(),
        )
        .unwrap();
    assert_eq!(fs::read(prepared.path.join("src/value")).unwrap(), b"5");
    fs::write(prepared.path.join("src/value"), b"15").unwrap();
    session
        .seal_candidate(
            "seal-next",
            "next",
            autoresearch_core::supervisor::now_ms().unwrap(),
        )
        .unwrap();
    let regression = session.evaluate_candidate("evaluate-next", "next").unwrap();
    assert_eq!(regression.report.decision, Decision::Discarded);
    assert_eq!(session.state().accepted.as_deref(), Some("sealed-fast"));
    assert_eq!(session.state().attempts_used, 2);
}
#[test]
fn noisy_baselines_are_not_qualified_and_network_policy_fails_closed() {
    let f=Fixture::new("import json,os\nv=10+10*(int(os.environ['AUTORESEARCH_SAMPLE_INDEX'])%2)\nprint('METRIC '+json.dumps(dict(name='bench_ms',value=v,unit='ms')))\n");
    let mut session = f.session();
    let result = session.baseline("baseline").unwrap();
    assert_eq!(result.report.decision, Decision::Inconclusive);
    assert!(session.state().qualification.is_none());
    candidate(&mut session, "fast", "5");
    assert!(session.evaluate_candidate("run", "fast").is_err());
    let mut f = Fixture::new(BENCH);
    let mut config = f.config.get().clone();
    config.execution.network = NetworkPolicy::Disabled;
    f.config = ValidatedConfig::try_from(config).unwrap();
    let mut session = f.session();
    assert!(session.baseline("baseline").is_err());
    assert_eq!(session.state().active_ms_used, 0);
}
#[test]
fn failed_checks_and_modified_code_cannot_be_kept() {
    let f = Fixture::new(BENCH);
    let mut session = f.session();
    session.baseline("baseline").unwrap();
    candidate(&mut session, "invalid", "0");
    let result = session
        .evaluate_candidate("run-invalid", "invalid")
        .unwrap();
    assert_eq!(result.report.decision, Decision::Failed);
    assert_eq!(session.state().accepted.as_deref(), Some("workspace"));
    let f=Fixture::new("from pathlib import Path\nPath('src/value').write_text('1')\nprint('METRIC {\"name\":\"bench_ms\",\"value\":1,\"unit\":\"ms\"}')\n");
    let mut session = f.session();
    let result = session.baseline("baseline").unwrap();
    assert_eq!(result.report.decision, Decision::Failed);
    assert_eq!(result.report.reason, "integrity_failed");
    assert!(session.state().qualification.is_none());
}
#[test]
fn one_attempt_covers_all_stages_and_exhaustion_blocks_the_next_trial() {
    let mut f = Fixture::new(BENCH);
    let mut config = f.config.get().clone();
    config.budget.max_experiments = 1;
    f.config = ValidatedConfig::try_from(config).unwrap();
    let mut session = f.session();
    session.baseline("baseline").unwrap();
    candidate(&mut session, "fast", "5");
    let result = session.evaluate_candidate("run-fast", "fast").unwrap();
    assert_eq!(result.report.decision, Decision::Kept);
    assert_eq!(session.state().attempts_used, 1);
    candidate(&mut session, "faster", "2");
    assert!(session.evaluate_candidate("run-faster", "faster").is_err());
    assert_eq!(session.state().attempts_used, 1);
}

#[test]
fn confirmation_is_required_and_reference_drift_invalidates_qualification() {
    // Each runtime has its own generated counter, reset when an evaluation starts.
    let bench = "import json\nfrom pathlib import Path\np=Path('build/count');p.parent.mkdir(exist_ok=True)\nn=int(p.read_text())+1 if p.exists() else 1;p.write_text(str(n))\nv=int(Path('src/value').read_text())\nif v==5 and n>5: v=15\nprint('METRIC '+json.dumps(dict(name='bench_ms',value=v,unit='ms')))\n";
    let f = Fixture::new(bench);
    let mut session = f.session();
    session.baseline("baseline").unwrap();
    candidate(&mut session, "unstable", "5");
    let result = session.evaluate_candidate("trial", "unstable").unwrap();
    assert_eq!(result.report.decision, Decision::Inconclusive);
    assert_eq!(result.report.reason, "confirmation_failed");
    assert_eq!(session.state().accepted.as_deref(), Some("workspace"));

    // A trusted external workload change is observed through the reference samples.
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("input");
    fs::write(&input, "10").unwrap();
    let bench = format!("import json\nfrom pathlib import Path\nv=int(Path({:?}).read_text())\nprint('METRIC '+json.dumps(dict(name='bench_ms',value=v,unit='ms')))\n", input.to_str().unwrap());
    let f = Fixture::new(&bench);
    let mut session = f.session();
    session.baseline("baseline").unwrap();
    candidate(&mut session, "drift", "5");
    fs::write(&input, "20").unwrap();
    let result = session.evaluate_candidate("trial", "drift").unwrap();
    assert_eq!(result.report.reason, "baseline_stale");
    assert!(session.state().qualification.is_none());
    assert_eq!(session.state().accepted.as_deref(), Some("workspace"));
}

#[test]
fn secondary_constraints_block_promotion_and_after_hook_failure_keeps_the_verdict() {
    use autoresearch_core::config::{ConstraintBound, MetricDomain, SecondaryConstraint};
    let mut f = Fixture::new(&format!(
        "{BENCH}\nprint('METRIC '+json.dumps(dict(name='memory',value=100/value,unit='MB')))\n"
    ));
    let mut config = f.config.get().clone();
    config.secondary_constraints.push(SecondaryConstraint {
        name: "memory".into(),
        unit: "MB".into(),
        domain: MetricDomain::Positive,
        bound: ConstraintBound::AtMost { value: 15.0 },
    });
    f.config = ValidatedConfig::try_from(config).unwrap();
    let mut session = f.session();
    session.baseline("baseline").unwrap();
    candidate(&mut session, "memory", "5");
    let result = session.evaluate_candidate("trial", "memory").unwrap();
    assert_eq!(result.report.decision, Decision::Discarded);
    assert_eq!(result.report.reason, "constraint_failed");
    assert_eq!(session.state().accepted.as_deref(), Some("workspace"));

    let mut f = Fixture::new(BENCH);
    let mut config = f.config.get().clone();
    config.execution.hooks.after.push(CommandSpec {
        executable: "/bin/sh".into(),
        args: vec!["-c".into(), "exit 1".into()],
        cwd: ".".into(),
    });
    f.config = ValidatedConfig::try_from(config).unwrap();
    let mut session = f.session();
    assert!(
        session
            .baseline("baseline")
            .unwrap()
            .report
            .after_hook_failed
    );
    candidate(&mut session, "fast", "5");
    let result = session.evaluate_candidate("trial", "fast").unwrap();
    assert!(result.report.after_hook_failed);
    assert_eq!(result.report.decision, Decision::Kept);
}

#[test]
fn durable_report_can_finish_promotion_without_reexecuting_commands() {
    let f = Fixture::new(BENCH);
    let mut session = f.session();
    session.baseline("baseline").unwrap();
    candidate(&mut session, "fast", "5");
    let first = session.evaluate_candidate("trial", "fast").unwrap();
    let used = session.state().active_ms_used;
    drop(session);
    let path = f.root.join(".auto/engine/sessions/test");
    // Reproduce the durable boundary: report exists, completion event not appended yet.
    let bytes = fs::read(path.join("events.jsonl")).unwrap();
    let end = bytes[..bytes.len() - 1]
        .iter()
        .rposition(|b| *b == b'\n')
        .unwrap()
        + 1;
    fs::write(path.join("events.jsonl"), &bytes[..end]).unwrap();
    fs::remove_file(path.join("state.json")).unwrap();
    let mut session = f.reopen();
    assert!(
        session
            .view(autoresearch_core::supervisor::now_ms().unwrap())
            .recovery_required
    );
    let replay = session.evaluate_candidate("trial", "fast").unwrap();
    assert!(replay.already_applied);
    assert_eq!(first.sha256, replay.sha256);
    assert_eq!(session.state().active_ms_used, used);
    assert_eq!(session.state().accepted.as_deref(), Some("sealed-fast"));
    assert!(session.state().execution.is_none());
    drop(session);
    fs::remove_file(path.join("state.json")).unwrap();
    let mut session = f.reopen();
    assert!(
        !session
            .view(autoresearch_core::supervisor::now_ms().unwrap())
            .projection_current
    );
    session.evaluate_candidate("trial", "fast").unwrap();
    assert!(
        session
            .view(autoresearch_core::supervisor::now_ms().unwrap())
            .projection_current
    );
    let exported = session
        .export_candidate(
            "export",
            "fast",
            &f._temp.path().join("result"),
            autoresearch_core::supervisor::now_ms().unwrap(),
        )
        .unwrap();
    assert!(exported.path.join("candidate.patch").exists());
}

#[test]
fn cancellation_request_stops_the_live_evaluation_and_preserves_accounting() {
    let f = Fixture::new("import time\ntime.sleep(5)\n");
    let mut session = f.session();
    let root = f.root.clone();
    let stopper = std::thread::spawn(move || {
        let start = std::time::Instant::now();
        while !root
            .join(".auto/engine/sessions/test/process.json")
            .exists()
        {
            assert!(start.elapsed() < std::time::Duration::from_secs(10));
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        SessionStore::new(&root)
            .unwrap()
            .request_stop("test", "user-stop")
            .unwrap();
    });
    let result = session.baseline("baseline").unwrap();
    stopper.join().unwrap();
    assert_eq!(result.report.decision, Decision::Cancelled);
    assert_eq!(
        session.state().status,
        autoresearch_core::session::SessionStatus::Stopped
    );
    assert!(session.state().active_ms_used > 0);
    assert!(session.state().execution.is_none());
    assert!(session.state().in_flight.is_none());
    assert!(!f
        .root
        .join(".auto/engine/sessions/test/process.json")
        .exists());
}

#[test]
fn private_cache_policy_excludes_warmups_and_preserves_the_seed_schedule() {
    use autoresearch_core::config::CacheMode;
    let bench = "import json\nfrom pathlib import Path\np=Path('build/cache/value');p.parent.mkdir(parents=True,exist_ok=True)\nv=1 if p.exists() else 100;p.write_text('cached')\nprint('METRIC '+json.dumps(dict(name='bench_ms',value=v,unit='ms')))\n";
    for mode in [CacheMode::Cold, CacheMode::Warm] {
        let mut f = Fixture::new(bench);
        let mut config = f.config.get().clone();
        config.sampling.cache.mode = mode;
        config.sampling.cache.paths = vec!["build/cache/".into()];
        config.sampling.warmup = 1;
        f.config = ValidatedConfig::try_from(config).unwrap();
        let mut session = f.session();
        let result = session.baseline("baseline").unwrap();
        assert_eq!(result.report.decision, Decision::Qualified);
        assert_eq!(result.report.samples.len(), 18);
        let summary = result.report.accepted_summary.unwrap();
        assert_eq!(summary.count, 15);
        assert_eq!(
            summary.median,
            if mode == CacheMode::Cold { 100.0 } else { 1.0 }
        );
        assert!(result
            .report
            .samples
            .iter()
            .all(|sample| sample.seed == Some(42)));
    }
}

#[test]
fn command_timeout_consumes_remaining_time_and_blocks_a_fresh_launch() {
    let mut f = Fixture::new("import time\ntime.sleep(2)\n");
    let mut config = f.config.get().clone();
    config.budget.active_seconds = 1;
    config.budget.command_timeout_seconds = 1;
    f.config = ValidatedConfig::try_from(config).unwrap();
    let mut session = f.session();
    let result = session.baseline("baseline").unwrap();
    assert_eq!(result.report.decision, Decision::Failed);
    assert_eq!(result.report.reason, "command_timeout");
    assert!(session.state().active_ms_used >= 1000);
    assert!(session.state().qualification.is_none());
    assert_eq!(
        session.baseline("new-baseline").unwrap_err().code,
        "budget_exhausted"
    );
}

#[test]
fn recovery_never_kills_a_live_group_named_in_a_persistent_marker() {
    use std::os::unix::process::CommandExt;
    let f = Fixture::new(BENCH);
    let mut session = f.session();
    let mut child = Command::new("/bin/sleep")
        .arg("10")
        .process_group(0)
        .spawn()
        .unwrap();
    let marker = f.root.join(".auto/engine/sessions/test/process.json");
    fs::write(&marker, serde_json::to_vec(&serde_json::json!({
        "format_version":1, "token":"abandoned", "supervisor_pid":u32::MAX, "group_pid":child.id()
    })).unwrap()).unwrap();
    let result = session.resume("resume", autoresearch_core::supervisor::now_ms().unwrap());
    let alive = child.try_wait().unwrap().is_none();
    child.kill().unwrap();
    child.wait().unwrap();
    assert_eq!(result.unwrap_err().code, "recovery_required");
    assert!(alive);
    session
        .resume("resume", autoresearch_core::supervisor::now_ms().unwrap())
        .unwrap();
    assert!(!marker.exists());
}

#[test]
fn a_stop_retry_does_not_cancel_a_different_execution() {
    let f = Fixture::new(BENCH);
    let _session = f.session();
    let path = f.root.join(".auto/engine/sessions/test");
    let store = SessionStore::new(&f.root).unwrap();
    let active = |token: &str| {
        fs::write(
            path.join("active.json"),
            serde_json::to_vec(&serde_json::json!({
                "format_version":1, "token":token, "operation_id":token
            }))
            .unwrap(),
        )
        .unwrap();
    };
    active("original");
    assert_eq!(
        store.request_stop("test", "stop-request").unwrap(),
        "original"
    );
    active("subsequent");
    assert_eq!(
        store.request_stop("test", "stop-request").unwrap(),
        "original"
    );
    let request: serde_json::Value =
        serde_json::from_slice(&fs::read(path.join("cancel.json")).unwrap()).unwrap();
    assert_eq!(request["token"], "original");
    assert_eq!(
        store.request_stop("test", "new-stop-request").unwrap(),
        "subsequent"
    );
}

#[test]
fn historical_keep_cannot_qualify_or_promote_code_without_new_measurements() {
    use autoresearch_core::legacy::{LegacyFormat, PreparedImport};
    let f = Fixture::new(BENCH);
    let prepared = PreparedImport::parse(
        include_bytes!("../../../tests/fixtures/legacy/pi.jsonl"),
        LegacyFormat::Auto,
    )
    .unwrap();
    let store = SessionStore::new(&f.root).unwrap();
    let mut session = store
        .import_legacy(f.config.clone(), "import", &prepared, 1)
        .unwrap();
    assert!(session.state().accepted.is_none());
    session
        .create_workspace("workspace", LocalChanges::Exclude, 2)
        .unwrap();
    candidate(&mut session, "fast", "5");
    assert_eq!(
        session
            .evaluate_candidate("trial-before-baseline", "fast")
            .unwrap_err()
            .code,
        "baseline_required"
    );
    assert_eq!(session.state().attempts_used, 0);
    assert_eq!(session.state().active_ms_used, 0);
    assert_eq!(
        session.baseline("baseline").unwrap().report.decision,
        Decision::Qualified
    );
    assert_eq!(
        session
            .evaluate_candidate("trial", "fast")
            .unwrap()
            .report
            .decision,
        Decision::Kept
    );
    assert_eq!(session.state().attempts_used, 1);
    assert_eq!(
        session.legacy_report().unwrap().unwrap().entries[1].trust,
        "historical_unverified"
    );
}

#[test]
fn resuming_after_timeout_keeps_charged_time_and_requires_explicit_new_work() {
    let temp = tempfile::tempdir().unwrap();
    let ready = temp.path().join("ready");
    let bench = format!(
        "import time\nfrom pathlib import Path\nif not Path({:?}).exists(): time.sleep(2)\n{BENCH}",
        ready.to_str().unwrap()
    );
    let mut f = Fixture::new(&bench);
    let mut config = f.config.get().clone();
    config.budget.command_timeout_seconds = 1;
    f.config = ValidatedConfig::try_from(config).unwrap();
    let mut session = f.session();
    let result = session.baseline("timeout").unwrap();
    assert_eq!(result.report.reason, "command_timeout");
    let used = session.state().active_ms_used;
    let deadline = session.config().budget.deadline_unix_ms;
    session
        .stop("stop", autoresearch_core::supervisor::now_ms().unwrap())
        .unwrap();
    drop(session);
    let mut session = f.reopen();
    session
        .resume("resume", autoresearch_core::supervisor::now_ms().unwrap())
        .unwrap();
    assert_eq!(session.state().active_ms_used, used);
    assert_eq!(session.config().budget.deadline_unix_ms, deadline);
    assert!(session.state().qualification.is_none());
    assert_eq!(
        session.baseline("timeout").unwrap().report.reason,
        "command_timeout"
    );
    assert_eq!(session.state().active_ms_used, used);
    fs::write(ready, "ready").unwrap();
    assert_eq!(
        session.baseline("new-baseline").unwrap().report.decision,
        Decision::Qualified
    );
    assert!(session.state().active_ms_used > used);
}
