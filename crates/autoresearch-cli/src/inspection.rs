//! Read-only views of engine-owned evidence. No decision is recomputed here.
use autoresearch_core::session::{SessionError, SessionGuard, SessionStatus, SessionView};
use serde_json::{json, Value};

pub fn next_action(view: &SessionView) -> &'static str {
    if view.recovery_required {
        "inspect_recovery"
    } else if view.clock_regressed {
        "check_clock"
    } else if view.deadline_expired || view.active_ms_remaining == 0 || view.attempts_remaining == 0
    {
        "report_or_export"
    } else if view.state.status == SessionStatus::Stopped {
        "resume_if_authorized"
    } else if !view.state.artifacts.contains_key("workspace") {
        "workspace"
    } else if view.state.qualification.is_none() {
        "baseline"
    } else {
        "prepare_or_evaluate_if_authorized"
    }
}

pub fn inspect(
    guard: &SessionGuard,
    command: &str,
    selected: Option<&str>,
    historical: bool,
) -> Result<(Value, String), SessionError> {
    if historical {
        let report = guard.legacy_report()?.ok_or_else(|| SessionError {
            code: "report_not_found",
            message: "session has no historical import".into(),
        })?;
        return Ok((json!({"schema_version":1, "command":"report", "ok":true,
            "legacy_import":{"sha256":guard.state().legacy_import, "report":report},
            "commands_executed":false}), format!("Historical import (unverified)\nSource SHA-256: {}\nEntries: {}\nNo historical outcome is current proof.",report.source_sha256,report.entries.len())));
    }
    if command == "report" {
        let key = selected
            .or(guard.state().qualification.as_deref())
            .ok_or_else(|| SessionError {
                code: "report_not_found",
                message: "no qualified reference report; use history and select --evaluation"
                    .into(),
            })?;
        if !key.starts_with("run-") || !guard.state().artifacts.contains_key(key) {
            return Err(SessionError {
                code: "report_not_found",
                message: "evaluation key is not recorded; use history".into(),
            });
        }
        let evaluation = guard.evaluation(key)?;
        let report = &evaluation.report;
        let metric = &guard.config().metric;
        let mut text = format!(
            "Evaluation: {key}\nDecision: {:?}\nReason: {}\nCandidate: {}\nReference: {}\nMetric: {} ({}, {:?})\nEvidence SHA-256: {}",
            report.decision, report.reason, report.candidate.as_deref().unwrap_or("none (qualification)"),
            report.parent, metric.name, metric.unit, metric.direction, evaluation.sha256,
        );
        if let Some(summary) = &report.accepted_summary {
            text.push_str(&format!(
                "\nAccepted median: {} {}; noise range: {} {}; observations: {}",
                summary.median, metric.unit, summary.noise, metric.unit, summary.count
            ));
        }
        if let Some(gain) = report.paired_gain {
            text.push_str(&format!("\nPaired gain: {} {}", gain, metric.unit));
        }
        return Ok((
            json!({"schema_version":1, "command":"report", "ok":true,
            "evaluation_key":key, "evaluation":evaluation, "commands_executed":false}),
            text,
        ));
    }
    let mut rows = Vec::new();
    for key in guard
        .state()
        .artifacts
        .keys()
        .filter(|key| key.starts_with("run-"))
    {
        // Check every report's journal-bound fingerprint; never skip corrupt evidence.
        let evaluation = guard.evaluation(key)?;
        let report = evaluation.report;
        rows.push(
            json!({"evaluation_key":key, "operation_id":report.operation_id,
            "candidate":report.candidate, "parent":report.parent, "decision":report.decision,
            "reason":report.reason, "finished_unix_ms":report.finished_unix_ms,
            "sha256":evaluation.sha256}),
        );
    }
    rows.sort_by(|a, b| {
        a["finished_unix_ms"]
            .as_u64()
            .cmp(&b["finished_unix_ms"].as_u64())
            .then_with(|| {
                a["evaluation_key"]
                    .as_str()
                    .cmp(&b["evaluation_key"].as_str())
            })
    });
    let mut text = if rows.is_empty() {
        "No completed evaluations.".into()
    } else {
        rows.iter()
            .map(|row| {
                format!(
                    "{}  {}  {}  {}",
                    row["evaluation_key"].as_str().unwrap(),
                    row["decision"].as_str().unwrap(),
                    row["operation_id"].as_str().unwrap(),
                    row["reason"].as_str().unwrap()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let historical = guard.legacy_report()?.map(|report| {
        json!({
            "sha256":guard.state().legacy_import, "trust":report.trust,
            "source_profile":report.source_profile, "entries":report.entries.len(),
        })
    });
    if let Some(imported) = &historical {
        text.push_str(&format!(
            "\nHistorical import: {} unverified entries; use report --legacy.",
            imported["entries"]
        ));
    }
    Ok((
        json!({"schema_version":1, "command":"history", "ok":true,
        "session_id":guard.state().session_id, "accepted":guard.state().accepted,
        "qualification":guard.state().qualification, "evaluations":rows, "historical_import":historical,
        "commands_executed":false}),
        text,
    ))
}
