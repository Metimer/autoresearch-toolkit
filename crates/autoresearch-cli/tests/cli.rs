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
        let path = std::env::temp_dir().join(format!(
            "autoresearch-cli-{}-{}",
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
fn doctor_reports_missing_git_and_unimplemented_execution() {
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
