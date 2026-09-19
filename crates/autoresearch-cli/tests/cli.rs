use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        Self::named("autoresearch-cli")
    }
    fn named(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn cli(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_autoresearch"));
        command.current_dir(&self.0);
        command
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn body(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn validation_is_read_only_and_does_not_execute_declared_commands() {
    let fixture = Fixture::new();
    let mut config: Value =
        serde_json::from_str(include_str!("../../../examples/session.json")).unwrap();
    config["benchmark"] =
        json!({"executable": "/this-command-does-not-exist", "args": [], "cwd": "."});
    config["source"]["repository"] = json!("/repository-does-not-exist");
    let bytes = serde_json::to_vec(&config).unwrap();
    let path = fixture.0.join("config with spaces.json");
    fs::write(&path, &bytes).unwrap();
    let result = fixture
        .cli()
        .arg("validate")
        .arg("--config")
        .arg(&path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result);
    assert_eq!(body(&result)["commands_executed"], false);
    assert_eq!(body(&result)["repository_verified"], false);
    assert_eq!(body(&result)["deadline_checked"], false);
    assert!(result.stderr.is_empty());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
}

#[test]
fn invalid_config_has_machine_readable_error_and_exit_two() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("invalid.json"), b"{\"private-value\": true}").unwrap();
    let result = fixture
        .cli()
        .args(["validate", "--config", "invalid.json", "--json"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert_eq!(body(&result)["error"]["code"], "invalid_config");
    assert!(!String::from_utf8_lossy(&result.stdout).contains("private-value"));
}

#[test]
fn missing_or_directory_config_has_io_error() {
    let fixture = Fixture::new();
    for path in ["missing.json", "."] {
        let result = fixture
            .cli()
            .args(["validate", "--config", path, "--json"])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        assert_eq!(body(&result)["error"]["code"], "io");
    }
}

#[test]
fn oversized_config_is_rejected() {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.join("large.json"),
        vec![b' '; autoresearch_core::MAX_CONFIG_BYTES + 1],
    )
    .unwrap();
    let result = fixture
        .cli()
        .args(["validate", "--config", "large.json", "--json"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
}

#[test]
fn doctor_reports_missing_git_and_unavailable_execution() {
    let fixture = Fixture::new();
    let result = fixture
        .cli()
        .env("PATH", &fixture.0)
        .args(["doctor", "--json"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(3));
    assert_eq!(body(&result)["git_on_path"], false);
    assert_eq!(body(&result)["capabilities"]["validate_config"], true);
    assert_eq!(body(&result)["capabilities"]["run_experiments"], false);
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 0);
}

#[test]
fn help_version_and_bad_arguments_have_stable_outputs() {
    let fixture = Fixture::new();
    for args in [["--help", "--json"], ["--version", "--json"]] {
        let result = fixture.cli().args(args).output().unwrap();
        assert!(result.status.success());
        assert_eq!(body(&result)["ok"], true);
    }
    for args in [
        vec!["run", "--json"],
        vec!["validate", "--json"],
        vec!["doctor", "extra", "--json"],
        vec!["doctor", "--json", "--json"],
    ] {
        let result = fixture.cli().args(args).output().unwrap();
        assert_eq!(result.status.code(), Some(2));
        assert_eq!(body(&result)["error"]["code"], "usage");
    }
}

#[cfg(unix)]
#[test]
fn symlinked_config_is_not_read() {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.join("real.json"),
        include_bytes!("../../../examples/session.json"),
    )
    .unwrap();
    std::os::unix::fs::symlink("real.json", fixture.0.join("link.json")).unwrap();
    let result = fixture
        .cli()
        .args(["validate", "--config", "link.json", "--json"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
}

fn session_config(fixture: &Fixture) -> String {
    let mut config: Value =
        serde_json::from_str(include_str!("../../../examples/session.json")).unwrap();
    config["budget"]["deadline_unix_ms"] = json!(4_000_000_000_000_u64);
    config["source"]["repository"] = json!("missing-repository");
    config["execution"]["hooks"]["before"] =
        json!([{"executable":"/this-must-not-run","args":[],"cwd":"."}]);
    fs::write(
        fixture.0.join("config.json"),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    config["session_id"].as_str().unwrap().into()
}

#[test]
fn separate_cli_processes_initialize_resume_stop_and_replay_without_executing() {
    let fixture = Fixture::new();
    let id = session_config(&fixture);
    let root = fixture.0.join("project with spaces");
    fs::create_dir(&root).unwrap();
    let run = |args: &[&str]| {
        fixture
            .cli()
            .args(args)
            .arg("--root")
            .arg(&root)
            .arg("--json")
            .output()
            .unwrap()
    };
    let first = run(&[
        "init",
        "--config",
        "config.json",
        "--operation-id",
        "create-1",
    ]);
    assert!(first.status.success(), "{first:?}");
    assert_eq!(body(&first)["session"]["state"]["status"], "created");
    assert_eq!(body(&first)["commands_executed"], false);
    assert!(!fixture.0.join(".auto").exists());
    let journal = root
        .join(".auto/engine/sessions")
        .join(&id)
        .join("events.jsonl");
    let initialized = fs::read(&journal).unwrap();
    assert!(run(&[
        "init",
        "--config",
        "config.json",
        "--operation-id",
        "create-1"
    ])
    .status
    .success());
    assert_eq!(fs::read(&journal).unwrap(), initialized);
    for (command, operation, expected) in [
        ("resume", "r1", "active"),
        ("stop", "s1", "stopped"),
        ("resume", "r2", "active"),
    ] {
        let args = [command, "--session", &id, "--operation-id", operation];
        let result = run(&args);
        assert!(result.status.success(), "{result:?}");
        assert_eq!(body(&result)["session"]["state"]["status"], expected);
        assert_eq!(body(&result)["already_applied"], false);
        let bytes = fs::read(&journal).unwrap();
        let repeated = run(&args);
        assert!(repeated.status.success(), "{repeated:?}");
        assert_eq!(body(&repeated)["already_applied"], true);
        assert_eq!(fs::read(&journal).unwrap(), bytes);
        let status = run(&["status", "--session", &id]);
        assert!(status.status.success());
        assert_eq!(body(&status)["session"]["state"]["status"], expected);
        assert_eq!(fs::read(&journal).unwrap(), bytes);
    }
    let conflict = run(&["stop", "--session", &id, "--operation-id", "r2"]);
    assert_eq!(conflict.status.code(), Some(4));
    assert_eq!(body(&conflict)["error"]["code"], "conflict");
}

#[test]
fn session_usage_expiry_and_recovery_errors_have_stable_codes() {
    let fixture = Fixture::new();
    let id = session_config(&fixture);
    for args in [
        vec!["init", "--config", "config.json"],
        vec!["status", "--session", &id, "--repair-tail"],
        vec![
            "resume",
            "--session",
            &id,
            "--operation-id",
            "x",
            "--operation-id",
            "x",
        ],
        vec!["stop", "--session", &id, "--operation-id", "bad/id"],
        vec![
            "init",
            "--config",
            "config.json",
            "--operation-id",
            "x",
            "--root",
            ".",
            "--root",
            ".",
        ],
    ] {
        let result = fixture.cli().args(args).arg("--json").output().unwrap();
        assert_eq!(result.status.code(), Some(2));
        assert!(!fixture.0.join(".auto").exists());
    }
    let mut config: Value =
        serde_json::from_slice(&fs::read(fixture.0.join("config.json")).unwrap()).unwrap();
    config["budget"]["deadline_unix_ms"] = json!(1);
    fs::write(
        fixture.0.join("expired.json"),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    let expired = fixture
        .cli()
        .args([
            "init",
            "--config",
            "expired.json",
            "--operation-id",
            "create",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(expired.status.code(), Some(6));
    assert_eq!(body(&expired)["error"]["code"], "budget_exhausted");
    let init = fixture
        .cli()
        .args([
            "init",
            "--config",
            "config.json",
            "--operation-id",
            "create",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(init.status.success());
    let journal = fixture
        .0
        .join(".auto/engine/sessions")
        .join(&id)
        .join("events.jsonl");
    let mut bytes = fs::read(&journal).unwrap();
    bytes.extend_from_slice(b"{");
    fs::write(&journal, &bytes).unwrap();
    let status = fixture
        .cli()
        .args(["status", "--session", &id, "--json"])
        .output()
        .unwrap();
    assert_eq!(status.status.code(), Some(5));
    assert_eq!(body(&status)["error"]["code"], "corrupt_session");
    let resumed = fixture
        .cli()
        .args([
            "resume",
            "--session",
            &id,
            "--operation-id",
            "repair",
            "--repair-tail",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(resumed.status.success(), "{resumed:?}");
    assert_eq!(body(&resumed)["session"]["state"]["status"], "active");
}

#[test]
fn schema_command_outputs_the_checked_in_schema() {
    let fixture = Fixture::new();
    let output = fixture.cli().arg("schema").output().unwrap();
    assert!(output.status.success());
    let expected: Value =
        serde_json::from_str(include_str!("../../../schemas/session-v2.schema.json")).unwrap();
    assert_eq!(body(&output), expected);
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 0);
}

fn source_fixture(fixture: &Fixture) -> String {
    let source = fixture.0.join("source");
    fs::create_dir(&source).unwrap();
    fs::create_dir(source.join("src")).unwrap();
    fs::write(source.join("src/main.txt"), b"baseline\n").unwrap();
    let run = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&source)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Metimer")
            .env("GIT_AUTHOR_EMAIL", "metinamerwane@gmail.com")
            .env("GIT_COMMITTER_NAME", "Metimer")
            .env("GIT_COMMITTER_EMAIL", "metinamerwane@gmail.com")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        output.stdout
    };
    run(&["init", "--template="]);
    run(&["add", "--all"]);
    run(&["commit", "-m", "source"]);
    let commit = String::from_utf8(run(&["rev-parse", "HEAD"])).unwrap();
    let mut config: Value =
        serde_json::from_str(include_str!("../../../examples/session.json")).unwrap();
    config["source"] = json!({"repository":"source", "commit":commit.trim()});
    config["budget"]["deadline_unix_ms"] = json!(4_000_000_000_000_u64);
    config["benchmark"]["executable"] = json!("/must-not-execute");
    fs::write(
        fixture.0.join("workspace.json"),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    config["session_id"].as_str().unwrap().into()
}

#[test]
fn workspace_cli_builds_seals_and_exports_an_unevaluated_candidate() {
    let fixture = Fixture::new();
    let id = source_fixture(&fixture);
    let run = |args: &[&str]| fixture.cli().args(args).arg("--json").output().unwrap();
    assert!(run(&[
        "init",
        "--config",
        "workspace.json",
        "--operation-id",
        "init"
    ])
    .status
    .success());
    let base = run(&[
        "workspace",
        "--session",
        &id,
        "--local-changes",
        "exclude",
        "--operation-id",
        "workspace",
    ]);
    assert!(base.status.success(), "{base:?}");
    assert_eq!(body(&base)["experiments_executed"], false);
    let prepared = run(&[
        "prepare-candidate",
        "--session",
        &id,
        "--candidate",
        "one",
        "--hypothesis",
        "less work",
        "--operation-id",
        "prepare",
    ]);
    assert!(prepared.status.success(), "{prepared:?}");
    let candidate = PathBuf::from(body(&prepared)["artifact"]["path"].as_str().unwrap());
    fs::write(candidate.join("src/main.txt"), b"candidate\n").unwrap();
    let sealed = run(&[
        "seal",
        "--session",
        &id,
        "--candidate",
        "one",
        "--operation-id",
        "seal",
    ]);
    assert!(sealed.status.success(), "{sealed:?}");
    let exported = run(&[
        "export-candidate",
        "--session",
        &id,
        "--candidate",
        "one",
        "--output",
        "bundle",
        "--operation-id",
        "export",
    ]);
    assert!(exported.status.success(), "{exported:?}");
    assert_eq!(body(&exported)["artifact"]["evaluated"], false);
    assert!(fixture.0.join("bundle/candidate.patch").is_file());
    assert!(fixture.0.join("bundle/base/src/main.txt").is_file());
    assert_eq!(
        fs::read(fixture.0.join("source/src/main.txt")).unwrap(),
        b"baseline\n"
    );
    let repeated = run(&[
        "export-candidate",
        "--session",
        &id,
        "--candidate",
        "one",
        "--output",
        "bundle",
        "--operation-id",
        "export",
    ]);
    assert!(repeated.status.success());
    assert_eq!(body(&repeated)["artifact"]["already_applied"], true);
    let status = run(&["status", "--session", &id]);
    assert_eq!(
        body(&status)["session"]["state"]["artifacts"]
            .as_object()
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn workspace_cli_requires_explicit_policy_and_rejects_invalid_options_and_scope() {
    let fixture = Fixture::new();
    let id = source_fixture(&fixture);
    for args in [
        vec!["workspace", "--session", &id, "--operation-id", "workspace"],
        vec![
            "workspace",
            "--session",
            &id,
            "--local-changes",
            "auto",
            "--operation-id",
            "workspace",
        ],
        vec!["seal", "--session", &id, "--operation-id", "seal"],
        vec![
            "prepare-candidate",
            "--session",
            &id,
            "--candidate",
            "one",
            "--operation-id",
            "prepare",
        ],
        vec![
            "export-candidate",
            "--session",
            &id,
            "--candidate",
            "one",
            "--operation-id",
            "export",
        ],
        vec!["status", "--session", &id, "--output", "bad"],
    ] {
        let output = fixture.cli().args(args).arg("--json").output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(!fixture.0.join(".auto").exists());
    }
    let run = |args: &[&str]| fixture.cli().args(args).arg("--json").output().unwrap();
    assert!(run(&[
        "init",
        "--config",
        "workspace.json",
        "--operation-id",
        "init"
    ])
    .status
    .success());
    assert!(run(&[
        "workspace",
        "--session",
        &id,
        "--local-changes",
        "exclude",
        "--operation-id",
        "workspace"
    ])
    .status
    .success());
    let candidate = run(&[
        "prepare-candidate",
        "--session",
        &id,
        "--candidate",
        "one",
        "--hypothesis",
        "scope",
        "--operation-id",
        "prepare",
    ]);
    assert!(candidate.status.success());
    let path = PathBuf::from(body(&candidate)["artifact"]["path"].as_str().unwrap());
    fs::write(path.join("outside.txt"), b"forbidden").unwrap();
    let sealed = run(&[
        "seal",
        "--session",
        &id,
        "--candidate",
        "one",
        "--operation-id",
        "seal",
    ]);
    assert_eq!(sealed.status.code(), Some(8));
    assert_eq!(body(&sealed)["error"]["code"], "scope_violation");
}

fn execution_fixture(fixture: &Fixture, slow: bool) -> String {
    let id = source_fixture(fixture);
    let path = fixture.0.join("workspace.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config["execution"]["network"] = json!("allowed");
    config["sampling"]["warmup"] = json!(0);
    config["checks"] =
        json!([{"executable":"/bin/sh", "args":["-c", "test -s src/main.txt"], "cwd":"."}]);
    let code = if slow {
        "sleep 10"
    } else {
        "v=10; if test \"$(cat src/main.txt)\" = fast; then v=5; fi; printf 'METRIC {\"name\":\"bench_ms\",\"value\":%s,\"unit\":\"ms\"}\\n' \"$v\""
    };
    config["benchmark"] = json!({"executable":"/bin/sh", "args":["-c",code], "cwd":"."});
    fs::write(path, serde_json::to_vec(&config).unwrap()).unwrap();
    for args in [
        vec![
            "init",
            "--config",
            "workspace.json",
            "--operation-id",
            "init",
        ],
        vec![
            "workspace",
            "--session",
            &id,
            "--local-changes",
            "exclude",
            "--operation-id",
            "workspace",
        ],
    ] {
        let out = fixture.cli().args(args).arg("--json").output().unwrap();
        assert!(out.status.success(), "{out:?}");
    }
    id
}

#[test]
fn standalone_cli_qualifies_evaluates_and_retries_a_sealed_candidate() {
    let fixture = Fixture::new();
    let id = execution_fixture(&fixture, false);
    let run = |args: &[&str]| {
        let out = fixture
            .cli()
            .args(args)
            .args(["--session", &id, "--json"])
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        body(&out)
    };
    let baseline = run(&["baseline", "--operation-id", "baseline"]);
    assert_eq!(baseline["evaluation"]["report"]["decision"], "qualified");
    let prepared = run(&[
        "prepare-candidate",
        "--candidate",
        "fast",
        "--hypothesis",
        "less work",
        "--operation-id",
        "prepare",
    ]);
    let path = PathBuf::from(prepared["artifact"]["path"].as_str().unwrap());
    fs::write(path.join("src/main.txt"), "fast").unwrap();
    run(&["seal", "--candidate", "fast", "--operation-id", "seal"]);
    let args = ["evaluate", "--candidate", "fast", "--operation-id", "trial"];
    let result = run(&args);
    assert_eq!(result["evaluation"]["report"]["decision"], "kept");
    assert_eq!(result["session"]["state"]["accepted"], "sealed-fast");
    assert_eq!(run(&args)["evaluation"]["already_applied"], true);
    assert_eq!(
        fs::read(fixture.0.join("source/src/main.txt")).unwrap(),
        b"baseline\n"
    );
}

#[test]
fn stop_and_sigterm_cancel_a_separate_running_cli_process() {
    use std::{
        process::Stdio,
        time::{Duration, Instant},
    };
    for signal in [false, true] {
        let fixture = Fixture::new();
        let id = execution_fixture(&fixture, true);
        let mut child = fixture
            .cli()
            .args([
                "baseline",
                "--session",
                &id,
                "--operation-id",
                "baseline",
                "--json",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let path = fixture.0.join(".auto/engine/sessions").join(&id);
        let start = Instant::now();
        while !path.join("process.json").exists() {
            if start.elapsed() > Duration::from_secs(10) {
                let _ = child.kill();
                let _ = child.wait();
                panic!("runner did not start");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if signal {
            assert!(Command::new("/bin/kill")
                .args(["-TERM", &child.id().to_string()])
                .status()
                .unwrap()
                .success());
        } else {
            let stop = fixture
                .cli()
                .args(["stop", "--session", &id, "--operation-id", "stop", "--json"])
                .output()
                .unwrap();
            assert!(stop.status.success(), "{stop:?}");
            assert_eq!(body(&stop)["cancellation_requested"], true);
        }
        while child.try_wait().unwrap().is_none() {
            if start.elapsed() > Duration::from_secs(10) {
                let _ = child.kill();
                let _ = child.wait();
                panic!("runner did not cancel");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let result = child.wait_with_output().unwrap();
        assert!(result.status.success(), "{result:?}");
        assert_eq!(
            body(&result)["evaluation"]["report"]["decision"],
            "cancelled"
        );
        assert_eq!(body(&result)["session"]["state"]["status"], "stopped");
        assert!(!path.join("process.json").exists());
    }
}

#[test]
fn reports_are_read_only_and_reject_unknown_or_corrupt_evidence() {
    let fixture = Fixture::new();
    let id = execution_fixture(&fixture, false);
    let run = |args: &[&str]| {
        fixture
            .cli()
            .args(args)
            .args(["--session", &id, "--json"])
            .output()
            .unwrap()
    };
    let history = run(&["history"]);
    assert!(history.status.success());
    assert_eq!(body(&history)["evaluations"], json!([]));
    let report = run(&["report"]);
    assert_eq!(report.status.code(), Some(4));
    assert_eq!(body(&report)["error"]["code"], "report_not_found");
    assert_eq!(body(&run(&["status"]))["next_action"], "baseline");
    assert!(run(&["baseline", "--operation-id", "baseline"])
        .status
        .success());
    let path = fixture.0.join(".auto/engine/sessions").join(&id);
    let before = fs::read(path.join("events.jsonl")).unwrap();
    let projection = fs::read(path.join("state.json")).unwrap();
    let history = body(&run(&["history"]));
    assert_eq!(history["evaluations"].as_array().unwrap().len(), 1);
    assert_eq!(history["commands_executed"], false);
    let key = history["evaluations"][0]["evaluation_key"]
        .as_str()
        .unwrap();
    let text_report = fixture
        .cli()
        .args(["report", "--session", &id])
        .output()
        .unwrap();
    assert!(text_report.status.success());
    let text_report = String::from_utf8(text_report.stdout).unwrap();
    assert!(text_report.contains("Accepted median: 10 ms"));
    assert!(text_report.contains("observations: 15"));
    let default_report = body(&run(&["report"]));
    let selected_report = body(&run(&["report", "--evaluation", key]));
    assert_eq!(default_report, selected_report);
    assert_eq!(
        selected_report["evaluation"]["sha256"],
        history["evaluations"][0]["sha256"]
    );
    assert_eq!(selected_report["commands_executed"], false);
    for args in [
        vec!["report", "--evaluation", "../../outside"],
        vec!["report", "--evaluation", "run-unknown"],
        vec!["report", "--evaluation", "workspace"],
    ] {
        let result = run(&args);
        assert_eq!(result.status.code(), Some(4));
    }
    for args in [
        vec!["report", "--operation-id", "write"],
        vec!["history", "--evaluation", key],
        vec!["report", "--evaluation", key, "--evaluation", key],
    ] {
        assert_eq!(run(&args).status.code(), Some(2));
    }
    assert_eq!(fs::read(path.join("events.jsonl")).unwrap(), before);
    assert_eq!(fs::read(path.join("state.json")).unwrap(), projection);
    let evidence = path.join("executions").join(key).join("report.json");
    let mut bytes = fs::read(&evidence).unwrap();
    bytes.push(b' ');
    fs::write(&evidence, bytes).unwrap();
    for args in [
        vec!["history"],
        vec!["report"],
        vec!["report", "--evaluation", key],
    ] {
        let result = run(&args);
        assert_eq!(result.status.code(), Some(5));
        assert_eq!(body(&result)["error"]["code"], "corrupt_artifact");
    }
    assert_eq!(fs::read(path.join("events.jsonl")).unwrap(), before);
}

#[test]
fn standalone_acceptance_uses_unicode_paths_and_preserves_source_and_evidence() {
    let fixture = Fixture::named("projet été 測定 avec espaces");
    let id = source_fixture(&fixture);
    // The distributed skill template is the starting contract for this acceptance run.
    let mut config: Value = serde_json::from_str(include_str!(
        "../../../skills/autoresearch-scout/assets/session.json"
    ))
    .unwrap();
    let original: Value =
        serde_json::from_slice(&fs::read(fixture.0.join("workspace.json")).unwrap()).unwrap();
    config["source"] = original["source"].clone();
    config["budget"]["deadline_unix_ms"] = original["budget"]["deadline_unix_ms"].clone();
    let source = fixture.0.join("source été 測定");
    fs::rename(fixture.0.join("source"), &source).unwrap();
    config["source"]["repository"] = json!(source);
    config["execution"]["network"] = json!("allowed");
    config["sampling"]["warmup"] = json!(0);
    config["budget"]["max_experiments"] = json!(3);
    config["checks"] =
        json!([{"executable":"/bin/sh", "args":["-c", "test -s src/main.txt"], "cwd":"."}]);
    let code = "v=10; case \"$(cat src/main.txt)\" in fast) v=5;; slow) v=20;; esac; printf 'METRIC {\"name\":\"bench_ms\",\"value\":%s,\"unit\":\"ms\"}\\n' \"$v\"";
    config["benchmark"] = json!({"executable":"/bin/sh", "args":["-c",code], "cwd":"."});
    let contract = fixture.0.join("contrat été 測定.json");
    fs::write(&contract, serde_json::to_vec(&config).unwrap()).unwrap();
    let pilot = fixture.0.join("pilote séparé 測定");
    fs::create_dir(&pilot).unwrap();
    let binary = fixture.0.join("moteur Rust été");
    fs::copy(env!("CARGO_BIN_EXE_autoresearch"), &binary).unwrap();
    let raw = |args: &[&str]| {
        Command::new(&binary)
            .current_dir(&fixture.0)
            .args(args)
            .arg("--json")
            .output()
            .unwrap()
    };
    let checked = |args: &[&str]| {
        let output = raw(args);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let value = body(&output);
        assert_eq!(value["schema_version"], 1);
        value
    };
    assert_eq!(
        checked(&["doctor"])["capabilities"]["inspect_evaluations"],
        true
    );
    checked(&["validate", "--config", contract.to_str().unwrap()]);
    checked(&[
        "init",
        "--config",
        contract.to_str().unwrap(),
        "--root",
        pilot.to_str().unwrap(),
        "--operation-id",
        "init",
    ]);
    let run = |args: &[&str]| {
        let mut all = args.to_vec();
        all.extend(["--session", &id, "--root", pilot.to_str().unwrap()]);
        checked(&all)
    };
    assert_eq!(run(&["status"])["next_action"], "workspace");
    run(&[
        "workspace",
        "--local-changes",
        "exclude",
        "--operation-id",
        "workspace",
    ]);
    run(&["baseline", "--operation-id", "baseline"]);
    assert_eq!(
        run(&["status"])["next_action"],
        "prepare_or_evaluate_if_authorized"
    );
    for (candidate, decision) in [("fast", "kept"), ("slow", "discarded")] {
        let prepared = run(&[
            "prepare-candidate",
            "--candidate",
            candidate,
            "--hypothesis",
            "Mesurer un changement ciblé",
            "--operation-id",
            &format!("prepare-{candidate}"),
        ]);
        fs::write(
            PathBuf::from(prepared["artifact"]["path"].as_str().unwrap()).join("src/main.txt"),
            candidate,
        )
        .unwrap();
        run(&[
            "seal",
            "--candidate",
            candidate,
            "--operation-id",
            &format!("seal-{candidate}"),
        ]);
        let result = run(&[
            "evaluate",
            "--candidate",
            candidate,
            "--operation-id",
            &format!("evaluate-{candidate}"),
        ]);
        assert_eq!(result["evaluation"]["report"]["decision"], decision);
    }
    let history = run(&["history"]);
    assert_eq!(history["evaluations"].as_array().unwrap().len(), 3);
    assert_eq!(history["accepted"], "sealed-fast");
    assert_eq!(
        run(&["report"])["evaluation"]["report"]["candidate"],
        "sealed-fast"
    );
    let rejected = history["evaluations"][2]["evaluation_key"]
        .as_str()
        .unwrap();
    assert_eq!(
        run(&["report", "--evaluation", rejected])["evaluation"]["report"]["decision"],
        "discarded"
    );
    run(&["stop", "--operation-id", "stop"]);
    assert_eq!(run(&["status"])["next_action"], "resume_if_authorized");
    run(&["resume", "--operation-id", "resume"]);
    let destination = fixture.0.join("résultat accepté 測定");
    run(&[
        "export-candidate",
        "--candidate",
        "fast",
        "--output",
        destination.to_str().unwrap(),
        "--operation-id",
        "export",
    ]);
    assert!(destination.join("candidate.patch").is_file());
    let preview = run(&["result-preview", "--evaluation", rejected]);
    assert_eq!(preview["files_published"], false);
    assert_eq!(preview["preview"]["report"]["decision"], "discarded");
    let bundle = fixture.0.join("rapport partagé 測定");
    let exported = run(&[
        "export-result",
        "--evaluation",
        rejected,
        "--output",
        bundle.to_str().unwrap(),
        "--operation-id",
        "result",
    ]);
    assert_eq!(exported["result"]["sha256"], preview["preview"]["sha256"]);
    assert!(bundle.join("report.md").is_file());
    assert!(!bundle.join("base").exists());
    let memory = checked(&[
        "memory",
        "--root",
        pilot.to_str().unwrap(),
        "--query",
        "src/main.txt",
    ]);
    assert_eq!(memory["rows"].as_array().unwrap().len(), 2);
    assert_eq!(memory["automatic_rejection"], false);
    let index = checked(&[
        "index",
        "--root",
        pilot.to_str().unwrap(),
        "--operation-id",
        "index",
    ]);
    assert_eq!(index["cache_rebuilt"], true);
    assert!(pilot.join(".auto/engine/memory.json").is_file());
    assert_eq!(
        fs::read(source.join("src/main.txt")).unwrap(),
        b"baseline\n"
    );
    assert_eq!(run(&["status"])["session"]["state"]["attempts_used"], 2);
}

#[test]
fn result_commands_reject_ambiguous_or_missing_options_before_creating_files() {
    let f = Fixture::new();
    for args in [
        vec!["index"],
        vec!["index", "--operation-id", "one", "--operation-id", "two"],
        vec!["memory", "--include-code"],
        vec!["memory", "--query"],
        vec!["memory", "--query", "a", "--query", "b"],
        vec!["result-preview", "--session", "test"],
        vec![
            "result-preview",
            "--session",
            "test",
            "--evaluation",
            "run-test",
            "--include-code",
            "--include-code",
        ],
        vec![
            "export-result",
            "--session",
            "test",
            "--evaluation",
            "run-test",
            "--operation-id",
            "export",
        ],
    ] {
        let result = f.cli().args(args).arg("--json").output().unwrap();
        assert_eq!(result.status.code(), Some(2), "{result:?}");
    }
    assert!(!f.0.join(".auto").exists());
}

#[test]
fn legacy_cli_inspects_imports_and_exposes_only_unverified_history() {
    let fixture = Fixture::named("import été historique");
    let source = fixture.0.join("archive Pi.jsonl");
    let bytes = include_bytes!("../../../tests/fixtures/legacy/pi.jsonl");
    fs::write(&source, bytes).unwrap();
    session_config(&fixture);
    let config = "config.json";
    let raw = |args: &[&str]| fixture.cli().args(args).arg("--json").output().unwrap();
    let inspected = raw(&["inspect-legacy", "--source", source.to_str().unwrap()]);
    assert!(inspected.status.success(), "{inspected:?}");
    assert_eq!(
        body(&inspected)["import_report"]["source_profile"],
        "pi_unversioned"
    );
    assert_eq!(body(&inspected)["session_created"], false);
    assert!(!fixture.0.join(".auto").exists());
    let args = [
        "import-legacy",
        "--source",
        source.to_str().unwrap(),
        "--config",
        config,
        "--operation-id",
        "import",
    ];
    let imported = raw(&args);
    assert!(imported.status.success(), "{imported:?}");
    assert_eq!(body(&imported)["commands_executed"], false);
    assert_eq!(body(&imported)["qualification_required"], true);
    assert_eq!(body(&imported)["session"]["state"]["accepted"], Value::Null);
    let id = body(&imported)["session"]["state"]["session_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let path = fixture.0.join(".auto/engine/sessions").join(&id);
    let journal = fs::read(path.join("events.jsonl")).unwrap();
    assert!(raw(&args).status.success());
    assert_eq!(fs::read(path.join("events.jsonl")).unwrap(), journal);
    let history = raw(&["history", "--session", &id]);
    assert!(history.status.success());
    assert_eq!(body(&history)["evaluations"], json!([]));
    assert_eq!(
        body(&history)["historical_import"]["trust"],
        "historical_unverified"
    );
    let report = raw(&["report", "--session", &id, "--legacy"]);
    assert!(report.status.success());
    assert_eq!(
        body(&report)["legacy_import"]["report"]["entries"][1]["declared"]["status"],
        "keep"
    );
    assert_eq!(raw(&["report", "--session", &id]).status.code(), Some(4));
    assert_eq!(
        raw(&[
            "report",
            "--session",
            &id,
            "--legacy",
            "--evaluation",
            "run-test"
        ])
        .status
        .code(),
        Some(2)
    );
    assert_eq!(fs::read(path.join("events.jsonl")).unwrap(), journal);
    assert_eq!(fs::read(&source).unwrap(), bytes);
    assert!(!fixture.0.join("must-not-execute").exists());
    fs::write(&source, b"{broken\n").unwrap();
    let rejected = raw(&args);
    assert_eq!(rejected.status.code(), Some(2));
    assert_eq!(body(&rejected)["import_report"]["importable"], false);
    assert_eq!(fs::read(path.join("events.jsonl")).unwrap(), journal);
    assert_eq!(fs::read(path.join("legacy/source.jsonl")).unwrap(), bytes);
}

#[test]
fn legacy_rejection_and_usage_errors_do_not_create_sessions() {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.join("bad.jsonl"),
        b"{\"iteration\":1,\"decision\":\"unknown\"}\n",
    )
    .unwrap();
    session_config(&fixture);
    let config = "config.json";
    for args in [
        vec!["inspect-legacy", "--source", "bad.jsonl"],
        vec![
            "import-legacy",
            "--source",
            "bad.jsonl",
            "--config",
            config,
            "--operation-id",
            "import",
        ],
        vec!["import-legacy", "--source", "bad.jsonl"],
        vec![
            "inspect-legacy",
            "--source",
            "bad.jsonl",
            "--format",
            "unknown",
        ],
        vec![
            "inspect-legacy",
            "--source",
            "bad.jsonl",
            "--source",
            "bad.jsonl",
        ],
        vec![
            "inspect-legacy",
            "--source",
            "bad.jsonl",
            "--operation-id",
            "no",
        ],
    ] {
        let out = fixture.cli().args(args).arg("--json").output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{out:?}");
        assert!(!fixture.0.join(".auto").exists());
    }
}

#[test]
fn abrupt_supervisor_death_blocks_resume_until_the_owned_group_exits() {
    use std::{
        process::Stdio,
        time::{Duration, Instant},
    };
    let fixture = Fixture::new();
    let id = source_fixture(&fixture);
    let path = fixture.0.join("workspace.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config["execution"]["network"] = json!("allowed");
    config["checks"] = json!([{"executable":"/bin/sh", "args":["-c","exit 0"], "cwd":"."}]);
    config["benchmark"] = json!({"executable":"/bin/sh", "args":["-c","mkdir -p build; printf ready > build/started; exec /bin/sleep 2"], "cwd":"."});
    config["budget"]["command_timeout_seconds"] = json!(3);
    fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let raw = |args: &[&str]| fixture.cli().args(args).arg("--json").output().unwrap();
    assert!(raw(&[
        "init",
        "--config",
        "workspace.json",
        "--operation-id",
        "init"
    ])
    .status
    .success());
    assert!(raw(&[
        "workspace",
        "--session",
        &id,
        "--local-changes",
        "exclude",
        "--operation-id",
        "workspace"
    ])
    .status
    .success());
    let mut host = fixture
        .cli()
        .args([
            "baseline",
            "--session",
            &id,
            "--operation-id",
            "interrupted",
            "--json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let session_path = fixture.0.join(".auto/engine/sessions").join(&id);
    let start = Instant::now();
    loop {
        let started = fs::read(session_path.join("active.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v["token"].as_str().map(str::to_owned))
            .is_some_and(|token| {
                session_path
                    .join("executions")
                    .join(format!("run-{token}"))
                    .join("reference/build/started")
                    .exists()
            });
        let identified = fs::read(session_path.join("process.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|marker| marker["group_pid"].as_u64().is_some());
        if started && identified {
            break;
        }
        if host.try_wait().unwrap().is_some() {
            panic!(
                "benchmark exited before starting: {:?}",
                host.wait_with_output().unwrap()
            );
        }
        if start.elapsed() > Duration::from_secs(10) {
            let _ = host.kill();
            let _ = host.wait();
            panic!("benchmark did not start");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    host.kill().unwrap();
    host.wait().unwrap();
    let before = raw(&["status", "--session", &id]);
    assert!(before.status.success(), "{before:?}");
    let before = body(&before);
    assert_eq!(before["session"]["recovery_required"], true);
    let resume = ["resume", "--session", &id, "--operation-id", "recover"];
    let blocked = raw(&resume);
    assert_eq!(blocked.status.code(), Some(5), "{blocked:?}");
    assert_eq!(body(&blocked)["error"]["code"], "recovery_required");
    // The abandoned child exits naturally; recovery itself must never signal its PID.
    let resumed = loop {
        let out = raw(&resume);
        if out.status.success() {
            break body(&out);
        }
        assert_eq!(body(&out)["error"]["code"], "recovery_required");
        assert!(start.elapsed() < Duration::from_secs(10), "{out:?}");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(
        resumed["session"]["state"]["active_ms_used"],
        before["session"]["state"]["active_ms_used"]
    );
    assert_eq!(
        resumed["session"]["deadline_unix_ms"],
        before["session"]["deadline_unix_ms"]
    );
    assert_eq!(resumed["session"]["state"]["attempts_used"], 0);
    assert_eq!(resumed["session"]["state"]["accepted"], Value::Null);
    assert_eq!(resumed["session"]["state"]["qualification"], Value::Null);
    assert_eq!(resumed["session"]["recovery_required"], false);
    assert!(!session_path.join("process.json").exists());
    assert_eq!(
        body(&raw(&["history", "--session", &id]))["evaluations"],
        json!([])
    );
}
