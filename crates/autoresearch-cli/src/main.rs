use autoresearch_core::{ValidatedConfig, MAX_CONFIG_BYTES};
use serde_json::{json, Value};
use std::{
    env,
    ffi::OsString,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::ExitCode,
};

const HELP: &str = "Autoresearch Toolkit — Rust engine foundation

Usage:
  autoresearch doctor [--json]
  autoresearch validate --config <file> [--json]
  autoresearch --help
  autoresearch --version

doctor checks the supported platform and Git executable discovery (not its version).
validate checks the draft JSON contract without executing commands, inspecting the
source repository, checking the current deadline, or creating a session.
Experiment execution and the Pi adapter are not implemented yet.
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
        _ => error(
            as_json,
            "usage",
            "expected doctor or validate --config <file>; use --help",
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
        "capabilities": {"validate_config": true, "run_experiments": false, "pi_adapter": false}
    }), &format!(
        "Autoresearch {}\nPlatform: {} / {} (supported: {})\nGit executable on PATH: {} (version not verified)\nAvailable: configuration validation. Experiment execution is not implemented yet.",
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

fn validate(path: &Path, as_json: bool) -> ExitCode {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return error(as_json, "io", "cannot inspect configuration file", 1);
    };
    if !metadata.is_file() {
        return error(
            as_json,
            "io",
            "configuration must be a regular file, not a symlink or directory",
            1,
        );
    }
    if metadata.len() > MAX_CONFIG_BYTES as u64 {
        return error(
            as_json,
            "invalid_config",
            "configuration exceeds the 1 MiB input limit",
            2,
        );
    }
    let Ok(file) = fs::File::open(path) else {
        return error(as_json, "io", "cannot open configuration file", 1);
    };
    let mut bytes = Vec::new();
    if file
        .take(MAX_CONFIG_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return error(as_json, "io", "cannot read configuration file", 1);
    }
    match ValidatedConfig::from_json(&bytes) {
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
        Err(issue) => error(as_json, "invalid_config", &issue.to_string(), 2),
    }
}
