use autoresearch_core::{config::schema, ValidatedConfig};
use jsonschema::{Draft, JSONSchema};
use serde_json::{json, Value};

fn example() -> Value {
    serde_json::from_str(include_str!("../../../examples/session.json")).unwrap()
}
fn runtime(value: &Value) -> bool {
    ValidatedConfig::from_json(&serde_json::to_vec(value).unwrap()).is_ok()
}

#[test]
fn checked_in_schema_matches_the_rust_types() {
    let checked_in: Value =
        serde_json::from_str(include_str!("../../../schemas/session-v2.schema.json")).unwrap();
    assert_eq!(checked_in, serde_json::to_value(schema()).unwrap());
    let validator = JSONSchema::options()
        .with_draft(Draft::Draft7)
        .compile(&checked_in)
        .unwrap();
    assert!(validator.is_valid(&example()));
    assert!(runtime(&example()));
}

#[test]
fn schema_and_runtime_reject_structurally_invalid_contracts() {
    let schema = serde_json::to_value(schema()).unwrap();
    let validator = JSONSchema::options()
        .with_draft(Draft::Draft7)
        .compile(&schema)
        .unwrap();
    for (pointer, invalid) in [
        ("/schema_version", json!(1)),
        ("/schema_version", json!(999)),
        ("/session_id", json!("../escape")),
        ("/session_id", json!("")),
        ("/source/commit", json!("abc")),
        ("/source/commit", json!("z".repeat(40))),
        ("/sampling/runs", json!(2)),
        ("/sampling/runs", json!(32)),
        ("/sampling/warmup", json!(32)),
        ("/sampling/baseline_rounds", json!(0)),
        ("/sampling/order", json!("random")),
        ("/sampling/input_sha256", json!("abc")),
        ("/execution/network", json!("auto")),
        (
            "/execution/hooks",
            json!({"before":[],"after":[],"extra":true}),
        ),
        (
            "/execution/environment",
            json!({"inherit":[],"set":{},"extra":true}),
        ),
        ("/commit_policy", json!("always")),
        ("/checks", json!([])),
        ("/metric/direction", json!("unknown")),
        ("/metric/name", json!("9bad")),
        ("/budget/active_seconds", json!(0)),
        ("/budget/max_experiments", json!(0)),
        ("/budget/deadline_unix_ms", json!(-1)),
        ("/scope/allowed_paths", json!([])),
        (
            "/secondary_constraints",
            json!([{"name":"rss", "unit":"bytes", "domain":"positive", "bound":{"kind":"approximate","value":1}}]),
        ),
    ] {
        let mut value = example();
        *value.pointer_mut(pointer).unwrap() = invalid;
        assert!(
            !validator.is_valid(&value),
            "schema accepted {pointer}: {value}"
        );
        assert!(!runtime(&value), "runtime accepted {pointer}: {value}");
    }
    for field in example().as_object().unwrap().keys() {
        let mut value = example();
        value.as_object_mut().unwrap().remove(field);
        assert!(
            !validator.is_valid(&value),
            "schema accepted missing {field}"
        );
        assert!(!runtime(&value), "runtime accepted missing {field}");
    }
}

#[test]
fn relational_policies_remain_a_separate_runtime_gate() {
    let schema = serde_json::to_value(schema()).unwrap();
    let validator = JSONSchema::options()
        .with_draft(Draft::Draft7)
        .compile(&schema)
        .unwrap();
    for (pointer, invalid) in [
        ("/scope/generated_paths", json!(["src/"])),
        (
            "/scope/protected_sha256",
            json!({"outside.txt": "a".repeat(64)}),
        ),
        ("/sampling/cache", json!({"mode":"none","paths":["build/"]})),
        ("/sampling/cache", json!({"mode":"cold","paths":["src/"]})),
        (
            "/execution/environment",
            json!({"inherit":["PATH"],"set":{"PATH":"value"}}),
        ),
        (
            "/execution/environment",
            json!({"inherit":["PATH","PATH"],"set":{}}),
        ),
        ("/budget/command_timeout_seconds", json!(99_999)),
        (
            "/secondary_constraints",
            json!([{"name":"rss", "unit":"bytes", "domain":"positive", "bound":{"kind":"at_most","value":-1}}]),
        ),
    ] {
        let mut value = example();
        *value.pointer_mut(pointer).unwrap() = invalid;
        assert!(
            validator.is_valid(&value),
            "this case should demonstrate a runtime-only rule: {pointer}"
        );
        assert!(!runtime(&value), "runtime accepted inconsistent {pointer}");
    }
}

#[test]
fn extended_policy_accepts_explicit_values_and_rejects_duplicate_map_keys() {
    let mut value = example();
    value["scope"]["protected_paths"] = json!(["tests/"]);
    value["scope"]["protected_sha256"] = json!({"tests/check.py":"a".repeat(64)});
    value["sampling"]["cache"] = json!({"mode":"cold", "paths":["build/cache/"]});
    value["execution"]["environment"]["set"] = json!({"LANG":"C"});
    value["secondary_constraints"] = json!([{"name":"rss","unit":"bytes","domain":"positive","bound":{"kind":"at_most","value":100}}]);
    assert!(runtime(&value));
    let schema = serde_json::to_value(schema()).unwrap();
    assert!(JSONSchema::compile(&schema).unwrap().is_valid(&value));
    let raw = serde_json::to_string(&value).unwrap();
    for duplicate in [
        raw.replace("\"LANG\":\"C\"", "\"LANG\":\"C\",\"LANG\":\"other\""),
        raw.replace(
            &format!("\"tests/check.py\":\"{}\"", "a".repeat(64)),
            &format!(
                "\"tests/check.py\":\"{}\",\"tests/check.py\":\"{}\"",
                "a".repeat(64),
                "b".repeat(64)
            ),
        ),
    ] {
        assert_ne!(raw, duplicate);
        assert!(ValidatedConfig::from_json(duplicate.as_bytes()).is_err());
    }
}
