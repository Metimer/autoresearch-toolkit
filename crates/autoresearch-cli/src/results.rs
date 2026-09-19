use crate::{emit, error, session_error};
use autoresearch_core::{results::Selection, session::SessionStore, supervisor};
use serde_json::json;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::ExitCode,
};

pub fn command(name: &str, args: &[OsString], as_json: bool) -> ExitCode {
    let mut root: Option<PathBuf> = None;
    let mut session = None;
    let mut evaluation = None;
    let mut output: Option<PathBuf> = None;
    let mut operation = None;
    let mut query = None;
    let mut selection = Selection::default();
    let is_result = matches!(name, "result-preview" | "export-result");
    let mut options = args.iter();
    while let Some(flag) = options.next() {
        match flag.to_str() {
            Some("--include-code") if is_result && !selection.code => {
                selection.code = true;
                continue;
            }
            Some("--include-protocol") if is_result && !selection.protocol => {
                selection.protocol = true;
                continue;
            }
            _ => {}
        }
        let Some(value) = options.next() else {
            return error(as_json, "usage", "missing option value; use --help", 2);
        };
        let text = value.to_str();
        match flag.to_str() {
            Some("--root") if root.is_none() => root = Some(value.into()),
            Some("--session") if is_result && session.is_none() && text.is_some() => session = text,
            Some("--evaluation") if is_result && evaluation.is_none() && text.is_some() => {
                evaluation = text
            }
            Some("--output") if name == "export-result" && output.is_none() => {
                output = Some(value.into())
            }
            Some("--operation-id")
                if matches!(name, "index" | "export-result")
                    && operation.is_none()
                    && text.is_some() =>
            {
                operation = text
            }
            Some("--query") if name == "memory" && query.is_none() && text.is_some() => {
                query = text
            }
            Some("--log") if is_result && text.is_some() => {
                selection.logs.push(text.unwrap().into())
            }
            _ => {
                return error(
                    as_json,
                    "usage",
                    "unknown, repeated or non-UTF-8 result option; use --help",
                    2,
                )
            }
        }
    }
    if (is_result && (session.is_none() || evaluation.is_none()))
        || (matches!(name, "index" | "export-result") && operation.is_none())
        || (name == "export-result" && output.is_none())
        || query.is_some_and(|q| q.len() > 4096)
    {
        return error(
            as_json,
            "usage",
            "missing required result options or query exceeds 4096 bytes; use --help",
            2,
        );
    }
    let store = match SessionStore::new(root.as_deref().unwrap_or_else(|| Path::new("."))) {
        Ok(store) => store,
        Err(e) => return session_error(as_json, e),
    };
    if !is_result {
        let index = match if name == "index" {
            store.rebuild_memory(operation.unwrap())
        } else {
            store.memory()
        } {
            Ok(index) => index,
            Err(e) => return session_error(as_json, e),
        };
        let rows = index.search(query.unwrap_or(""));
        let text = rows
            .iter()
            .map(|row| {
                format!(
                    "{}/{}  {:?}  {}  duplicates: {}",
                    row.session_id,
                    row.evaluation,
                    row.decision,
                    row.reason,
                    row.duplicate_of.len()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        emit(
            as_json,
            json!({"schema_version":1,"command":name,"ok":true,"index_version":index.format_version,
            "session_sequences":index.session_sequences,"rows":rows,"cache_rebuilt":name=="index",
            "commands_executed":false,"automatic_rejection":false}),
            if text.is_empty() {
                "No matching evaluations."
            } else {
                &text
            },
        );
        return ExitCode::SUCCESS;
    }
    let mut guard = match store.open(session.unwrap(), false) {
        Ok(guard) => guard,
        Err(e) => return session_error(as_json, e),
    };
    if name == "result-preview" {
        match guard.prepare_result(evaluation.unwrap(), selection) {
            Ok(result) => {
                let preview = result.preview();
                let mut text = format!(
                    "Bundle SHA-256: {}\nBytes: {}\nSelected files (plus manifest.json):",
                    preview.sha256, preview.total_bytes
                );
                for (path, file) in &preview.manifest.files {
                    text.push_str(&format!("\n{}  {} bytes", path, file.bytes));
                }
                emit(
                    as_json,
                    json!({"schema_version":1,"command":name,"ok":true,"preview":preview,"commands_executed":false,"files_published":false}),
                    &text,
                );
                ExitCode::SUCCESS
            }
            Err(e) => session_error(as_json, e),
        }
    } else {
        let now = match supervisor::now_ms() {
            Ok(now) => now,
            Err(e) => return session_error(as_json, e),
        };
        match guard.export_result(
            operation.unwrap(),
            evaluation.unwrap(),
            selection,
            &output.unwrap(),
            now,
        ) {
            Ok(result) => {
                emit(
                    as_json,
                    json!({"schema_version":1,"command":name,"ok":true,"result":result,"commands_executed":false}),
                    &format!(
                        "Result bundle: {}\nSHA-256: {}",
                        result.path.display(),
                        result.sha256
                    ),
                );
                ExitCode::SUCCESS
            }
            Err(e) => session_error(as_json, e),
        }
    }
}
