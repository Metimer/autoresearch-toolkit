use autoresearch_core::{
    config::ExperimentOutcome, SessionConfig, ValidatedConfig, MAX_CONFIG_BYTES,
};
use serde_json::{json, Value};

const EXAMPLE: &str = include_str!("../../../examples/session.json");

fn check(value: Value) -> bool {
    ValidatedConfig::from_json(&serde_json::to_vec(&value).unwrap()).is_ok()
}

#[test]
fn example_roundtrips_and_validated_config_has_read_only_access() {
    let valid = ValidatedConfig::from_json(EXAMPLE.as_bytes()).unwrap();
    let bytes = serde_json::to_vec(valid.get()).unwrap();
    let restored = ValidatedConfig::from_json(&bytes).unwrap();
    assert_eq!(restored.get().session_id, "example-session");
    assert_eq!(restored.get().checks.len(), 1);
}

#[test]
fn rejects_invalid_contracts_without_favorable_defaults() {
    let cases = [
        ("/schema_version", json!(2)),
        ("/schema_version", json!(-1)),
        ("/session_id", json!("../escape")),
        ("/goal", json!(" ")),
        ("/source/commit", json!("HEAD")),
        ("/checks", json!([])),
        ("/metric/direction", json!("unknown")),
        ("/metric/domain", json!("unknown")),
        ("/metric/name", json!("run_min_ms")),
        ("/metric/name", json!("bad=metric")),
        ("/metric/minimum_improvement", json!(0)),
        ("/metric/minimum_improvement", json!(-1)),
        ("/sampling/runs", json!(0)),
        ("/sampling/runs", json!(32)),
        ("/sampling/warmup", json!(-1)),
        ("/sampling/warmup", json!(32)),
        ("/budget/max_experiments", json!(0)),
        ("/budget/active_seconds", json!(0)),
        ("/budget/command_timeout_seconds", json!(1201)),
        ("/budget/deadline_unix_ms", json!(0)),
        ("/budget/max_output_bytes", json!(104857601)),
        ("/budget/max_artifact_bytes", json!(0)),
        ("/benchmark/executable", json!("")),
        ("/benchmark/args", json!(["a\u{0}b"])),
        ("/benchmark/cwd", json!("../escape")),
        ("/scope/allowed_paths", json!([])),
        ("/scope/allowed_paths", json!(["src/", "src"])),
        ("/scope/allowed_paths", json!(["tests/suite.rs"])),
    ];
    for (pointer, invalid) in cases {
        let mut config: Value = serde_json::from_str(EXAMPLE).unwrap();
        *config.pointer_mut(pointer).unwrap() = invalid;
        assert!(!check(config), "accepted invalid value at {pointer}");
    }
}

#[test]
fn rejects_unknown_missing_and_duplicate_fields() {
    for pointer in [
        "",
        "/source",
        "/scope",
        "/benchmark",
        "/metric",
        "/sampling",
        "/budget",
        "/checks/0",
    ] {
        let mut config: Value = serde_json::from_str(EXAMPLE).unwrap();
        config
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), json!(true));
        assert!(!check(config), "accepted unknown field at {pointer}");
    }
    let mut missing: Value = serde_json::from_str(EXAMPLE).unwrap();
    missing.as_object_mut().unwrap().remove("checks");
    assert!(!check(missing));
    let duplicate = EXAMPLE.replacen(
        "\"schema_version\": 1",
        "\"schema_version\": 1, \"schema_version\": 1",
        1,
    );
    assert!(ValidatedConfig::from_json(duplicate.as_bytes()).is_err());
}

#[test]
fn rejects_nonfinite_values_from_rust_and_json() {
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut config: SessionConfig = serde_json::from_str(EXAMPLE).unwrap();
        config.metric.minimum_improvement = number;
        assert!(ValidatedConfig::try_from(config).is_err());
    }
    for number in ["NaN", "Infinity", "1e999"] {
        let text = EXAMPLE.replace(
            "\"minimum_improvement\": 1.0",
            &format!("\"minimum_improvement\": {number}"),
        );
        assert!(ValidatedConfig::from_json(text.as_bytes()).is_err());
    }
}

#[test]
fn rejects_unsafe_and_ambiguous_paths_on_every_platform() {
    for path in [
        "/tmp/file",
        "../file",
        "src/../file",
        "src//file",
        "src/./file",
        "src//",
        "C:/file",
        "src\\file",
        "src/*",
        ".git/config",
        ".Git/config",
        ".auto/session.json",
        ".AUTO/session.json",
        "src/\nfile",
    ] {
        let mut config: Value = serde_json::from_str(EXAMPLE).unwrap();
        config["scope"]["allowed_paths"] = json!([path]);
        assert!(!check(config), "accepted {path:?}");
    }
}

#[test]
fn protected_subdirectories_and_literal_unicode_paths_are_supported() {
    let mut config: Value = serde_json::from_str(EXAMPLE).unwrap();
    config["scope"]["allowed_paths"] = json!(["src/", "données/local file.txt"]);
    config["scope"]["protected_paths"] = json!(["src/tests/"]);
    assert!(check(config));
}

#[test]
fn deadline_is_checked_explicitly_and_expires_at_the_boundary() {
    let config = ValidatedConfig::from_json(EXAMPLE.as_bytes()).unwrap();
    let deadline = config.get().budget.deadline_unix_ms;
    assert!(config.check_deadline(deadline - 1).is_ok());
    assert!(config.check_deadline(deadline).is_err());
    assert!(config.check_deadline(deadline + 1).is_err());
}

#[test]
fn unknown_outcome_is_rejected_instead_of_becoming_kept() {
    for value in ["\"unknown\"", "\"keep\"", "null", "{}", "0"] {
        assert!(serde_json::from_str::<ExperimentOutcome>(value).is_err());
    }
    assert_eq!(
        serde_json::from_str::<ExperimentOutcome>("\"kept\"").unwrap(),
        ExperimentOutcome::Kept
    );
}

#[test]
fn oversized_inputs_and_parse_errors_do_not_echo_input_values() {
    assert!(ValidatedConfig::from_json(&vec![b' '; MAX_CONFIG_BYTES + 1]).is_err());
    let error = ValidatedConfig::from_json(b"{\"SYNTHETIC_PRIVATE_VALUE\": true}").unwrap_err();
    assert!(!error.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
}
