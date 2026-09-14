use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt};

pub const MAX_CONFIG_BYTES: usize = 1024 * 1024;
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigError {
    pub field: String,
    pub message: String,
}

impl ConfigError {
    fn new(field: &str, message: &str) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionConfig {
    pub schema_version: u32,
    pub session_id: String,
    pub goal: String,
    pub source: Source,
    pub scope: Scope,
    pub checks: Vec<CommandSpec>,
    pub benchmark: CommandSpec,
    pub metric: Metric,
    pub sampling: Sampling,
    pub budget: Budget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub repository: String,
    pub commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    /// Relative literal paths: a trailing slash denotes a directory prefix.
    pub allowed_paths: Vec<String>,
    pub protected_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandSpec {
    pub executable: String,
    pub args: Vec<String>,
    pub cwd: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Lower,
    Higher,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricDomain {
    Positive,
    NonNegative,
    Finite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metric {
    pub name: String,
    pub unit: String,
    pub direction: Direction,
    pub domain: MetricDomain,
    /// Smallest useful improvement in the declared unit, not a percentage.
    pub minimum_improvement: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sampling {
    pub runs: u32,
    pub warmup: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub max_experiments: u32,
    pub active_seconds: u64,
    pub command_timeout_seconds: u64,
    pub deadline_unix_ms: u64,
    pub max_output_bytes: u64,
    pub max_artifact_bytes: u64,
}

/// Strict outcome vocabulary; no unknown or missing outcome becomes a success.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentOutcome {
    Kept,
    Discarded,
    Inconclusive,
    Failed,
    Cancelled,
}

/// Can only be created by validation. Deliberately not deserializable or mutable.
#[derive(Debug, Clone)]
pub struct ValidatedConfig(SessionConfig);

impl ValidatedConfig {
    pub fn from_json(bytes: &[u8]) -> Result<Self, ConfigError> {
        if bytes.len() > MAX_CONFIG_BYTES {
            return Err(ConfigError::new("config", "exceeds the 1 MiB input limit"));
        }
        // Deserialize directly into structs: duplicate fields are rejected too.
        let raw: SessionConfig = serde_json::from_slice(bytes).map_err(|error| {
            ConfigError::new(
                "config",
                &format!(
                    "invalid JSON or contract shape at line {}, column {}",
                    error.line(),
                    error.column()
                ),
            )
        })?;
        Self::try_from(raw)
    }

    pub fn get(&self) -> &SessionConfig {
        &self.0
    }

    /// Time-dependent gate, separate from structural validation for reproducibility.
    pub fn check_deadline(&self, now_unix_ms: u64) -> Result<(), ConfigError> {
        if self.0.budget.deadline_unix_ms <= now_unix_ms {
            return Err(ConfigError::new(
                "budget.deadline_unix_ms",
                "deadline has expired",
            ));
        }
        Ok(())
    }
}

impl TryFrom<SessionConfig> for ValidatedConfig {
    type Error = ConfigError;

    fn try_from(raw: SessionConfig) -> Result<Self, Self::Error> {
        if raw.schema_version != SCHEMA_VERSION {
            return Err(ConfigError::new(
                "schema_version",
                "unsupported schema version",
            ));
        }
        if raw.session_id.is_empty()
            || raw.session_id.len() > 64
            || !raw
                .session_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(ConfigError::new(
                "session_id",
                "use 1..64 ASCII letters, digits, hyphens or underscores",
            ));
        }
        nonempty(&raw.goal, "goal", 4096)?;
        nonempty(&raw.source.repository, "source.repository", 4096)?;
        if ![40, 64].contains(&raw.source.commit.len())
            || !raw.source.commit.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(ConfigError::new(
                "source.commit",
                "must be a full 40- or 64-character hexadecimal commit ID",
            ));
        }
        paths(&raw.scope.allowed_paths, "scope.allowed_paths", true)?;
        paths(&raw.scope.protected_paths, "scope.protected_paths", false)?;
        for path in &raw.scope.allowed_paths {
            if path
                .trim_end_matches('/')
                .split('/')
                .next()
                .is_some_and(|part| {
                    part.eq_ignore_ascii_case(".git") || part.eq_ignore_ascii_case(".auto")
                })
            {
                return Err(ConfigError::new(
                    "scope.allowed_paths",
                    ".git and .auto are reserved",
                ));
            }
            // Protected paths inside an allowed directory are valid exclusions.
            if raw.scope.protected_paths.iter().any(|protected| {
                path == protected || (protected.ends_with('/') && path.starts_with(protected))
            }) {
                return Err(ConfigError::new(
                    "scope.allowed_paths",
                    "an allowed path is covered by a protected path",
                ));
            }
        }
        if raw.checks.is_empty() || raw.checks.len() > 64 {
            return Err(ConfigError::new(
                "checks",
                "declare 1..64 behavior check commands",
            ));
        }
        for check in &raw.checks {
            command(check, "checks")?;
        }
        command(&raw.benchmark, "benchmark")?;
        let name = &raw.metric.name;
        if name.len() > 64
            || !name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(ConfigError::new(
                "metric.name",
                "must be an ASCII identifier starting with a letter, at most 64 bytes",
            ));
        }
        if ["run_min_ms", "run_max_ms"].contains(&name.as_str()) {
            return Err(ConfigError::new(
                "metric.name",
                "reserved diagnostic metric name",
            ));
        }
        if raw.metric.unit.len() > 32 || raw.metric.unit.chars().any(char::is_control) {
            return Err(ConfigError::new(
                "metric.unit",
                "must be at most 32 bytes without control characters",
            ));
        }
        if !raw.metric.minimum_improvement.is_finite() || raw.metric.minimum_improvement <= 0.0 {
            return Err(ConfigError::new(
                "metric.minimum_improvement",
                "must be finite and positive",
            ));
        }
        if !(3..=31).contains(&raw.sampling.runs) || raw.sampling.warmup > 31 {
            return Err(ConfigError::new(
                "sampling",
                "runs must be 3..31 and warmup 0..31",
            ));
        }
        let budget = &raw.budget;
        if budget.max_experiments == 0
            || budget.active_seconds == 0
            || budget.command_timeout_seconds == 0
            || budget.max_output_bytes == 0
            || budget.max_artifact_bytes == 0
            || budget.deadline_unix_ms == 0
        {
            return Err(ConfigError::new("budget", "all limits must be positive"));
        }
        if budget.active_seconds > u64::MAX / 1000 || budget.deadline_unix_ms > i64::MAX as u64 {
            return Err(ConfigError::new(
                "budget",
                "time limits exceed the supported range",
            ));
        }
        if budget.command_timeout_seconds > budget.active_seconds {
            return Err(ConfigError::new(
                "budget.command_timeout_seconds",
                "cannot exceed the active time budget",
            ));
        }
        if budget.max_output_bytes > budget.max_artifact_bytes {
            return Err(ConfigError::new(
                "budget.max_output_bytes",
                "cannot exceed the artifact budget",
            ));
        }
        Ok(Self(raw))
    }
}

fn nonempty(value: &str, field: &str, limit: usize) -> Result<(), ConfigError> {
    if value.trim().is_empty() || value.len() > limit || value.contains('\0') {
        return Err(ConfigError::new(
            field,
            "must be nonempty, within its size limit, and contain no NUL",
        ));
    }
    Ok(())
}

fn relative_path(path: &str, allow_root: bool) -> bool {
    if allow_root && path == "." {
        return true;
    }
    let normalized = path.strip_suffix('/').unwrap_or(path);
    !normalized.is_empty()
        && normalized.len() <= 4096
        && !normalized
            .chars()
            .any(|c| c.is_control() || "\\:*?[]".contains(c))
        && normalized
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn paths(items: &[String], field: &str, required: bool) -> Result<(), ConfigError> {
    if (required && items.is_empty()) || items.len() > 1024 {
        return Err(ConfigError::new(
            field,
            "must contain at most 1024 paths and a nonempty allowed scope",
        ));
    }
    let mut seen = HashSet::new();
    for item in items {
        if !relative_path(item, false) || !seen.insert(item.trim_end_matches('/')) {
            return Err(ConfigError::new(
                field,
                "use distinct relative literal paths without traversal or globs",
            ));
        }
    }
    Ok(())
}

fn command(spec: &CommandSpec, field: &str) -> Result<(), ConfigError> {
    nonempty(&spec.executable, field, 4096)?;
    if spec.executable.chars().any(char::is_control) || !relative_path(&spec.cwd, true) {
        return Err(ConfigError::new(
            field,
            "executable must have no control characters; cwd must stay relative to the workspace",
        ));
    }
    if spec.args.len() > 4096
        || spec
            .args
            .iter()
            .any(|arg| arg.len() > 65536 || arg.contains('\0'))
    {
        return Err(ConfigError::new(
            field,
            "arguments exceed size limits or contain NUL",
        ));
    }
    Ok(())
}
