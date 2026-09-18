use crate::{emit, error, load_config, session_error};
use autoresearch_core::{
    legacy::{LegacyFormat, PreparedImport},
    session::SessionStore,
};
use serde_json::json;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::ExitCode,
};

pub fn command(name: &str, args: &[OsString], as_json: bool) -> ExitCode {
    let mut source: Option<PathBuf> = None;
    let mut config: Option<PathBuf> = None;
    let mut root: Option<PathBuf> = None;
    let mut operation = None;
    let mut format = None;
    let mut options = args.iter();
    while let Some(flag) = options.next() {
        let Some(value) = options.next() else {
            return error(as_json, "usage", "missing option value; use --help", 2);
        };
        match flag.to_str() {
            Some("--source") if source.is_none() => source = Some(value.into()),
            Some("--config") if name == "import-legacy" && config.is_none() => {
                config = Some(value.into())
            }
            Some("--root") if name == "import-legacy" && root.is_none() => {
                root = Some(value.into())
            }
            Some("--operation-id") if name == "import-legacy" && operation.is_none() => {
                let Some(id) = value.to_str() else {
                    return error(as_json, "usage", "operation ID must be UTF-8", 2);
                };
                operation = Some(id);
            }
            Some("--format") if format.is_none() => {
                format = Some(match value.to_str() {
                    Some("auto") => LegacyFormat::Auto,
                    Some("pi") => LegacyFormat::Pi,
                    Some("portable") => LegacyFormat::Portable,
                    _ => return error(as_json, "usage", "format must be auto, pi or portable", 2),
                });
            }
            _ => {
                return error(
                    as_json,
                    "usage",
                    "unknown or repeated legacy option; use --help",
                    2,
                )
            }
        }
    }
    let Some(source) = source else {
        return error(as_json, "usage", "--source is required", 2);
    };
    if name == "import-legacy" && (config.is_none() || operation.is_none()) {
        return error(
            as_json,
            "usage",
            "import requires --config and --operation-id for a new session",
            2,
        );
    }
    let prepared = match PreparedImport::read(&source, format.unwrap_or(LegacyFormat::Auto)) {
        Ok(prepared) => prepared,
        Err(issue) => return session_error(as_json, issue),
    };
    if !prepared.report().importable || name == "inspect-legacy" {
        let valid = prepared.report().importable;
        let mut body = json!({"schema_version":1, "command":name, "ok":valid,
            "import_report":prepared.report(), "commands_executed":false, "session_created":false});
        if !valid {
            body["error"] = json!({"code":"invalid_legacy", "message":"historical journal has blocking anomalies"});
        }
        let mut text = format!(
            "Historical journal: {}\nSource SHA-256: {}\nEntries: {}",
            if valid {
                "importable (unverified)"
            } else {
                "rejected"
            },
            prepared.report().source_sha256,
            prepared.report().entries.len()
        );
        for issue in &prepared.report().anomalies {
            text.push_str(&format!(
                "\n{} {:?}: {} — {}",
                issue.severity, issue.line, issue.code, issue.message
            ));
        }
        emit(as_json, body, &text);
        return ExitCode::from(if valid { 0 } else { 2 });
    }
    let config = match load_config(config.as_deref().unwrap()) {
        Ok(config) => config,
        Err((code, message, status)) => return error(as_json, code, &message, status),
    };
    let store = match SessionStore::new(root.as_deref().unwrap_or_else(|| Path::new("."))) {
        Ok(store) => store,
        Err(issue) => return session_error(as_json, issue),
    };
    let now = match autoresearch_core::supervisor::now_ms() {
        Ok(now) => now,
        Err(issue) => return session_error(as_json, issue),
    };
    match store.import_legacy(config, operation.unwrap(), &prepared, now) {
        Ok(guard) => {
            emit(as_json, json!({"schema_version":1, "command":name, "ok":true,
                "import_report":prepared.report(), "session":guard.view(now), "operation_id":operation,
                "historical_only":true, "commands_executed":false,
                "qualification_required":guard.state().qualification.is_none()}),
                &format!("Session {}: historical import recorded.\nHistorical outcomes are unverified. Capture source and qualify its reference before optimization.", guard.state().session_id));
            ExitCode::SUCCESS
        }
        Err(issue) => session_error(as_json, issue),
    }
}
