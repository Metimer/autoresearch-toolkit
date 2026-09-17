use autoresearch_core::{
    config::{CommandSpec, Direction, MetricDomain},
    measurement,
    supervisor::{self, ProcessOutcome},
    SessionConfig,
};
use std::{
    collections::BTreeMap,
    fs,
    time::{Duration, Instant},
};
fn spec(code: &str) -> CommandSpec {
    CommandSpec {
        executable: "/bin/sh".into(),
        args: vec!["-c".into(), code.into()],
        cwd: ".".into(),
    }
}
fn config() -> SessionConfig {
    serde_json::from_str(include_str!("../../../examples/session.json")).unwrap()
}
#[test]
fn supervisor_handles_success_failure_missing_binary_and_literal_arguments() {
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("process.json");
    let out = supervisor::run(
        &spec("printf 'ok'; printf 'err' >&2"),
        root.path(),
        &BTreeMap::new(),
        supervisor::Limits {
            timeout_ms: 1000,
            max_output_bytes: 100,
        },
        &marker,
        "test",
        || Ok(false),
    )
    .unwrap();
    assert_eq!(out.report.outcome, ProcessOutcome::Passed);
    assert_eq!(out.stdout, b"ok");
    assert_eq!(out.stderr, b"err");
    assert!(!marker.exists());
    assert_eq!(
        supervisor::run(
            &spec("exit 4"),
            root.path(),
            &BTreeMap::new(),
            supervisor::Limits {
                timeout_ms: 1000,
                max_output_bytes: 100
            },
            &marker,
            "test",
            || Ok(false)
        )
        .unwrap()
        .report
        .exit_code,
        Some(4)
    );
    let missing = CommandSpec {
        executable: "/missing-command".into(),
        args: vec![],
        cwd: ".".into(),
    };
    assert_eq!(
        supervisor::run(
            &missing,
            root.path(),
            &BTreeMap::new(),
            supervisor::Limits {
                timeout_ms: 1000,
                max_output_bytes: 100
            },
            &marker,
            "test",
            || Ok(false)
        )
        .unwrap()
        .report
        .outcome,
        ProcessOutcome::SpawnFailed
    );
    let literal = CommandSpec {
        executable: "/bin/echo".into(),
        args: vec!["$(touch injected)".into()],
        cwd: ".".into(),
    };
    let out = supervisor::run(
        &literal,
        root.path(),
        &BTreeMap::new(),
        supervisor::Limits {
            timeout_ms: 1000,
            max_output_bytes: 100,
        },
        &marker,
        "test",
        || Ok(false),
    )
    .unwrap();
    assert_eq!(out.stdout, b"$(touch injected)\n");
    assert!(!root.path().join("injected").exists());
}
#[test]
fn timeout_and_cancellation_kill_descendants_that_ignore_term_or_hold_output() {
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("process.json");
    let start = Instant::now();
    let out = supervisor::run(
        &spec("trap '' TERM; (trap '' TERM; sleep 1; echo survived > survived) & wait"),
        root.path(),
        &BTreeMap::new(),
        supervisor::Limits {
            timeout_ms: 80,
            max_output_bytes: 100,
        },
        &marker,
        "timeout",
        || Ok(false),
    )
    .unwrap();
    assert_eq!(out.report.outcome, ProcessOutcome::TimedOut);
    assert!(start.elapsed() < Duration::from_secs(3));
    let out = supervisor::run(
        &spec("(sleep 1; echo survived > orphan) & exit 0"),
        root.path(),
        &BTreeMap::new(),
        supervisor::Limits {
            timeout_ms: 1000,
            max_output_bytes: 100,
        },
        &marker,
        "orphan",
        || Ok(false),
    )
    .unwrap();
    assert_eq!(out.report.outcome, ProcessOutcome::Passed);
    let start = Instant::now();
    let out = supervisor::run(
        &spec("sleep 5"),
        root.path(),
        &BTreeMap::new(),
        supervisor::Limits {
            timeout_ms: 2000,
            max_output_bytes: 100,
        },
        &marker,
        "cancel",
        || Ok(start.elapsed() > Duration::from_millis(30)),
    )
    .unwrap();
    assert_eq!(out.report.outcome, ProcessOutcome::Cancelled);
    std::thread::sleep(Duration::from_millis(1100));
    assert!(!root.path().join("survived").exists());
    assert!(!root.path().join("orphan").exists());
    assert!(!marker.exists());
}
#[test]
fn massive_output_and_storage_failure_are_bounded() {
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("process.json");
    let out = supervisor::run(
        &spec("while :; do printf 1234567890; done"),
        root.path(),
        &BTreeMap::new(),
        supervisor::Limits {
            timeout_ms: 1000,
            max_output_bytes: 1024,
        },
        &marker,
        "output",
        || Ok(false),
    )
    .unwrap();
    assert_eq!(out.report.outcome, ProcessOutcome::OutputLimit);
    assert!(out.stdout.len() + out.stderr.len() <= 1024);
    let out = supervisor::run(
        &spec("sleep 5"),
        root.path(),
        &BTreeMap::new(),
        supervisor::Limits {
            timeout_ms: 1000,
            max_output_bytes: 1024,
        },
        &marker,
        "storage",
        || Err(std::io::Error::other("simulated disk quota").into()),
    )
    .unwrap();
    assert_eq!(out.report.outcome, ProcessOutcome::StorageLimit);
    assert!(!marker.exists());
    fs::create_dir(&marker).unwrap();
    assert!(supervisor::run(
        &spec("touch must-not-start"),
        root.path(),
        &BTreeMap::new(),
        supervisor::Limits {
            timeout_ms: 1000,
            max_output_bytes: 1024
        },
        &marker,
        "unsafe",
        || Ok(false)
    )
    .is_err());
    assert!(!root.path().join("must-not-start").exists());
}
#[test]
fn strict_metrics_and_paired_decisions_reject_missing_duplicate_or_invalid_values() {
    let mut config = config();
    let good = b"log\nMETRIC {\"name\":\"bench_ms\",\"value\":10,\"unit\":\"ms\"}\n";
    assert_eq!(measurement::parse(good, &config).unwrap()["bench_ms"], 10.0);
    for bytes in [
        b"METRIC bench_ms=10".as_slice(),
        b"METRIC {\"name\":\"bench_ms\",\"value\":-1,\"unit\":\"ms\"}",
        b"METRIC {\"name\":\"bench_ms\",\"value\":1e999,\"unit\":\"ms\"}",
        b"METRIC {\"name\":\"bench_ms\",\"value\":10,\"unit\":\"seconds\"}",
        b"no metrics",
        b"METRIC {\"name\":\"bench_ms\",\"value\":1,\"value\":2,\"unit\":\"ms\"}",
    ] {
        assert!(measurement::parse(bytes, &config).is_err());
    }
    let duplicate = [good.as_slice(), good.as_slice()].concat();
    assert!(measurement::parse(&duplicate, &config).is_err());
    config.metric.domain = MetricDomain::Finite;
    assert!(measurement::parse(
        b"METRIC {\"name\":\"bench_ms\",\"value\":-1,\"unit\":\"ms\"}",
        &config
    )
    .is_ok());
    assert_eq!(
        measurement::paired_gain(&[10.0; 5], &[7.0; 5], Direction::Lower).unwrap(),
        3.0
    );
    assert_eq!(
        measurement::paired_gain(&[10.0; 5], &[7.0; 5], Direction::Higher).unwrap(),
        -3.0
    );
    assert!(measurement::paired_gain(&[10.0; 3], &[7.0; 3], Direction::Lower).is_err());
}
