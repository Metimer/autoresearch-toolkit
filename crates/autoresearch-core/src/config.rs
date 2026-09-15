use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fmt,
};

pub const MAX_CONFIG_BYTES: usize = 1024 * 1024;
pub const SCHEMA_VERSION: u32 = 2;

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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionConfig {
    #[schemars(range(min = 2, max = 2))]
    pub schema_version: u32,
    #[schemars(length(min = 1, max = 64), regex(pattern = "^[A-Za-z0-9_-]+$"))]
    pub session_id: String,
    #[schemars(length(min = 1, max = 4096))]
    pub goal: String,
    pub source: Source,
    pub scope: Scope,
    #[schemars(length(min = 1, max = 64))]
    pub checks: Vec<CommandSpec>,
    pub benchmark: CommandSpec,
    pub metric: Metric,
    pub sampling: Sampling,
    pub budget: Budget,
    pub execution: ExecutionPolicy,
    #[schemars(length(max = 32))]
    pub secondary_constraints: Vec<SecondaryConstraint>,
    pub commit_policy: CommitPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Source {
    #[schemars(length(min = 1, max = 4096))]
    pub repository: String,
    #[schemars(regex(pattern = "^([A-Fa-f0-9]{40}|[A-Fa-f0-9]{64})$"))]
    pub commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    /// Relative literal paths: a trailing slash denotes a directory prefix.
    #[schemars(length(min = 1, max = 1024))]
    pub allowed_paths: Vec<String>,
    #[schemars(length(max = 1024))]
    pub protected_paths: Vec<String>,
    #[serde(deserialize_with = "distinct_map")]
    pub protected_sha256: BTreeMap<String, String>,
    #[schemars(length(max = 1024))]
    pub generated_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommandSpec {
    #[schemars(length(min = 1, max = 4096))]
    pub executable: String,
    #[schemars(length(max = 4096))]
    pub args: Vec<String>,
    pub cwd: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Lower,
    Higher,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MetricDomain {
    Positive,
    NonNegative,
    Finite,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Metric {
    #[schemars(length(min = 1, max = 64), regex(pattern = "^[A-Za-z][A-Za-z0-9_]*$"))]
    pub name: String,
    #[schemars(length(max = 32))]
    pub unit: String,
    pub direction: Direction,
    pub domain: MetricDomain,
    /// Smallest useful improvement in the declared unit, not a percentage.
    pub minimum_improvement: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Sampling {
    #[schemars(range(min = 3, max = 31))]
    pub runs: u32,
    #[schemars(range(max = 31))]
    pub warmup: u32,
    #[schemars(range(min = 3, max = 31))]
    pub baseline_rounds: u32,
    pub order: SampleOrder,
    pub cache: CachePolicy,
    #[schemars(regex(pattern = "^[A-Fa-f0-9]{64}$"))]
    pub input_sha256: String,
    #[schemars(length(max = 1024))]
    pub seeds: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    #[schemars(range(min = 1))]
    pub max_experiments: u32,
    #[schemars(range(min = 1, max = 18446744073709551_u64))]
    pub active_seconds: u64,
    #[schemars(range(min = 1, max = 18446744073709551_u64))]
    pub command_timeout_seconds: u64,
    #[schemars(range(min = 1, max = 9223372036854775807_u64))]
    pub deadline_unix_ms: u64,
    #[schemars(range(min = 1))]
    pub max_output_bytes: u64,
    #[schemars(range(min = 1))]
    pub max_artifact_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPolicy {
    pub environment: Environment,
    pub network: NetworkPolicy,
    #[schemars(length(max = 64))]
    pub setup: Vec<CommandSpec>,
    pub hooks: Hooks,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    pub inherit: Vec<String>,
    #[serde(deserialize_with = "distinct_map")]
    pub set: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Hooks {
    #[schemars(length(max = 64))]
    pub before: Vec<CommandSpec>,
    #[schemars(length(max = 64))]
    pub after: Vec<CommandSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CachePolicy {
    pub mode: CacheMode,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CacheMode {
    Cold,
    Warm,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SampleOrder {
    Balanced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NetworkPolicy {
    Disabled,
    Allowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CommitPolicy {
    Never,
    Explicit,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SecondaryConstraint {
    #[schemars(length(min = 1, max = 64), regex(pattern = "^[A-Za-z][A-Za-z0-9_]*$"))]
    pub name: String,
    #[schemars(length(max = 32))]
    pub unit: String,
    pub domain: MetricDomain,
    pub bound: ConstraintBound,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConstraintBound {
    AtMost { value: f64 },
    AtLeast { value: f64 },
}

/// Structural schema. Cross-field and filesystem checks are separate gates.
pub fn schema() -> schemars::schema::RootSchema {
    schemars::schema_for!(SessionConfig)
}

/// Strict outcome vocabulary; no unknown or missing outcome becomes a success.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
        extended_policy(&raw)?;
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

fn extended_policy(raw: &SessionConfig) -> Result<(), ConfigError> {
    paths(&raw.scope.generated_paths, "scope.generated_paths", false)?;
    let overlaps = |a: &str, b: &str| {
        a.trim_end_matches('/') == b.trim_end_matches('/')
            || (a.ends_with('/') && b.starts_with(a))
            || (b.ends_with('/') && a.starts_with(b))
    };
    for path in &raw.scope.generated_paths {
        if path
            .split('/')
            .next()
            .is_some_and(|p| p.eq_ignore_ascii_case(".git") || p.eq_ignore_ascii_case(".auto"))
            || raw
                .scope
                .allowed_paths
                .iter()
                .chain(&raw.scope.protected_paths)
                .any(|other| overlaps(path, other))
        {
            return Err(ConfigError::new(
                "scope.generated_paths",
                "must be disjoint from editable, protected and reserved paths",
            ));
        }
    }
    if raw.scope.protected_sha256.len() > 1024 {
        return Err(ConfigError::new(
            "scope.protected_sha256",
            "too many protected hashes",
        ));
    }
    for (path, hash) in &raw.scope.protected_sha256 {
        if !relative_path(path, false)
            || path.ends_with('/')
            || !sha256(hash)
            || !raw
                .scope
                .protected_paths
                .iter()
                .any(|p| path == p || (p.ends_with('/') && path.starts_with(p)))
        {
            return Err(ConfigError::new(
                "scope.protected_sha256",
                "each hash must cover a protected file and use SHA-256 hex",
            ));
        }
    }
    let sampling = &raw.sampling;
    if !(3..=31).contains(&sampling.baseline_rounds)
        || !sha256(&sampling.input_sha256)
        || sampling.seeds.len() > 1024
    {
        return Err(ConfigError::new(
            "sampling",
            "declare 3..31 baseline rounds, a SHA-256 workload identity and at most 1024 seeds",
        ));
    }
    paths(&sampling.cache.paths, "sampling.cache.paths", false)?;
    if sampling.cache.mode == CacheMode::None && !sampling.cache.paths.is_empty() {
        return Err(ConfigError::new(
            "sampling.cache",
            "none cache mode requires an empty path list",
        ));
    }
    for path in &sampling.cache.paths {
        if !raw
            .scope
            .generated_paths
            .iter()
            .any(|p| path == p || (p.ends_with('/') && path.starts_with(p)))
        {
            return Err(ConfigError::new(
                "sampling.cache.paths",
                "cache paths must be inside declared generated paths",
            ));
        }
    }
    let env = &raw.execution.environment;
    let valid_name = |name: &str| {
        !name.is_empty()
            && name.len() <= 128
            && name
                .as_bytes()
                .first()
                .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    };
    let mut names = HashSet::new();
    if env.inherit.len() + env.set.len() > 256 {
        return Err(ConfigError::new(
            "execution.environment",
            "too many environment entries",
        ));
    }
    for name in env.inherit.iter().chain(env.set.keys()) {
        if !valid_name(name) || !names.insert(name) {
            return Err(ConfigError::new(
                "execution.environment",
                "environment names must be valid and not overlap",
            ));
        }
    }
    if env
        .set
        .values()
        .any(|value| value.len() > 65536 || value.contains('\0'))
    {
        return Err(ConfigError::new(
            "execution.environment",
            "environment values exceed limits or contain NUL",
        ));
    }
    let commands = [
        &raw.execution.setup,
        &raw.execution.hooks.before,
        &raw.execution.hooks.after,
    ];
    for group in commands {
        if group.len() > 64 {
            return Err(ConfigError::new(
                "execution",
                "too many setup or hook commands",
            ));
        }
        for spec in group {
            command(spec, "execution")?;
        }
    }
    if raw.secondary_constraints.len() > 32 {
        return Err(ConfigError::new(
            "secondary_constraints",
            "at most 32 constraints are supported",
        ));
    }
    let mut metrics = HashSet::from([raw.metric.name.as_str()]);
    for constraint in &raw.secondary_constraints {
        let value = match constraint.bound {
            ConstraintBound::AtMost { value } | ConstraintBound::AtLeast { value } => value,
        };
        if !constraint
            .name
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic)
            || constraint.name.len() > 64
            || !constraint
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || !metrics.insert(constraint.name.as_str())
            || constraint.unit.len() > 32
            || constraint.unit.chars().any(char::is_control)
            || !value.is_finite()
            || (constraint.domain == MetricDomain::Positive && value <= 0.0)
            || (constraint.domain == MetricDomain::NonNegative && value < 0.0)
        {
            return Err(ConfigError::new(
                "secondary_constraints",
                "invalid, duplicate or out-of-domain constraint",
            ));
        }
    }
    Ok(())
}

fn sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

// Serde structs reject duplicate fields; maps need the same explicit protection.
fn distinct_map<'de, D>(deserializer: D) -> Result<BTreeMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Distinct;
    impl<'de> serde::de::Visitor<'de> for Distinct {
        type Value = BTreeMap<String, String>;
        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("an object with distinct keys")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut values = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, String>()? {
                if values.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("duplicate object key"));
                }
            }
            Ok(values)
        }
    }
    deserializer.deserialize_map(Distinct)
}
