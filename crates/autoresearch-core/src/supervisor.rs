//! Bounded POSIX execution for trusted local workloads. This is not a sandbox.
use crate::{
    config::CommandSpec,
    session::{self, SessionError},
};
use rustix::process::{
    kill_process_group, test_kill_process_group, waitid, Pid, Signal, WaitId, WaitidOptions,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    os::{
        fd::OwnedFd,
        unix::{net::UnixStream, process::CommandExt},
    },
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
type Result<T> = std::result::Result<T, SessionError>;
const OUTPUT_CEILING: u64 = 64 * 1024 * 1024;

pub fn now_ms() -> Result<u64> {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| SessionError::new("clock_regressed", "clock precedes Unix epoch"))?
        .as_millis();
    u64::try_from(ms)
        .map_err(|_| SessionError::new("clock_regressed", "clock outside supported range"))
}

pub struct Cancellation {
    flag: Arc<AtomicBool>,
    handlers: Vec<signal_hook::SigId>,
}
impl Cancellation {
    pub fn install() -> Result<Self> {
        let mut state = Self {
            flag: Arc::new(AtomicBool::new(false)),
            handlers: Vec::new(),
        };
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            state
                .handlers
                .push(signal_hook::flag::register(signal, state.flag.clone())?);
        }
        Ok(state)
    }
    pub fn cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}
impl Drop for Cancellation {
    fn drop(&mut self) {
        for handler in self.handlers.drain(..) {
            signal_hook::low_level::unregister(handler);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessOutcome {
    Passed,
    Failed,
    TimedOut,
    Cancelled,
    OutputLimit,
    StorageLimit,
    SpawnFailed,
    CleanupIncomplete,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessReport {
    pub outcome: ProcessOutcome,
    pub exit_code: Option<i32>,
    pub elapsed_ms: u64,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
}
pub struct ProcessOutput {
    pub report: ProcessReport,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    format_version: u32,
    token: String,
    supervisor_pid: u32,
    group_pid: Option<u32>,
}

/// Do not signal recorded PIDs after a restart. A surviving/reused group blocks recovery.
pub(crate) fn check_recovery(marker: &Path) -> Result<()> {
    match marker.symlink_metadata() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
        Ok(_) => (),
    }
    let record: Marker = serde_json::from_slice(&session::read_bounded(marker, 4096)?)
        .map_err(|_| SessionError::new("recovery_required", "invalid process ownership marker"))?;
    let pid=record.group_pid.and_then(|pid| i32::try_from(pid).ok()).and_then(Pid::from_raw).ok_or_else(||SessionError::new("recovery_required","launch was interrupted before process identity was recorded; inspect the owned processes"))?;
    if record.format_version != 1 {
        return Err(SessionError::new(
            "recovery_required",
            "unknown process ownership format",
        ));
    }
    match test_kill_process_group(pid) {
        Err(rustix::io::Errno::SRCH)=>{fs::remove_file(marker)?;session::sync_directory(marker.parent().unwrap())},
        _=>Err(SessionError::new("recovery_required","the recorded process group still exists or cannot be inspected; no PID will be killed during recovery")),
    }
}

struct OwnedChild {
    child: Child,
    pid: Pid,
    reaped: bool,
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = kill_process_group(self.pid, Signal::Kill);
            let start = Instant::now();
            while start.elapsed() < Duration::from_millis(250) {
                if exited(self.pid).unwrap_or(false) {
                    let _ = self.child.wait();
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}
fn exited(pid: Pid) -> Result<bool> {
    match waitid(
        WaitId::Pid(pid),
        WaitidOptions::EXITED | WaitidOptions::NOWAIT | WaitidOptions::NOHANG,
    ) {
        Ok(status) => Ok(status.is_some()),
        Err(rustix::io::Errno::INTR) => Ok(false),
        Err(e) => Err(std::io::Error::from(e).into()),
    }
}
fn drain(
    stream: &mut UnixStream,
    target: &mut Vec<u8>,
    remaining: &mut usize,
) -> Result<(bool, bool)> {
    let mut buffer = [0_u8; 8192];
    for _ in 0..8 {
        match stream.read(&mut buffer) {
            Ok(0) => return Ok((false, true)),
            Ok(size) => {
                let accepted = size.min(*remaining);
                target.extend_from_slice(&buffer[..accepted]);
                *remaining -= accepted;
                if accepted < size {
                    return Ok((true, false));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok((false, false)),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Ok((false, false))
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub timeout_ms: u64,
    pub max_output_bytes: u64,
}

/// `monitor` returns cancellation and checks storage/integrity. It is called while the child runs.
/// A marker survives any uncertain launch/cleanup, preventing automatic recovery.
pub fn run(
    spec: &CommandSpec,
    cwd: &Path,
    environment: &BTreeMap<String, String>,
    limits: Limits,
    marker: &Path,
    token: &str,
    mut monitor: impl FnMut() -> Result<bool>,
) -> Result<ProcessOutput> {
    let Limits {
        timeout_ms,
        max_output_bytes,
    } = limits;
    if timeout_ms == 0 {
        return Err(SessionError::new(
            "budget_exhausted",
            "no time reserved for command",
        ));
    }
    check_recovery(marker)?;
    let launch = Marker {
        format_version: 1,
        token: token.into(),
        supervisor_pid: std::process::id(),
        group_pid: None,
    };
    session::write_new(marker, &serde_json::to_vec(&launch).unwrap())?;
    session::sync_directory(marker.parent().unwrap())?;
    let (mut stdout, out_child) = UnixStream::pair()?;
    let (mut stderr, err_child) = UnixStream::pair()?;
    stdout.set_nonblocking(true)?;
    stderr.set_nonblocking(true)?;
    let start = Instant::now();
    let child = Command::new(&spec.executable)
        .args(&spec.args)
        .current_dir(cwd)
        .env_clear()
        .envs(environment)
        .stdin(Stdio::null())
        .stdout(Stdio::from(OwnedFd::from(out_child)))
        .stderr(Stdio::from(OwnedFd::from(err_child)))
        .process_group(0)
        .spawn();
    let child = match child {
        Ok(child) => child,
        Err(_) => {
            fs::remove_file(marker)?;
            session::sync_directory(marker.parent().unwrap())?;
            return Ok(output(
                ProcessOutcome::SpawnFailed,
                None,
                start,
                Vec::new(),
                Vec::new(),
            ));
        }
    };
    let pid = Pid::from_raw(child.id() as i32)
        .ok_or_else(|| SessionError::new("recovery_required", "invalid owned process ID"))?;
    let mut child = OwnedChild {
        child,
        pid,
        reaped: false,
    };
    let running = Marker {
        group_pid: Some(pid.as_raw_nonzero().get() as u32),
        ..launch
    };
    session::atomic_replace(marker, &serde_json::to_vec(&running).unwrap())?;
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut remaining = max_output_bytes.min(OUTPUT_CEILING) as usize;
    let mut outcome = None;
    let mut terminating = None;
    let mut last_monitor = Instant::now() - Duration::from_secs(1);
    loop {
        let (out_overflow, out_eof) = drain(&mut stdout, &mut out, &mut remaining)?;
        let (err_overflow, err_eof) = drain(&mut stderr, &mut err, &mut remaining)?;
        let overflow = out_overflow || err_overflow;
        if overflow && outcome.is_none() {
            outcome = Some(ProcessOutcome::OutputLimit);
        }
        if last_monitor.elapsed() >= Duration::from_millis(20) && outcome.is_none() {
            match monitor() {
                Ok(true) => outcome = Some(ProcessOutcome::Cancelled),
                Ok(false) => (),
                Err(_) => outcome = Some(ProcessOutcome::StorageLimit),
            }
            last_monitor = Instant::now();
        }
        if start.elapsed() >= Duration::from_millis(timeout_ms) && outcome.is_none() {
            outcome = Some(ProcessOutcome::TimedOut);
        }
        let done = exited(pid)?;
        if done || outcome.is_some() {
            if terminating.is_none() {
                let _ = kill_process_group(pid, Signal::Term);
                terminating = Some(Instant::now());
            }
            if done || terminating.unwrap().elapsed() >= Duration::from_millis(100) {
                let _ = kill_process_group(pid, Signal::Kill);
            }
            if done && ((out_eof && err_eof) || outcome.is_some()) {
                // The unreaped leader pins the group identity through the final signal.
                let status = child.child.wait()?;
                child.reaped = true;
                fs::remove_file(marker)?;
                session::sync_directory(marker.parent().unwrap())?;
                let verdict = outcome.unwrap_or(if status.success() {
                    ProcessOutcome::Passed
                } else {
                    ProcessOutcome::Failed
                });
                return Ok(output(verdict, status.code(), start, out, err));
            }
            if terminating.unwrap().elapsed() >= Duration::from_secs(2) {
                return Ok(output(
                    ProcessOutcome::CleanupIncomplete,
                    None,
                    start,
                    out,
                    err,
                ));
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn output(
    outcome: ProcessOutcome,
    exit_code: Option<i32>,
    start: Instant,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
) -> ProcessOutput {
    ProcessOutput {
        report: ProcessReport {
            outcome,
            exit_code,
            elapsed_ms: start.elapsed().as_millis().min(u64::MAX as u128) as u64,
            stdout_bytes: stdout.len(),
            stderr_bytes: stderr.len(),
            stdout_sha256: session::hash(&stdout),
            stderr_sha256: session::hash(&stderr),
        },
        stdout,
        stderr,
    }
}
