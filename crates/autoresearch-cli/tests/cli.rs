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
