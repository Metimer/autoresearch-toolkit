use autoresearch_core::{
    legacy::{LegacyFormat, PreparedImport, SOURCE_LIMIT},
    session::{SessionStatus, SessionStore},
    SessionConfig, ValidatedConfig,
};
use serde_json::{json, Value};
use std::fs;
const PI: &[u8] = include_bytes!("../../../tests/fixtures/legacy/pi.jsonl");
const PORTABLE: &[u8] = include_bytes!("../../../tests/fixtures/legacy/portable-v1.jsonl");
fn config() -> ValidatedConfig {
    let mut config: SessionConfig =
        serde_json::from_str(include_str!("../../../examples/session.json")).unwrap();
    config.session_id = "imported".into();
    config.budget.deadline_unix_ms = 4_000_000_000_000;
    ValidatedConfig::try_from(config).unwrap()
}
fn portable() -> Value {
    serde_json::from_str(
        std::str::from_utf8(PORTABLE)
            .unwrap()
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap()
}
fn pi() -> (Value, Value) {
    let mut lines = std::str::from_utf8(PI).unwrap().lines();
    (
        serde_json::from_str(lines.next().unwrap()).unwrap(),
        serde_json::from_str(lines.next().unwrap()).unwrap(),
    )
}
fn journal(values: &[Value]) -> Vec<u8> {
    let mut result = Vec::new();
    for value in values {
        result.extend(serde_json::to_vec(value).unwrap());
        result.push(b'\n');
    }
    result
}
#[test]
fn profiles_preserve_declared_data_and_method_segments_without_verifying_results() {
    for (bytes, format, count) in [
        (PI, LegacyFormat::Pi, 4),
        (PORTABLE, LegacyFormat::Portable, 2),
    ] {
        let prepared = PreparedImport::parse(bytes, LegacyFormat::Auto).unwrap();
        let report = prepared.report();
        assert!(report.importable, "{:?}", report.anomalies);
        assert_eq!(report.source_format, Some(format));
        assert_eq!(report.entries.len(), count);
        assert!(report
            .entries
            .iter()
            .all(|entry| entry.trust == "historical_unverified"));
        assert_eq!(report.trust, "historical_unverified");
    }
    let prepared = PreparedImport::parse(PI, LegacyFormat::Pi).unwrap();
    assert_eq!(prepared.report().entries[1].segment, 0);
    assert_eq!(prepared.report().entries[3].segment, 1);
    assert_eq!(
        prepared.report().entries[2].declared["bestDirection"],
        "higher"
    );
    assert_eq!(
        prepared.report().entries[1].declared["metrics"]["memory_mb"],
        20
    );
}
#[test]
fn malformed_duplicate_nonfinite_and_mixed_records_block_the_entire_import() {
    let (header, run) = pi();
    let prefix = serde_json::to_string(&header).unwrap();
    let run = serde_json::to_string(&run).unwrap();
    let invalid = [
        format!("{prefix}\n{run}\n{{broken\n{run}\n"),
        format!("{prefix}\n{run}\n{{\"run\":"),
        format!("{prefix}\n{}\n", run.replace("\"keep\"", "\"unknown\"")),
        format!(
            "{prefix}\n{}\n",
            run.replace("\"metric\":10", "\"metric\":1e999")
        ),
        format!(
            "{prefix}\n{}\n",
            run.replace("\"metric\":10", "\"metric\":10,\"metric\":1")
        ),
        format!(
            "{prefix}\n{}\n",
            run.replace("\"memory_mb\":20", "\"memory_mb\":20,\"memory_mb\":1")
        ),
        format!(
            "{prefix}\n{run}\n{}",
            String::from_utf8(journal(&[portable()])).unwrap()
        ),
        "null\n".into(),
        "[]\n".into(),
        "{}\n".into(),
        "".into(),
    ];
    for input in invalid {
        let prepared = PreparedImport::parse(input.as_bytes(), LegacyFormat::Auto).unwrap();
        assert!(!prepared.report().importable, "{input}");
        assert!(prepared
            .report()
            .anomalies
            .iter()
            .any(|a| a.severity == "error"));
    }
    assert!(
        !PreparedImport::parse(PI, LegacyFormat::Portable)
            .unwrap()
            .report()
            .importable
    );
    assert!(
        !PreparedImport::parse(b"\xff", LegacyFormat::Auto)
            .unwrap()
            .report()
            .importable
    );
}
#[test]
fn missing_and_unknown_fields_versions_statuses_and_invalid_numbers_are_rejected() {
    for (field, value) in [
        ("decision", json!("kept")),
        ("decision", Value::Null),
        ("schema_version", json!(2)),
        ("iteration", json!(0)),
        ("iteration", json!(-1)),
        ("iteration", json!(1.5)),
        ("elapsed_seconds", json!(-1)),
        ("candidate_samples", json!(["NaN"])),
        ("timestamp_unix_ms", json!(0)),
        ("head", json!("not-a-commit")),
        ("direction", json!("up")),
        ("check_exit_status", json!(1)),
        ("unknown", json!(true)),
    ] {
        let mut row = portable();
        row[field] = value;
        assert!(
            !PreparedImport::parse(&journal(&[row]), LegacyFormat::Auto)
                .unwrap()
                .report()
                .importable,
            "{field}"
        );
    }
    for field in [
        "decision",
        "metric",
        "baseline_samples",
        "head",
        "timestamp_unix_ms",
    ] {
        let mut row = portable();
        row.as_object_mut().unwrap().remove(field);
        assert!(
            !PreparedImport::parse(&journal(&[row]), LegacyFormat::Auto)
                .unwrap()
                .report()
                .importable,
            "{field}"
        );
    }
    let (mut header, mut row) = pi();
    row.as_object_mut().unwrap().remove("status");
    assert!(
        !PreparedImport::parse(&journal(&[header.clone(), row]), LegacyFormat::Auto)
            .unwrap()
            .report()
            .importable
    );
    header["schema_version"] = json!(1);
    assert!(
        !PreparedImport::parse(&journal(&[header, pi().1]), LegacyFormat::Auto)
            .unwrap()
            .report()
            .importable
    );
    assert!(
        !PreparedImport::parse(&journal(&[pi().1]), LegacyFormat::Pi)
            .unwrap()
            .report()
            .importable
    );
}
#[test]
fn incomplete_portable_context_is_preserved_without_defaults_and_utc_is_checked() {
    let mut row =
        json!({"iteration":1,"decision":"inconclusive","timestamp":"2024-02-29T12:00:00.123Z"});
    let prepared = PreparedImport::parse(&journal(&[row.clone()]), LegacyFormat::Auto).unwrap();
    assert!(prepared.report().importable);
    assert_eq!(
        prepared.report().source_profile.as_deref(),
        Some("portable_unversioned")
    );
    assert!(prepared.report().entries[0]
        .declared
        .get("metric")
        .is_none());
    for invalid in [
        "2023-02-29T12:00:00Z",
        "2024-13-01T12:00:00Z",
        "2024-01-01T25:00:00Z",
        "2024-01-01",
        "2024-01-01T12:00:00+01:00",
    ] {
        row["timestamp"] = json!(invalid);
        assert!(
            !PreparedImport::parse(&journal(&[row.clone()]), LegacyFormat::Auto)
                .unwrap()
                .report()
                .importable
        );
    }
    let mut missing_newline = PORTABLE.to_vec();
    missing_newline.pop();
    let prepared = PreparedImport::parse(&missing_newline, LegacyFormat::Portable).unwrap();
    assert!(prepared.report().importable);
    assert!(prepared
        .report()
        .anomalies
        .iter()
        .any(|a| a.code == "missing_final_newline"));
}
#[test]
fn input_size_line_count_sequence_and_version_consistency_are_bounded() {
    assert!(PreparedImport::parse(&vec![b' '; SOURCE_LIMIT + 1], LegacyFormat::Auto).is_err());
    for bytes in [
        vec![b' '; 65537],
        vec![b'\n'; 4097],
        journal(&[portable(), portable()]),
    ] {
        assert!(
            !PreparedImport::parse(&bytes, LegacyFormat::Auto)
                .unwrap()
                .report()
                .importable
        );
    }
    let mut second = portable();
    second["iteration"] = json!(2);
    second.as_object_mut().unwrap().remove("schema_version");
    assert!(
        !PreparedImport::parse(&journal(&[portable(), second]), LegacyFormat::Auto)
            .unwrap()
            .report()
            .importable
    );
}
#[test]
fn import_is_atomic_idempotent_and_cannot_replace_or_resume_a_live_session() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("historical.jsonl");
    fs::write(&source, PI).unwrap();
    let prepared = PreparedImport::read(&source, LegacyFormat::Auto).unwrap();
    let store = SessionStore::new(temp.path()).unwrap();
    let session = store
        .import_legacy(config(), "import", &prepared, 1)
        .unwrap();
    assert_eq!(session.state().status, SessionStatus::Created);
    assert_eq!(session.state().sequence, 1);
    assert_eq!(session.state().attempts_used, 0);
    assert_eq!(session.state().active_ms_used, 0);
    assert!(session.state().accepted.is_none());
    assert!(session.state().qualification.is_none());
    assert!(session.state().execution.is_none());
    assert!(session.state().artifacts.is_empty());
    assert_eq!(session.legacy_report().unwrap().unwrap().entries.len(), 4);
    assert_eq!(fs::read(&source).unwrap(), PI);
    assert!(!temp.path().join("must-not-execute").exists());
    drop(session);
    let path = temp.path().join(".auto/engine/sessions/imported");
    assert_eq!(fs::read(path.join("legacy/source.jsonl")).unwrap(), PI);
    let before = fs::read(path.join("events.jsonl")).unwrap();
    fs::remove_file(path.join("state.json")).unwrap();
    let session = store
        .import_legacy(config(), "import", &prepared, 2)
        .unwrap();
    assert!(session.view(2).projection_current);
    assert_eq!(fs::read(path.join("events.jsonl")).unwrap(), before);
    drop(session);
    assert!(store.init(config(), "import", 3).is_err());
    assert!(store
        .import_legacy(config(), "different", &prepared, 3)
        .is_err());
    let other = PreparedImport::parse(PORTABLE, LegacyFormat::Auto).unwrap();
    assert!(store.import_legacy(config(), "import", &other, 3).is_err());
    fs::write(path.join("legacy/source.jsonl"), PORTABLE).unwrap();
    assert_eq!(
        store.open("imported", false).err().unwrap().code,
        "corrupt_artifact"
    );
    assert_eq!(fs::read(path.join("events.jsonl")).unwrap(), before);
}
#[test]
fn invalid_import_quota_and_links_never_publish_a_session() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let store = SessionStore::new(temp.path()).unwrap();
    let invalid = PreparedImport::parse(b"{broken", LegacyFormat::Auto).unwrap();
    assert!(store
        .import_legacy(config(), "import", &invalid, 1)
        .is_err());
    assert!(!temp.path().join(".auto").exists());
    let prepared = PreparedImport::parse(PI, LegacyFormat::Pi).unwrap();
    let mut small = config().get().clone();
    small.budget.max_artifact_bytes = 1024;
    small.budget.max_output_bytes = 512;
    assert_eq!(
        store
            .import_legacy(
                ValidatedConfig::try_from(small).unwrap(),
                "import",
                &prepared,
                1
            )
            .err()
            .unwrap()
            .code,
        "storage_limit"
    );
    assert!(!temp.path().join(".auto/engine/sessions/imported").exists());
    let source = temp.path().join("source");
    fs::write(&source, PI).unwrap();
    let link = temp.path().join("link");
    symlink(&source, &link).unwrap();
    assert!(PreparedImport::read(&link, LegacyFormat::Auto).is_err());
    fs::remove_file(&link).unwrap();
    fs::hard_link(&source, &link).unwrap();
    assert!(PreparedImport::read(&link, LegacyFormat::Auto).is_err());
}
