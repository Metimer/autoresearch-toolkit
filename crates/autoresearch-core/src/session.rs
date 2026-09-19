//! Durable local session metadata. No benchmark, hook or Git command is executed here.
use crate::{SessionConfig, ValidatedConfig, MAX_CONFIG_BYTES};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const JOURNAL_LIMIT: usize = 64 * 1024 * 1024;
const RECORD_LIMIT: usize = 16 * 1024;
const FORMAT_VERSION: u32 = 1;

#[derive(Debug)]
pub struct SessionError {
    pub code: &'static str,
    pub message: String,
}
impl SessionError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for SessionError {}
impl From<std::io::Error> for SessionError {
    fn from(error: std::io::Error) -> Self {
        Self::new("io", error.to_string())
    }
}
type Result<T> = std::result::Result<T, SessionError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Created,
    Active,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reservation {
    pub operation_id: String,
    pub reserved_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Execution {
    pub operation_id: String,
    pub candidate: Option<String>,
    pub parent: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Qualified,
    Kept,
    Discarded,
    Inconclusive,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionState {
    pub format_version: u32,
    pub session_id: String,
    pub config_sha256: String,
    pub status: SessionStatus,
    pub sequence: u64,
    pub last_event_unix_ms: u64,
    pub attempts_used: u32,
    pub active_ms_used: u64,
    pub in_flight: Option<Reservation>,
    #[serde(default)]
    pub artifacts: BTreeMap<String, String>,
    #[serde(default)]
    pub accepted: Option<String>,
    #[serde(default)]
    pub qualification: Option<String>,
    #[serde(default)]
    pub execution: Option<Execution>,
    #[serde(default)]
    pub evaluated: BTreeMap<String, String>,
    #[serde(default)]
    pub legacy_import: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionView {
    pub state: SessionState,
    pub attempts_remaining: u32,
    pub active_ms_remaining: u64,
    pub deadline_unix_ms: u64,
    pub deadline_expired: bool,
    pub clock_regressed: bool,
    pub recovery_required: bool,
    pub projection_current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Event {
    ExecutionStarted {
        candidate: Option<String>,
        parent: String,
    },
    StageReserved {
        reserved_ms: u64,
    },
    StageSettled {
        reservation_id: String,
        elapsed_ms: u64,
    },
    ExecutionFinished {
        report_key: String,
        sha256: String,
        decision: Decision,
        invalidate_reference: bool,
    },
    ArtifactPublished {
        key: String,
        sha256: String,
    },
    Created {
        config_sha256: String,
    },
    Imported {
        config_sha256: String,
        import_sha256: String,
    },
    Resumed,
    Stopped,
    WorkReserved {
        reserved_ms: u64,
    },
    WorkSettled {
        reservation_id: String,
        elapsed_ms: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format_version: u32,
    sequence: u64,
    operation_id: String,
    at_unix_ms: u64,
    event: Event,
    previous_sha256: String,
    sha256: String,
}

impl Record {
    fn digest(&self) -> String {
        hash(
            &serde_json::to_vec(&(
                self.format_version,
                self.sequence,
                &self.operation_id,
                self.at_unix_ms,
                &self.event,
                &self.previous_sha256,
            ))
            .expect("record contains only serializable finite data"),
        )
    }
}

/// The project root is explicit and trusted. Owned paths below it reject symlinks.
pub struct SessionStore {
    root: PathBuf,
}

impl SessionStore {
    pub fn new(project_root: &Path) -> Result<Self> {
        let root = project_root.canonicalize()?;
        if !root.is_dir() {
            return Err(SessionError::new(
                "io",
                "project root must be an existing directory",
            ));
        }
        if !cfg!(any(target_os = "linux", target_os = "macos")) {
            return Err(SessionError::new(
                "unsupported",
                "session storage currently supports Linux and macOS",
            ));
        }
        Ok(Self { root })
    }

    pub(crate) fn session_path(&self, id: &str) -> Result<PathBuf> {
        identifier(id)?;
        let path = self.directory(false)?.join(id);
        check_directory(&path)?;
        Ok(path)
    }

    pub(crate) fn directory(&self, create: bool) -> Result<PathBuf> {
        let mut path = self.root.clone();
        for name in [".auto", "engine", "sessions"] {
            path.push(name);
            if create {
                let mut builder = fs::DirBuilder::new();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                match builder.create(&path) {
                    Ok(()) => sync_directory(path.parent().expect("owned path has parent"))?,
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.into()),
                }
            }
            check_directory(&path)?;
        }
        Ok(path)
    }

    pub fn init(
        &self,
        config: ValidatedConfig,
        operation_id: &str,
        now: u64,
    ) -> Result<SessionGuard> {
        self.initialize(config, operation_id, now, None)
    }

    pub fn import_legacy(
        &self,
        config: ValidatedConfig,
        operation_id: &str,
        prepared: &crate::legacy::PreparedImport,
        now: u64,
    ) -> Result<SessionGuard> {
        if !prepared.report().importable {
            return Err(SessionError::new(
                "invalid_legacy",
                "inspect and resolve historical journal anomalies before importing",
            ));
        }
        self.initialize(config, operation_id, now, Some(prepared))
    }

    fn initialize(
        &self,
        config: ValidatedConfig,
        operation_id: &str,
        now: u64,
        imported: Option<&crate::legacy::PreparedImport>,
    ) -> Result<SessionGuard> {
        identifier(operation_id)?;
        let directory = self.directory(true)?;
        // Serialize publication; the stable lock files must never be removed.
        let _publication_lock = acquire(&directory.join(".init.lock"), true)?;
        let path = directory.join(&config.get().session_id);
        let bytes = serde_json::to_vec(config.get())
            .map_err(|e| SessionError::new("invalid_config", e.to_string()))?;
        let digest = hash(&bytes);
        let historical = imported.map(|source| source.report_bytes()).transpose()?;
        if let (Some(source), Some(report)) = (imported, &historical) {
            let needed = source
                .report()
                .source_bytes
                .saturating_add(report.len())
                .saturating_add(bytes.len() * 2)
                .saturating_add(65536);
            if needed as u64 > config.get().budget.max_artifact_bytes {
                return Err(SessionError::new(
                    "storage_limit",
                    "historical source and report exceed the session storage budget",
                ));
            }
        }
        let created = if let Some(report) = &historical {
            Event::Imported {
                config_sha256: digest.clone(),
                import_sha256: hash(report),
            }
        } else {
            Event::Created {
                config_sha256: digest.clone(),
            }
        };
        let exists = match path.symlink_metadata() {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        if exists {
            let mut guard = self.open(&config.get().session_id, false)?;
            if guard.state.config_sha256 == digest
                && guard.operations.get(operation_id) == Some(&created)
            {
                guard.confirm_committed()?;
                guard.save_projection()?;
                sync_directory(&directory)?;
                return Ok(guard);
            }
            return Err(SessionError::new(
                "conflict",
                "session already exists with another configuration or initialization operation",
            ));
        }
        config
            .check_deadline(now)
            .map_err(|error| SessionError::new("budget_exhausted", error.to_string()))?;
        let staging = tempfile::Builder::new()
            .prefix(".creating-")
            .tempdir_in(&directory)?;
        write_new(&staging.path().join("session.json"), &bytes)?;
        let lock = acquire(&staging.path().join("lock"), true)?;
        let mut guard = SessionGuard {
            path: staging.path().to_path_buf(),
            _lock: lock,
            config,
            state: SessionState {
                format_version: FORMAT_VERSION,
                session_id: String::new(),
                config_sha256: digest.clone(),
                status: SessionStatus::Created,
                sequence: 0,
                last_event_unix_ms: 0,
                attempts_used: 0,
                active_ms_used: 0,
                in_flight: None,
                artifacts: BTreeMap::new(),
                accepted: None,
                qualification: None,
                execution: None,
                evaluated: BTreeMap::new(),
                legacy_import: None,
            },
            previous_hash: String::new(),
            operations: HashMap::new(),
            journal_bytes: 0,
            uncertain: false,
            projection_current: false,
        };
        write_new(&guard.path.join("events.jsonl"), b"")?;
        if let (Some(source), Some(report)) = (imported, &historical) {
            source.write(&staging.path().join("legacy"), report)?;
        }
        guard.record(operation_id, now, created)?;
        sync_directory(staging.path())?;
        // Under the publication lock a cooperating initializer cannot race this rename.
        fs::rename(staging.path(), &path)?;
        guard.path = path;
        sync_directory(&directory)?;
        Ok(guard)
    }

    /// `repair_tail` explicitly preserves then removes a non-newline-terminated tail.
    /// A malformed complete record or corrupt prefix is never repaired automatically.
    pub fn open(&self, session_id: &str, repair_tail: bool) -> Result<SessionGuard> {
        identifier(session_id)?;
        let path = self.directory(false)?.join(session_id);
        check_directory(&path)?;
        let lock = acquire(&path.join("lock"), false)?;
        let config_bytes = read_bounded(&path.join("session.json"), MAX_CONFIG_BYTES)?;
        let config = ValidatedConfig::from_json(&config_bytes).map_err(|_| {
            SessionError::new(
                "corrupt_session",
                "stored configuration is invalid or unsupported",
            )
        })?;
        if config.get().session_id != session_id {
            return Err(SessionError::new(
                "corrupt_session",
                "session directory and configuration identities differ",
            ));
        }
        let mut bytes = read_bounded(&path.join("events.jsonl"), JOURNAL_LIMIT)?;
        if !bytes.is_empty() && !bytes.ends_with(b"\n") {
            if !repair_tail {
                return Err(SessionError::new("corrupt_session", "journal has an unfinished tail; inspect it, then use resume --repair-tail to preserve and remove the tail"));
            }
            let end = bytes
                .iter()
                .rposition(|b| *b == b'\n')
                .map(|i| i + 1)
                .ok_or_else(|| {
                    SessionError::new(
                        "corrupt_session",
                        "journal has no complete initialization record",
                    )
                })?;
            let (prefix_state, _, _) = replay(&config, &bytes[..end])?;
            projection_matches(&path, &prefix_state)?;
            let backup = path.join(format!("events.partial-{}.jsonl", hash(&bytes)));
            if backup.symlink_metadata().is_ok() {
                if read_bounded(&backup, JOURNAL_LIMIT)? != bytes {
                    return Err(SessionError::new(
                        "corrupt_session",
                        "recovery backup does not match the original journal",
                    ));
                }
            } else {
                write_new(&backup, &bytes)?;
            }
            sync_directory(&path)?;
            atomic_replace(&path.join("events.jsonl"), &bytes[..end])?;
            bytes.truncate(end);
        }
        let (state, previous_hash, operations) = replay(&config, &bytes)?;
        let projection_current = projection_matches(&path, &state)?;
        let guard = SessionGuard {
            path,
            _lock: lock,
            config,
            state,
            previous_hash,
            operations,
            journal_bytes: bytes.len(),
            uncertain: false,
            projection_current,
        };
        guard.legacy_report()?;
        Ok(guard)
    }
}

/// Owns an OS advisory lock until dropped (also released by the OS on process exit).
/// Keep this guard for the entire runner operation, not just the first write.
pub struct SessionGuard {
    path: PathBuf,
    _lock: OwnedLock,
    config: ValidatedConfig,
    state: SessionState,
    previous_hash: String,
    operations: HashMap<String, Event>,
    journal_bytes: usize,
    uncertain: bool,
    projection_current: bool,
}

impl SessionGuard {
    pub(crate) fn owned_path(&self) -> &Path {
        &self.path
    }
    pub(crate) fn project_root(&self) -> &Path {
        self.path
            .ancestors()
            .nth(4)
            .expect("session under project root")
    }
    pub(crate) fn artifact_operation(&self, operation: &str, key: &str) -> Result<Option<String>> {
        identifier(operation)?;
        identifier(key)?;
        if self.uncertain {
            return Err(SessionError::new(
                "corrupt_session",
                "reopen after an uncertain journal write",
            ));
        }
        match self.operations.get(operation) {
            Some(Event::ArtifactPublished {
                key: previous,
                sha256,
            }) if previous == key => Ok(Some(sha256.clone())),
            Some(_) => Err(SessionError::new(
                "conflict",
                "operation ID already belongs to another operation",
            )),
            None => Ok(None),
        }
    }
    pub(crate) fn publish_artifact(
        &mut self,
        operation: &str,
        key: &str,
        sha256: &str,
        now: u64,
    ) -> Result<bool> {
        self.record(
            operation,
            now,
            Event::ArtifactPublished {
                key: key.into(),
                sha256: sha256.into(),
            },
        )
    }

    pub(crate) fn begin_execution(
        &mut self,
        operation: &str,
        candidate: Option<String>,
        parent: String,
        now: u64,
    ) -> Result<()> {
        if self.operations.contains_key(operation) {
            return Err(SessionError::new("recovery_required", "this execution ID was already started; inspect its result or resume and use a new ID"));
        }
        self.record(
            operation,
            now,
            Event::ExecutionStarted { candidate, parent },
        )?;
        Ok(())
    }
    pub(crate) fn reserve_stage(&mut self, operation: &str, now: u64) -> Result<u64> {
        let budget = &self.config.get().budget;
        let reserved_ms = (budget.command_timeout_seconds * 1000)
            .min((budget.active_seconds * 1000).saturating_sub(self.state.active_ms_used))
            .min(budget.deadline_unix_ms.saturating_sub(now));
        self.record(operation, now, Event::StageReserved { reserved_ms })?;
        Ok(reserved_ms)
    }
    pub(crate) fn settle_stage(
        &mut self,
        operation: &str,
        reservation: &str,
        elapsed_ms: u64,
        now: u64,
    ) -> Result<()> {
        self.record(
            operation,
            now.max(self.state.last_event_unix_ms),
            Event::StageSettled {
                reservation_id: reservation.into(),
                elapsed_ms,
            },
        )?;
        Ok(())
    }
    pub(crate) fn finish_execution(
        &mut self,
        operation: &str,
        report_key: &str,
        sha256: &str,
        decision: Decision,
        invalidate_reference: bool,
        now: u64,
    ) -> Result<()> {
        self.record(
            operation,
            now.max(self.state.last_event_unix_ms),
            Event::ExecutionFinished {
                report_key: report_key.into(),
                sha256: sha256.into(),
                decision,
                invalidate_reference,
            },
        )?;
        Ok(())
    }

    pub fn state(&self) -> &SessionState {
        &self.state
    }
    pub fn config(&self) -> &SessionConfig {
        self.config.get()
    }

    pub fn view(&self, now: u64) -> SessionView {
        let budget = &self.config.get().budget;
        SessionView {
            state: self.state.clone(),
            attempts_remaining: budget
                .max_experiments
                .saturating_sub(self.state.attempts_used),
            active_ms_remaining: (budget.active_seconds * 1000)
                .saturating_sub(self.state.active_ms_used),
            deadline_unix_ms: budget.deadline_unix_ms,
            deadline_expired: now >= budget.deadline_unix_ms,
            clock_regressed: now < self.state.last_event_unix_ms,
            recovery_required: self.state.in_flight.is_some() || self.state.execution.is_some(),
            projection_current: self.projection_current,
        }
    }

    /// Resume restores metadata only. It never starts an experiment or replenishes budgets.
    /// Unsettled work retains its full reservation when explicitly abandoned by resume.
    pub fn resume(&mut self, operation_id: &str, now: u64) -> Result<bool> {
        crate::supervisor::check_recovery(&self.path.join("process.json"))?;
        self.record(operation_id, now, Event::Resumed)
    }

    /// Stop is permitted after budget expiry and clamps a regressed wall clock.
    pub fn stop(&mut self, operation_id: &str, now: u64) -> Result<bool> {
        crate::supervisor::check_recovery(&self.path.join("process.json"))?;
        self.record(
            operation_id,
            now.max(self.state.last_event_unix_ms),
            Event::Stopped,
        )
    }

    /// Legacy single-command accounting. Reserve before starting any work.
    /// `true` means already recorded: never execute the same work again on a retry.
    /// One reservation consumes one attempt. The engine uses separate stage accounting.
    pub fn reserve_work(&mut self, operation_id: &str, now: u64) -> Result<bool> {
        // Preserve retry identity even if the available budget has changed since the call.
        let reserved_ms = match self.operations.get(operation_id) {
            Some(Event::WorkReserved { reserved_ms }) => *reserved_ms,
            _ => {
                let budget = &self.config.get().budget;
                (budget.command_timeout_seconds * 1000)
                    .min((budget.active_seconds * 1000).saturating_sub(self.state.active_ms_used))
                    .min(budget.deadline_unix_ms.saturating_sub(now))
            }
        };
        self.record(operation_id, now, Event::WorkReserved { reserved_ms })
    }

    /// Only the trusted runner may supply elapsed time from its monotonic clock.
    /// This operation is deliberately not exposed as a user-facing CLI command.
    pub fn settle_work(
        &mut self,
        operation_id: &str,
        reservation_id: &str,
        elapsed_ms: u64,
        now: u64,
    ) -> Result<bool> {
        self.record(
            operation_id,
            now,
            Event::WorkSettled {
                reservation_id: reservation_id.into(),
                elapsed_ms,
            },
        )
    }

    /// Returns true for an already committed operation. Such a retry never appends again.
    fn record(&mut self, operation_id: &str, now: u64, event: Event) -> Result<bool> {
        identifier(operation_id)?;
        if self.uncertain {
            return Err(SessionError::new(
                "corrupt_session",
                "a journal write failed; release the guard and inspect/reopen the session",
            ));
        }
        if let Some(previous) = self.operations.get(operation_id) {
            if previous != &event {
                return Err(SessionError::new(
                    "conflict",
                    "operation ID was already used for another operation",
                ));
            }
            self.confirm_committed()?;
            self.save_projection()?;
            return Ok(true);
        }
        let mut next = self.state.clone();
        let mut record = Record {
            format_version: FORMAT_VERSION,
            sequence: next.sequence + 1,
            operation_id: operation_id.into(),
            at_unix_ms: now,
            event,
            previous_sha256: self.previous_hash.clone(),
            sha256: String::new(),
        };
        apply(&mut next, self.config.get(), &record)?;
        record.sha256 = record.digest();
        let mut bytes = serde_json::to_vec(&record).expect("finite record");
        bytes.push(b'\n');
        if bytes.len() > RECORD_LIMIT || self.journal_bytes + bytes.len() > JOURNAL_LIMIT {
            return Err(SessionError::new(
                "storage_limit",
                "journal size limit reached",
            ));
        }
        let result = (|| -> std::io::Result<()> {
            let mut file = regular_open(&self.path.join("events.jsonl"), true)?;
            if file.metadata()?.len() != self.journal_bytes as u64 {
                return Err(std::io::Error::other(
                    "journal changed outside its owning writer",
                ));
            }
            file.write_all(&bytes)?;
            file.sync_all()
        })();
        if let Err(error) = result {
            self.uncertain = true;
            return Err(SessionError::new(
                "io",
                format!("journal append could not be confirmed: {error}"),
            ));
        }
        // The journal is authoritative even if the following projection write fails.
        self.state = next;
        self.previous_hash = record.sha256;
        self.operations.insert(operation_id.into(), record.event);
        self.journal_bytes += bytes.len();
        self.projection_current = false;
        self.save_projection()?;
        Ok(false)
    }

    fn confirm_committed(&self) -> Result<()> {
        // A previous process may have observed a complete append but failed its
        // fsync. Reconfirm durability before acknowledging an idempotent retry.
        regular_open(&self.path.join("events.jsonl"), false)?.sync_all()?;
        sync_directory(&self.path)
    }

    fn save_projection(&mut self) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(&self.state).expect("finite session state");
        atomic_replace(&self.path.join("state.json"), &bytes).map_err(|e| {
            SessionError::new(
                "projection_failed",
                format!("journal committed; state projection needs repair on retry: {e}"),
            )
        })?;
        self.projection_current = true;
        Ok(())
    }
}

fn apply(state: &mut SessionState, config: &SessionConfig, record: &Record) -> Result<()> {
    identifier(&record.operation_id)?;
    if record.format_version != FORMAT_VERSION || record.sequence != state.sequence + 1 {
        return Err(SessionError::new(
            "corrupt_session",
            "unsupported event version or broken event sequence",
        ));
    }
    if record.at_unix_ms < state.last_event_unix_ms || record.at_unix_ms > i64::MAX as u64 {
        return Err(SessionError::new(
            "clock_regressed",
            "event clock moved backwards or outside the supported range",
        ));
    }
    let budget = &config.budget;
    let remaining = (budget.active_seconds * 1000).saturating_sub(state.active_ms_used);
    let ensure_budget = || {
        if remaining == 0
            || state.attempts_used >= budget.max_experiments
            || record.at_unix_ms >= budget.deadline_unix_ms
        {
            Err(SessionError::new(
                "budget_exhausted",
                "session budget or original deadline is exhausted",
            ))
        } else {
            Ok(())
        }
    };
    if state.sequence == 0
        && !matches!(record.event, Event::Created { .. } | Event::Imported { .. })
    {
        return Err(SessionError::new(
            "corrupt_session",
            "first event must initialize the session",
        ));
    }
    match &record.event {
        Event::ExecutionStarted { candidate, parent } => {
            if state.artifacts.len() >= 256 {
                return Err(SessionError::new(
                    "storage_limit",
                    "session artifact limit reached",
                ));
            }
            if state.status == SessionStatus::Stopped
                || state.execution.is_some()
                || state.in_flight.is_some()
                || !state.artifacts.contains_key(parent)
                || state.accepted.as_deref().unwrap_or("workspace") != parent
            {
                return Err(SessionError::new(
                    "conflict",
                    "execution requires the current reference and an idle session",
                ));
            }
            if remaining == 0 || record.at_unix_ms >= budget.deadline_unix_ms {
                return Err(SessionError::new(
                    "budget_exhausted",
                    "execution budget exhausted",
                ));
            }
            if let Some(candidate) = candidate {
                if state.evaluated.contains_key(candidate) {
                    return Err(SessionError::new("conflict", "candidate already evaluated"));
                }
                if state.qualification.is_none() || !state.artifacts.contains_key(candidate) {
                    return Err(SessionError::new(
                        "conflict",
                        "qualify the reference and seal the candidate first",
                    ));
                }
                if state.attempts_used >= budget.max_experiments {
                    return Err(SessionError::new(
                        "budget_exhausted",
                        "attempt budget exhausted",
                    ));
                }
                state.attempts_used += 1;
            }
            state.execution = Some(Execution {
                operation_id: record.operation_id.clone(),
                candidate: candidate.clone(),
                parent: parent.clone(),
            });
            state.status = SessionStatus::Active;
        }
        Event::StageReserved { reserved_ms } => {
            if state.execution.is_none() || state.in_flight.is_some() {
                return Err(SessionError::new(
                    "conflict",
                    "stage requires an active execution",
                ));
            }
            let expected = (budget.command_timeout_seconds * 1000)
                .min(remaining)
                .min(budget.deadline_unix_ms.saturating_sub(record.at_unix_ms));
            if *reserved_ms == 0 || *reserved_ms != expected {
                return Err(SessionError::new(
                    "budget_exhausted",
                    "stage has no bounded time reservation",
                ));
            }
            state.active_ms_used += reserved_ms;
            state.in_flight = Some(Reservation {
                operation_id: record.operation_id.clone(),
                reserved_ms: *reserved_ms,
            });
        }
        Event::StageSettled {
            reservation_id,
            elapsed_ms,
        } => {
            let pending = state
                .in_flight
                .as_ref()
                .ok_or_else(|| SessionError::new("conflict", "stage is not reserved"))?;
            if state.execution.is_none() || &pending.operation_id != reservation_id {
                return Err(SessionError::new("conflict", "stage reservation differs"));
            }
            state.active_ms_used = state
                .active_ms_used
                .saturating_sub(pending.reserved_ms)
                .checked_add(*elapsed_ms)
                .ok_or_else(|| SessionError::new("storage_limit", "elapsed time overflow"))?;
            state.in_flight = None;
        }
        Event::ExecutionFinished {
            report_key,
            sha256,
            decision,
            invalidate_reference,
        } => {
            identifier(report_key)?;
            let execution = state
                .execution
                .as_ref()
                .ok_or_else(|| SessionError::new("conflict", "no active execution"))?;
            if state.in_flight.is_some()
                || state.artifacts.contains_key(report_key)
                || sha256.len() != 64
                || !sha256.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(SessionError::new(
                    "conflict",
                    "invalid execution completion",
                ));
            }
            match decision {
                Decision::Qualified if execution.candidate.is_none() => {
                    state.accepted = Some(execution.parent.clone());
                    state.qualification = Some(report_key.clone());
                }
                Decision::Kept if execution.candidate.is_some() => {
                    state.accepted = execution.candidate.clone();
                    state.qualification = Some(report_key.clone());
                }
                Decision::Qualified | Decision::Kept => {
                    return Err(SessionError::new(
                        "conflict",
                        "decision does not match execution kind",
                    ))
                }
                _ => {
                    if execution.candidate.is_none() || *invalidate_reference {
                        state.qualification = None;
                    }
                }
            }
            if let Some(candidate) = &execution.candidate {
                state
                    .evaluated
                    .insert(candidate.clone(), report_key.clone());
            }
            state.artifacts.insert(report_key.clone(), sha256.clone());
            state.execution = None;
        }
        Event::ArtifactPublished { key, sha256 } => {
            identifier(key)?;
            if sha256.len() != 64
                || !sha256.bytes().all(|b| b.is_ascii_hexdigit())
                || state.artifacts.contains_key(key)
                || state.artifacts.len() >= 256
            {
                return Err(SessionError::new(
                    "conflict",
                    "invalid, duplicate or excessive artifact publication",
                ));
            }
            state.artifacts.insert(key.clone(), sha256.clone());
        }
        Event::Created { config_sha256 } | Event::Imported { config_sha256, .. } => {
            if state.sequence != 0 || config_sha256 != &state.config_sha256 {
                return Err(SessionError::new(
                    "corrupt_session",
                    "configuration fingerprint differs from initialization",
                ));
            }
            state.session_id = config.session_id.clone();
            if let Event::Imported { import_sha256, .. } = &record.event {
                if import_sha256.len() != 64
                    || !import_sha256.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err(SessionError::new(
                        "corrupt_session",
                        "invalid historical report fingerprint",
                    ));
                }
                state.legacy_import = Some(import_sha256.clone());
            }
        }
        Event::Resumed => {
            ensure_budget()?;
            if state.status == SessionStatus::Active
                && state.in_flight.is_none()
                && state.execution.is_none()
            {
                return Err(SessionError::new("conflict", "session is already active"));
            }
            state.in_flight = None; // Any lost reservation stays charged.
            state.execution = None;
            state.status = SessionStatus::Active;
        }
        Event::Stopped => {
            if state.status == SessionStatus::Stopped {
                return Err(SessionError::new("conflict", "session is already stopped"));
            }
            state.in_flight = None;
            state.status = SessionStatus::Stopped;
            state.execution = None;
        }
        Event::WorkReserved { reserved_ms } => {
            if state.execution.is_some() {
                return Err(SessionError::new("conflict", "an evaluation is active"));
            }
            ensure_budget()?;
            if state.status != SessionStatus::Active || state.in_flight.is_some() {
                return Err(SessionError::new(
                    "conflict",
                    "work requires an active session with no unfinished reservation",
                ));
            }
            let expected = (budget.command_timeout_seconds * 1000)
                .min(remaining)
                .min(budget.deadline_unix_ms.saturating_sub(record.at_unix_ms));
            if *reserved_ms == 0 || *reserved_ms != expected {
                return Err(SessionError::new(
                    "conflict",
                    "reservation must cover the bounded command timeout",
                ));
            }
            state.active_ms_used += reserved_ms;
            state.attempts_used += 1;
            state.in_flight = Some(Reservation {
                operation_id: record.operation_id.clone(),
                reserved_ms: *reserved_ms,
            });
        }
        Event::WorkSettled {
            reservation_id,
            elapsed_ms,
        } => {
            let pending = state
                .in_flight
                .as_ref()
                .ok_or_else(|| SessionError::new("conflict", "no work is reserved"))?;
            if &pending.operation_id != reservation_id || *elapsed_ms > pending.reserved_ms {
                return Err(SessionError::new(
                    "conflict",
                    "settlement does not match the current reservation or exceeds it",
                ));
            }
            state.active_ms_used -= pending.reserved_ms - elapsed_ms;
            state.in_flight = None;
        }
    }
    state.sequence = record.sequence;
    state.last_event_unix_ms = record.at_unix_ms;
    Ok(())
}

type Replay = (SessionState, String, HashMap<String, Event>);
fn replay(config: &ValidatedConfig, bytes: &[u8]) -> Result<Replay> {
    if bytes.is_empty() || !bytes.ends_with(b"\n") {
        return Err(SessionError::new(
            "corrupt_session",
            "journal is empty or incomplete",
        ));
    }
    let digest = hash(&serde_json::to_vec(config.get()).expect("validated configuration"));
    let mut state = SessionState {
        format_version: FORMAT_VERSION,
        session_id: String::new(),
        config_sha256: digest,
        status: SessionStatus::Created,
        sequence: 0,
        last_event_unix_ms: 0,
        attempts_used: 0,
        active_ms_used: 0,
        in_flight: None,
        artifacts: BTreeMap::new(),
        accepted: None,
        qualification: None,
        execution: None,
        evaluated: BTreeMap::new(),
        legacy_import: None,
    };
    let mut previous = String::new();
    let mut operations = HashMap::new();
    for line in bytes[..bytes.len() - 1].split(|b| *b == b'\n') {
        if line.len() > RECORD_LIMIT {
            return Err(SessionError::new(
                "corrupt_session",
                "journal record exceeds its limit",
            ));
        }
        let record: Record = serde_json::from_slice(line).map_err(|_| {
            SessionError::new(
                "corrupt_session",
                "journal contains a malformed complete record",
            )
        })?;
        if record.previous_sha256 != previous
            || record.sha256 != record.digest()
            || operations.contains_key(&record.operation_id)
        {
            return Err(SessionError::new(
                "corrupt_session",
                "journal hash chain or operation identity is invalid",
            ));
        }
        apply(&mut state, config.get(), &record).map_err(|e| {
            SessionError::new(
                "corrupt_session",
                format!("invalid journal transition: {e}"),
            )
        })?;
        previous = record.sha256;
        operations.insert(record.operation_id, record.event);
    }
    Ok((state, previous, operations))
}

pub(crate) fn identifier(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(SessionError::new(
            "invalid_identifier",
            "use 1..128 ASCII letters, digits, hyphens or underscores",
        ));
    }
    Ok(())
}
pub(crate) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn check_directory(path: &Path) -> Result<()> {
    if !path.symlink_metadata()?.is_dir() {
        return Err(SessionError::new(
            "unsafe_path",
            "session paths must be directories, never symlinks",
        ));
    }
    Ok(())
}

pub(crate) fn regular_open(path: &Path, append: bool) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).append(append);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    regular_file(&file)?;
    Ok(file)
}
fn regular_file(file: &File) -> std::io::Result<()> {
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(std::io::Error::other("expected a regular file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(std::io::Error::other(
                "hard-linked session files are not supported",
            ));
        }
    }
    Ok(())
}
pub(crate) fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let file = regular_open(path, false)?;
    if file.metadata()?.len() > limit as u64 {
        return Err(SessionError::new(
            "storage_limit",
            "session file exceeds its size limit",
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(SessionError::new(
            "storage_limit",
            "session file exceeds its size limit",
        ));
    }
    Ok(bytes)
}
pub(crate) struct OwnedLock(File);
impl Drop for OwnedLock {
    fn drop(&mut self) {
        // Explicitly unlock before close: concurrent fork/spawn may temporarily
        // duplicate this open file description before CLOEXEC takes effect.
        let _ = FileExt::unlock(&self.0);
    }
}

pub(crate) fn acquire(path: &Path, create: bool) -> Result<OwnedLock> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create(create)
        .truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    regular_file(&file)?;
    FileExt::try_lock_exclusive(&file).map_err(|e| {
        if e.kind() == fs2::lock_contended_error().kind() {
            SessionError::new("session_busy", "another process owns the session lock")
        } else {
            e.into()
        }
    })?;
    Ok(OwnedLock(file))
}
pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
pub(crate) fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<()> {
    match path.symlink_metadata() {
        Ok(_) => {
            regular_open(path, false)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let parent = path.parent().expect("session file has parent");
    check_directory(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path)
        .map_err(|e| SessionError::from(e.error))?;
    sync_directory(parent)
}
pub(crate) fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all().map_err(Into::into)
}

// A stale/missing cache is recoverable, but a valid cache ahead of the journal
// proves a formerly committed suffix is missing. Never silently reset its budget.
fn projection_matches(path: &Path, state: &SessionState) -> Result<bool> {
    let cached = read_bounded(&path.join("state.json"), MAX_CONFIG_BYTES)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<SessionState>(&bytes).ok());
    if cached
        .as_ref()
        .is_some_and(|c| c.config_sha256 == state.config_sha256 && c.sequence > state.sequence)
    {
        return Err(SessionError::new(
            "corrupt_session",
            "state projection is ahead of the journal; restore the missing committed events",
        ));
    }
    Ok(cached.as_ref() == Some(state))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropping_guard_releases_lock_even_with_a_duplicated_descriptor() {
        let root = tempfile::tempdir().unwrap();
        let config =
            ValidatedConfig::from_json(include_bytes!("../../../examples/session.json")).unwrap();
        let store = SessionStore::new(root.path()).unwrap();
        let guard = store.init(config, "init", 1).unwrap();
        // Models the descriptor that a concurrently forked child may retain until exec.
        let duplicate = guard._lock.0.try_clone().unwrap();
        drop(guard);
        let reopened = store.open("example-session", false).unwrap();
        drop(duplicate);
        assert_eq!(reopened.state().sequence, 1);
    }
}
