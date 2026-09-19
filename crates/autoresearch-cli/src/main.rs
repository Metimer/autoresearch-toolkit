mod inspection;
mod legacy;

use autoresearch_core::{
    session::{SessionError, SessionStore},
    workspace::LocalChanges,
    ValidatedConfig, MAX_CONFIG_BYTES,
};
use serde_json::{json, Value};
use std::{
    env,
    ffi::OsString,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::ExitCode,
};

const HELP: &str = "Autoresearch Toolkit — Rust session engine

Usage:
  autoresearch doctor [--json]
  autoresearch validate --config <file> [--json]
  autoresearch schema
  autoresearch inspect-legacy --source <jsonl> [--format <auto|pi|portable>] [--json]
  autoresearch import-legacy --source <jsonl> --config <file> --operation-id <id> [--format <auto|pi|portable>] [--root <dir>] [--json]
  autoresearch init --config <file> --operation-id <id> [--root <dir>] [--json]
  autoresearch status --session <id> [--root <dir>] [--json]
  autoresearch history --session <id> [--root <dir>] [--json]
  autoresearch report --session <id> [--evaluation <run-key> | --legacy] [--root <dir>] [--json]
  autoresearch resume --session <id> --operation-id <id> [--root <dir>] [--repair-tail] [--json]
  autoresearch stop --session <id> --operation-id <id> [--root <dir>] [--json]
  autoresearch workspace --session <id> --local-changes <exclude|include> --operation-id <id> [--root <dir>] [--json]
  autoresearch prepare-candidate --session <id> --candidate <id> --hypothesis <text> --operation-id <id> [--root <dir>] [--json]
  autoresearch seal --session <id> --candidate <id> --operation-id <id> [--root <dir>] [--json]
  autoresearch export-candidate --session <id> --candidate <id> --output <new-dir> --operation-id <id> [--root <dir>] [--json]
  autoresearch baseline --session <id> --operation-id <id> [--root <dir>] [--json]
  autoresearch evaluate --session <id> --candidate <id> --operation-id <id> [--root <dir>] [--json]
  autoresearch --help
  autoresearch --version

doctor checks the supported platform and Git executable discovery (not its version).
validate checks the draft JSON contract without executing commands, inspecting the
source repository, checking the current deadline, or creating a session.
Session commands persist metadata below the explicit root (default: current directory).
Use the same operation ID to retry a mutation; use a new ID for a new operation.
baseline qualifies the captured reference; evaluate checks and measures a sealed candidate.
stop requests cancellation when a supervisor owns the session lock.
import-legacy creates a new session with inert history; no commands or old verdicts are resumed.
history lists completed evaluations; report defaults to the qualified accepted reference.
Read evaluation.report.decision: exit zero also covers rejected or cancelled evaluations.
Workspace commands use isolated Git plumbing and never run project commands.
Execution requires trusted commands and network=allowed; there is no network sandbox.
The optional Pi adapter is available separately in adapters/pi/.
";

fn main() -> ExitCode {
    let mut args: Vec<OsString> = env::args_os().skip(1).collect();
    let json_count = args.iter().filter(|arg| *arg == "--json").count();
    let as_json = json_count > 0;
    args.retain(|arg| arg != "--json");
    if json_count > 1 {
        return error(as_json, "usage", "--json may only be specified once", 2);
    }
    match args.as_slice() {
        [command] if command == "--help" || command == "-h" => {
            emit(
                as_json,
                json!({"schema_version": 1, "ok": true, "command": "help", "help": HELP}),
                HELP,
            );
            ExitCode::SUCCESS
        }
        [command] if command == "--version" || command == "-V" => {
            let version = env!("CARGO_PKG_VERSION");
            emit(
                as_json,
                json!({"schema_version": 1, "ok": true, "command": "version", "version": version}),
                &format!("autoresearch {version}"),
            );
            ExitCode::SUCCESS
        }
        [command] if command == "doctor" => doctor(as_json),
        [command, flag, file] if command == "validate" && flag == "--config" => {
            validate(Path::new(file), as_json)
        }
        [command] if command == "schema" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&autoresearch_core::config::schema())
                    .expect("schema is serializable")
            );
            ExitCode::SUCCESS
        }
        [command, rest @ ..] if command == "inspect-legacy" || command == "import-legacy" => {
            legacy::command(command.to_str().unwrap(), rest, as_json)
        }
        [command, rest @ ..]
            if [
                "init",
                "status",
                "history",
                "report",
                "resume",
                "stop",
                "workspace",
                "prepare-candidate",
                "seal",
                "export-candidate",
                "baseline",
                "evaluate",
            ]
            .iter()
            .any(|name| command == name) =>
        {
            session_command(
                command.to_str().expect("matched ASCII command"),
                rest,
                as_json,
            )
        }
        _ => error(
            as_json,
            "usage",
            "invalid command or arguments; use --help",
            2,
        ),
    }
}

fn emit(as_json: bool, value: Value, text: &str) {
    if as_json {
        println!("{value}");
    } else {
        println!("{text}");
    }
}

fn error(as_json: bool, code: &str, message: &str, exit: u8) -> ExitCode {
    if as_json {
        println!(
            "{}",
            json!({"schema_version": 1, "ok": false, "error": {"code": code, "message": message}})
        );
    } else {
        eprintln!("{code}: {message}");
    }
    ExitCode::from(exit)
}

fn doctor(as_json: bool) -> ExitCode {
    let supported = cfg!(any(target_os = "linux", target_os = "macos"));
    let git_found = find_git().is_some();
    let ready = supported && git_found;
    emit(as_json, json!({
        "schema_version": 1, "command": "doctor", "ok": ready,
        "version": env!("CARGO_PKG_VERSION"),
        "platform": {"os": env::consts::OS, "arch": env::consts::ARCH, "supported": supported},
        "git_on_path": git_found,
        "git_version_verified": false,
        "capabilities": {"validate_config": true, "manage_sessions": supported, "isolated_workspaces": supported && git_found, "run_experiments": supported && git_found, "pi_adapter": supported, "hook_protocol_version": 1, "inspect_evaluations": supported, "import_legacy": supported}
    }), &format!(
        "Autoresearch {}\nPlatform: {} / {} (supported: {})\nGit executable on PATH: {} (version not verified)\nAvailable: configuration validation, persistent sessions and isolated snapshots. Trusted command supervision and paired evaluations are available.",
        env!("CARGO_PKG_VERSION"), env::consts::OS, env::consts::ARCH, supported, git_found
    ));
    if ready {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(3)
    }
}

fn find_git() -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|base| base.join("git"))
        .find(|file| {
            let Ok(metadata) = file.metadata() else {
                return false;
            };
            if !metadata.is_file() {
                return false;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o111 != 0
            }
            #[cfg(not(unix))]
            {
                false
            }
        })
}

fn load_config(path: &Path) -> Result<ValidatedConfig, (&'static str, String, u8)> {
    let io = |message: &str| ("io", message.to_owned(), 1);
    let metadata =
        fs::symlink_metadata(path).map_err(|_| io("cannot inspect configuration file"))?;
    if !metadata.is_file() {
        return Err(io(
            "configuration must be a regular file, not a symlink or directory",
        ));
    }
    if metadata.len() > MAX_CONFIG_BYTES as u64 {
        return Err((
            "invalid_config",
            "configuration exceeds the 1 MiB input limit".into(),
            2,
        ));
    }
    let file = fs::File::open(path).map_err(|_| io("cannot open configuration file"))?;
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| io("cannot read configuration file"))?;
    ValidatedConfig::from_json(&bytes).map_err(|issue| ("invalid_config", issue.to_string(), 2))
}

fn validate(path: &Path, as_json: bool) -> ExitCode {
    match load_config(path) {
        Ok(config) => {
            emit(as_json, json!({
                "schema_version": 1, "command": "validate", "ok": true,
                "session_id": config.get().session_id,
                "validation": "structural_and_semantic",
                "repository_verified": false,
                "deadline_checked": false,
                "commands_executed": false
            }), "Configuration is valid. Repository, current deadline and command behavior have not been checked. No session was created.");
            ExitCode::SUCCESS
        }
        Err((code, message, exit)) => error(as_json, code, &message, exit),
    }
}

#[derive(Default)]
struct SessionArgs {
    root: Option<PathBuf>,
    config: Option<PathBuf>,
    session: Option<String>,
    operation: Option<String>,
    repair: bool,
    candidate: Option<String>,
    hypothesis: Option<String>,
    local_changes: Option<LocalChanges>,
    output: Option<PathBuf>,
    evaluation: Option<String>,
    legacy: bool,
}

fn is_read_only(command: &str) -> bool {
    ["status", "history", "report"].contains(&command)
}

fn session_args(command: &str, args: &[OsString]) -> Result<SessionArgs, &'static str> {
    let mut parsed = SessionArgs::default();
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        if flag == "--legacy" && command == "report" && !parsed.legacy {
            parsed.legacy = true;
            continue;
        }
        if flag == "--repair-tail" && command == "resume" && !parsed.repair {
            parsed.repair = true;
            continue;
        }
        let value = iter.next().ok_or("missing option value")?;
        match flag.to_str() {
            Some("--root") if parsed.root.is_none() => parsed.root = Some(value.into()),
            Some("--config") if command == "init" && parsed.config.is_none() => {
                parsed.config = Some(value.into())
            }
            Some("--session") if command != "init" && parsed.session.is_none() => {
                parsed.session = Some(value.to_str().ok_or("session ID must be UTF-8")?.into());
            }
            Some("--operation-id") if !is_read_only(command) && parsed.operation.is_none() => {
                let id = value.to_str().ok_or("operation ID must be UTF-8")?;
                if id.is_empty()
                    || id.len() > 128
                    || !id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                {
                    return Err("operation ID must use 1..128 ASCII letters, digits, hyphens or underscores");
                }
                parsed.operation = Some(id.into());
            }
            Some("--candidate")
                if ["prepare-candidate", "seal", "export-candidate", "evaluate"]
                    .contains(&command)
                    && parsed.candidate.is_none() =>
            {
                parsed.candidate = Some(value.to_str().ok_or("candidate must be UTF-8")?.into())
            }
            Some("--hypothesis")
                if command == "prepare-candidate" && parsed.hypothesis.is_none() =>
            {
                parsed.hypothesis = Some(value.to_str().ok_or("hypothesis must be UTF-8")?.into())
            }
            Some("--local-changes") if command == "workspace" && parsed.local_changes.is_none() => {
                parsed.local_changes = Some(match value.to_str() {
                    Some("exclude") => LocalChanges::Exclude,
                    Some("include") => LocalChanges::Include,
                    _ => return Err("local changes policy must be exclude or include"),
                })
            }
            Some("--evaluation") if command == "report" && parsed.evaluation.is_none() => {
                parsed.evaluation =
                    Some(value.to_str().ok_or("evaluation key must be UTF-8")?.into())
            }
            Some("--output") if command == "export-candidate" && parsed.output.is_none() => {
                parsed.output = Some(value.into())
            }
            _ => return Err("unknown, repeated or unsupported session option"),
        }
    }
    if (command == "init" && parsed.config.is_none())
        || (command != "init" && parsed.session.is_none())
        || (!is_read_only(command) && parsed.operation.is_none())
        || (command == "workspace" && parsed.local_changes.is_none())
        || (["prepare-candidate", "seal", "export-candidate", "evaluate"].contains(&command)
            && parsed.candidate.is_none())
        || (command == "prepare-candidate" && parsed.hypothesis.is_none())
        || (command == "export-candidate" && parsed.output.is_none())
        || (parsed.legacy && parsed.evaluation.is_some())
    {
        return Err("missing required session option; use --help");
    }
    Ok(parsed)
}

fn session_command(command: &str, args: &[OsString], as_json: bool) -> ExitCode {
    let args = match session_args(command, args) {
        Ok(args) => args,
        Err(message) => return error(as_json, "usage", message, 2),
    };
    let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(time) => match u64::try_from(time.as_millis()) {
            Ok(ms) => ms,
            Err(_) => {
                return error(
                    as_json,
                    "clock_regressed",
                    "system clock is outside the supported range",
                    7,
                )
            }
        },
        Err(_) => {
            return error(
                as_json,
                "clock_regressed",
                "system clock precedes the Unix epoch",
                7,
            )
        }
    };
    let root = args.root.as_deref().unwrap_or_else(|| Path::new("."));
    let store = match SessionStore::new(root) {
        Ok(store) => store,
        Err(issue) => return session_error(as_json, issue),
    };
    let opened = if command == "init" {
        let config = match load_config(args.config.as_deref().expect("checked config")) {
            Ok(config) => config,
            Err((code, message, exit)) => return error(as_json, code, &message, exit),
        };
        store.init(
            config,
            args.operation.as_deref().expect("checked operation"),
            now,
        )
    } else {
        store.open(
            args.session.as_deref().expect("checked session"),
            args.repair,
        )
    };
    let mut guard = match opened {
        Ok(guard) => guard,
        Err(issue) if command == "stop" && issue.code == "session_busy" => {
            return match store.request_stop(
                args.session.as_deref().unwrap(),
                args.operation.as_deref().unwrap(),
            ) {
                Ok(token) => {
                    emit(as_json,json!({"schema_version":1,"command":"stop","ok":true,"cancellation_requested":true,"execution_token":token}),"Cancellation requested; the supervisor will terminate its owned process group.");
                    ExitCode::SUCCESS
                }
                Err(issue) => session_error(as_json, issue),
            };
        }
        Err(issue) => return session_error(as_json, issue),
    };
    if ["history", "report"].contains(&command) {
        return match inspection::inspect(&guard, command, args.evaluation.as_deref(), args.legacy) {
            Ok((value, text)) => {
                emit(as_json, value, &text);
                ExitCode::SUCCESS
            }
            Err(issue) => session_error(as_json, issue),
        };
    }
    if ["baseline", "evaluate"].contains(&command) {
        let result = if command == "baseline" {
            guard.baseline(args.operation.as_deref().unwrap())
        } else {
            guard.evaluate_candidate(
                args.operation.as_deref().unwrap(),
                args.candidate.as_deref().unwrap(),
            )
        };
        return match result {
            Ok(evaluation) => {
                emit(
                    as_json,
                    json!({"schema_version":1,"command":command,"ok":true,"evaluation":evaluation,"session":guard.view(autoresearch_core::supervisor::now_ms().unwrap_or(now))}),
                    &format!(
                        "Decision: {:?}\nReason: {}\nEvidence SHA-256: {}",
                        evaluation.report.decision, evaluation.report.reason, evaluation.sha256
                    ),
                );
                ExitCode::SUCCESS
            }
            Err(issue) => session_error(as_json, issue),
        };
    }
    if ["workspace", "prepare-candidate", "seal", "export-candidate"].contains(&command) {
        let operation = args.operation.as_deref().expect("checked operation");
        let result = match command {
            "workspace" => {
                guard.create_workspace(operation, args.local_changes.expect("checked policy"), now)
            }
            "prepare-candidate" => guard.prepare_candidate(
                operation,
                args.candidate.as_deref().unwrap(),
                args.hypothesis.as_deref().unwrap(),
                now,
            ),
            "seal" => guard.seal_candidate(operation, args.candidate.as_deref().unwrap(), now),
            "export-candidate" => guard.export_candidate(
                operation,
                args.candidate.as_deref().unwrap(),
                args.output.as_deref().unwrap(),
                now,
            ),
            _ => unreachable!(),
        };
        return match result {
            Ok(artifact) => {
                emit(
                    as_json,
                    json!({"schema_version":1, "command":command, "ok":true, "operation_id":operation, "artifact":artifact, "session":guard.view(now), "experiments_executed":false}),
                    &format!(
                        "{}: {}\nPath: {}\nSHA-256: {}\nFiles: {}; evaluated: false",
                        command,
                        artifact.artifact,
                        artifact.path.display(),
                        artifact.sha256,
                        artifact.files
                    ),
                );
                ExitCode::SUCCESS
            }
            Err(issue) => session_error(as_json, issue),
        };
    }
    let mutation = match command {
        "resume" => guard.resume(args.operation.as_deref().expect("checked operation"), now),
        "stop" => guard.stop(args.operation.as_deref().expect("checked operation"), now),
        _ => Ok(false),
    };
    let already_applied = match mutation {
        Ok(value) => value,
        Err(issue) => return session_error(as_json, issue),
    };
    let view = guard.view(now);
    let next_action = inspection::next_action(&view);
    let mut result = json!({"schema_version": 1, "command": command, "ok": true, "session": view, "next_action": next_action, "commands_executed": false});
    if command == "resume" || command == "stop" {
        result["already_applied"] = json!(already_applied);
    }
    if let Some(operation) = args.operation {
        result["operation_id"] = json!(operation);
    }
    emit(as_json, result, &format!("Session {}: {:?} (event {})\nBudget remaining: {} attempts, {} ms; deadline expired: {}\nRecovery required: {}; state projection current: {}\nNext action: {}",
        view.state.session_id, view.state.status, view.state.sequence, view.attempts_remaining,
        view.active_ms_remaining, view.deadline_expired, view.recovery_required, view.projection_current, next_action));
    ExitCode::SUCCESS
}

fn session_error(as_json: bool, issue: SessionError) -> ExitCode {
    let exit = match issue.code {
        "invalid_config"
        | "invalid_identifier"
        | "invalid_protocol"
        | "invalid_environment"
        | "invalid_legacy" => 2,
        "unsupported" => 3,
        "conflict" | "session_busy" | "source_changed" | "baseline_required" | "baseline_stale"
        | "report_not_found" => 4,
        "corrupt_session" | "corrupt_artifact" | "projection_failed" | "unsafe_path"
        | "storage_limit" | "recovery_required" => 5,
        "budget_exhausted" => 6,
        "clock_regressed" => 7,
        "git_failed" | "invalid_source" | "scope_violation" => 8,
        _ => 1,
    };
    error(as_json, issue.code, &issue.message, exit)
}
