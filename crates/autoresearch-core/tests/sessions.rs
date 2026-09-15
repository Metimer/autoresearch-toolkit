use autoresearch_core::{
    session::{SessionError, SessionStatus, SessionStore},
    SessionConfig, ValidatedConfig,
};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Child, Command},
    time::{Duration, Instant},
};

fn config() -> ValidatedConfig {
    let mut raw: SessionConfig =
        serde_json::from_str(include_str!("../../../examples/session.json")).unwrap();
    raw.session_id = "test-session".into();
    raw.budget.max_experiments = 3;
    raw.budget.active_seconds = 10;
    raw.budget.command_timeout_seconds = 4;
    raw.budget.deadline_unix_ms = 100_000;
    ValidatedConfig::try_from(raw).unwrap()
}
fn dir(root: &Path) -> std::path::PathBuf {
    root.join(".auto/engine/sessions/test-session")
}
fn code<T>(result: Result<T, SessionError>) -> &'static str {
    match result {
        Ok(_) => panic!("expected an error"),
        Err(error) => error.code,
    }
}

#[test]
fn lifecycle_replays_config_state_and_operation_identity() {
    let root = tempfile::tempdir().unwrap();
    let store = SessionStore::new(root.path()).unwrap();
    let mut guard = store.init(config(), "init", 1_000).unwrap();
    assert_eq!(guard.state().status, SessionStatus::Created);
    assert!(!guard.resume("resume", 1_001).unwrap());
    assert!(guard.resume("resume", 1_002).unwrap());
    assert_eq!(guard.state().sequence, 2);
    assert_eq!(code(guard.stop("resume", 1_003)), "conflict");
    assert_eq!(code(guard.resume("another-resume", 1_003)), "conflict");
    guard.stop("stop", 1_004).unwrap();
    let expected = guard.state().clone();
    drop(guard);
    // A retry still succeeds after the original deadline; it creates no new event.
    let mut reopened = store.init(config(), "init", 100_001).unwrap();
    assert_eq!(reopened.state(), &expected);
    assert!(reopened.stop("stop", 100_001).unwrap());
    assert!(reopened.view(100_001).projection_current);
    drop(reopened);
    assert_eq!(
        code(store.init(config(), "different-init", 1_005)),
        "conflict"
    );
    let mut changed = config().get().clone();
    changed.goal = "a different contract".into();
    assert_eq!(
        code(store.init(ValidatedConfig::try_from(changed).unwrap(), "init", 1_005)),
        "conflict"
    );
}

#[test]
fn reservations_survive_restart_and_settlement_is_idempotent() {
    let root = tempfile::tempdir().unwrap();
    let store = SessionStore::new(root.path()).unwrap();
    let mut guard = store.init(config(), "init", 1_000).unwrap();
    guard.resume("resume", 1_001).unwrap();
    guard.reserve_work("attempt-1", 1_002).unwrap();
    assert_eq!(guard.view(1_003).active_ms_remaining, 6_000);
    assert!(guard.reserve_work("attempt-1", 1_003).unwrap());
    assert_eq!(
        code(guard.settle_work("bad", "attempt-1", 4_001, 1_003)),
        "conflict"
    );
    guard
        .settle_work("settle-1", "attempt-1", 1_500, 2_502)
        .unwrap();
    assert!(guard
        .settle_work("settle-1", "attempt-1", 1_500, 2_503)
        .unwrap());
    assert_eq!(guard.view(2_503).active_ms_remaining, 8_500);
    guard.reserve_work("attempt-2", 2_504).unwrap();
    drop(guard);
    let mut guard = store.open("test-session", false).unwrap();
    assert!(guard.view(3_000).recovery_required);
    guard.resume("recover", 3_000).unwrap();
    assert_eq!(guard.view(3_000).active_ms_remaining, 4_500);
    assert_eq!(guard.view(3_000).attempts_remaining, 1);
    assert_eq!(
        code(guard.settle_work("late-settle", "attempt-2", 0, 3_001)),
        "conflict"
    );
    guard.stop("stop", 3_002).unwrap();
    guard.resume("resume-again", 3_003).unwrap();
    guard.reserve_work("attempt-3", 3_004).unwrap();
    guard
        .settle_work("settle-3", "attempt-3", 1, 3_005)
        .unwrap();
    assert_eq!(
        code(guard.reserve_work("attempt-4", 3_006)),
        "budget_exhausted"
    );
    guard.stop("final-stop", 3_007).unwrap();
    assert_eq!(code(guard.resume("no-reset", 3_008)), "budget_exhausted");
}

#[test]
fn time_budget_deadline_and_clock_regression_are_enforced() {
    let root = tempfile::tempdir().unwrap();
    let store = SessionStore::new(root.path()).unwrap();
    assert_eq!(
        code(store.init(config(), "expired", 100_000)),
        "budget_exhausted"
    );
    let mut guard = store.init(config(), "init", 99_000).unwrap();
    assert_eq!(code(guard.resume("backwards", 98_999)), "clock_regressed");
    guard.resume("resume", 99_001).unwrap();
    guard.reserve_work("last-second", 99_002).unwrap();
    assert_eq!(guard.state().in_flight.as_ref().unwrap().reserved_ms, 998);
    assert!(guard.view(100_000).deadline_expired);
    assert_eq!(
        code(guard.resume("expired-resume", 100_000)),
        "budget_exhausted"
    );
    guard.stop("stop", 1).unwrap();
    assert_eq!(guard.state().last_event_unix_ms, 99_002);
    assert_eq!(guard.state().active_ms_used, 998);

    let other = tempfile::tempdir().unwrap();
    let store = SessionStore::new(other.path()).unwrap();
    let mut raw = config().get().clone();
    raw.budget.max_experiments = 20;
    let mut guard = store
        .init(ValidatedConfig::try_from(raw).unwrap(), "init", 1)
        .unwrap();
    guard.resume("resume", 2).unwrap();
    for (reserve, recover, now) in [("a", "ra", 3), ("b", "rb", 5)] {
        guard.reserve_work(reserve, now).unwrap();
        guard.resume(recover, now + 1).unwrap();
    }
    guard.reserve_work("c", 7).unwrap();
    assert_eq!(guard.state().in_flight.as_ref().unwrap().reserved_ms, 2_000);
    assert_eq!(guard.view(8).active_ms_remaining, 0);
    assert_eq!(code(guard.resume("rc", 8)), "budget_exhausted");
    guard.stop("stop", 100_001).unwrap();
}

#[test]
fn missing_stale_and_failed_projections_do_not_lose_committed_events() {
    let root = tempfile::tempdir().unwrap();
    let store = SessionStore::new(root.path()).unwrap();
    let mut guard = store.init(config(), "init", 1).unwrap();
    let snapshot = dir(root.path()).join("state.json");
    let original = fs::read(&snapshot).unwrap();
    fs::remove_file(&snapshot).unwrap();
    fs::create_dir(&snapshot).unwrap();
    assert_eq!(code(guard.resume("resume", 2)), "projection_failed");
    assert_eq!(guard.state().sequence, 2);
    drop(guard);
    fs::remove_dir(&snapshot).unwrap();
    fs::write(&snapshot, original).unwrap();
    let mut guard = store.open("test-session", false).unwrap();
    assert_eq!(guard.state().status, SessionStatus::Active);
    assert!(!guard.view(3).projection_current);
    assert!(guard.resume("resume", 3).unwrap());
    assert!(guard.view(3).projection_current);
    assert_eq!(guard.state().sequence, 2);
    drop(guard);
    fs::remove_file(&snapshot).unwrap();
    let guard = store.open("test-session", false).unwrap();
    assert_eq!(guard.state().sequence, 2);
    assert!(!guard.view(3).projection_current);
    assert!(
        !snapshot.exists(),
        "status must not rewrite a missing projection"
    );
}

#[test]
fn explicit_tail_repair_preserves_original_and_rejects_complete_corruption() {
    let root = tempfile::tempdir().unwrap();
    let store = SessionStore::new(root.path()).unwrap();
    drop(store.init(config(), "init", 1).unwrap());
    let journal = dir(root.path()).join("events.jsonl");
    let clean = fs::read(&journal).unwrap();
    let mut broken = clean.clone();
    broken.extend_from_slice(b"{\"format_version\":");
    fs::write(&journal, &broken).unwrap();
    assert_eq!(code(store.open("test-session", false)), "corrupt_session");
    assert_eq!(fs::read(&journal).unwrap(), broken);
    let mut guard = store.open("test-session", true).unwrap();
    assert_eq!(fs::read(&journal).unwrap(), clean);
    guard.resume("resume", 2).unwrap();
    drop(guard);
    let backups: Vec<_> = fs::read_dir(dir(root.path()))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("events.partial-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(fs::read(&backups[0]).unwrap(), broken);
    fs::OpenOptions::new()
        .append(true)
        .open(&journal)
        .unwrap()
        .write_all(b"invalid\n")
        .unwrap();
    let corrupt = fs::read(&journal).unwrap();
    assert_eq!(code(store.open("test-session", true)), "corrupt_session");
    assert_eq!(fs::read(&journal).unwrap(), corrupt);
}

#[test]
fn lost_committed_suffix_and_changed_contract_or_event_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let store = SessionStore::new(root.path()).unwrap();
    let mut guard = store.init(config(), "init", 1).unwrap();
    let journal = dir(root.path()).join("events.jsonl");
    let first = fs::read(&journal).unwrap();
    guard.resume("resume", 2).unwrap();
    guard.reserve_work("reserve", 3).unwrap();
    let all = fs::read(&journal).unwrap();
    drop(guard);
    fs::write(&journal, &first).unwrap();
    assert_eq!(code(store.open("test-session", false)), "corrupt_session");
    let mut partial = first;
    partial.extend_from_slice(b"{");
    fs::write(&journal, &partial).unwrap();
    assert_eq!(code(store.open("test-session", true)), "corrupt_session");
    assert_eq!(fs::read(&journal).unwrap(), partial);
    fs::write(&journal, &all).unwrap();
    let altered = String::from_utf8(all.clone())
        .unwrap()
        .replace("work_reserved", "work_changed");
    fs::write(&journal, altered).unwrap();
    assert_eq!(code(store.open("test-session", false)), "corrupt_session");
    fs::write(&journal, &all).unwrap();
    let mut changed = config().get().clone();
    changed.budget.active_seconds = 20;
    fs::write(
        dir(root.path()).join("session.json"),
        serde_json::to_vec(&changed).unwrap(),
    )
    .unwrap();
    assert_eq!(code(store.open("test-session", false)), "corrupt_session");
}

#[cfg(unix)]
#[test]
fn owned_paths_reject_links_and_session_traversal() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let store = SessionStore::new(root.path()).unwrap();
    symlink(outside.path(), root.path().join(".auto")).unwrap();
    assert_eq!(code(store.init(config(), "init", 1)), "unsafe_path");
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    fs::remove_file(root.path().join(".auto")).unwrap();
    drop(store.init(config(), "init", 1).unwrap());
    assert_eq!(
        code(store.open("../test-session", false)),
        "invalid_identifier"
    );
    let journal = dir(root.path()).join("events.jsonl");
    let original = fs::read(&journal).unwrap();
    let external = outside.path().join("events");
    fs::write(&external, &original).unwrap();
    fs::remove_file(&journal).unwrap();
    symlink(&external, &journal).unwrap();
    assert_eq!(code(store.open("test-session", false)), "io");
    fs::remove_file(&journal).unwrap();
    fs::hard_link(&external, &journal).unwrap();
    assert_eq!(code(store.open("test-session", false)), "io");
    assert_eq!(fs::read(external).unwrap(), original);
}

struct KillOnDrop(Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn process_death_releases_the_lock_and_preserves_the_reservation() {
    let root = tempfile::tempdir().unwrap();
    let store = SessionStore::new(root.path()).unwrap();
    let mut guard = store.init(config(), "init", 1).unwrap();
    guard.resume("resume", 2).unwrap();
    assert_eq!(code(store.open("test-session", false)), "session_busy");
    drop(guard);
    let mut child = KillOnDrop(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "lock_holder_process", "--ignored", "--nocapture"])
            .env("AUTORESEARCH_LOCK_TEST_ROOT", root.path())
            .spawn()
            .unwrap(),
    );
    let started = Instant::now();
    while !root.path().join("ready").exists() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "child did not acquire lock"
        );
        assert!(child.0.try_wait().unwrap().is_none(), "child exited early");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(code(store.open("test-session", false)), "session_busy");
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    let mut guard = store.open("test-session", false).unwrap();
    assert_eq!(guard.view(4).active_ms_remaining, 6_000);
    assert_eq!(guard.view(4).attempts_remaining, 2);
    assert!(guard.view(4).recovery_required);
    guard.resume("recover", 4).unwrap();
    assert_eq!(guard.view(4).active_ms_remaining, 6_000);
    assert!(!guard.view(4).recovery_required);
}

#[test]
#[ignore = "subprocess fixture exercised by process_death_releases_the_lock_and_preserves_the_reservation"]
fn lock_holder_process() {
    let Some(root) = std::env::var_os("AUTORESEARCH_LOCK_TEST_ROOT") else {
        return;
    };
    let store = SessionStore::new(Path::new(&root)).unwrap();
    let mut guard = store.open("test-session", false).unwrap();
    guard.reserve_work("interrupted-work", 3).unwrap();
    fs::write(Path::new(&root).join("ready"), b"ready").unwrap();
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[test]
fn uncertain_append_forces_reopen_and_repair_before_more_operations() {
    let root = tempfile::tempdir().unwrap();
    let store = SessionStore::new(root.path()).unwrap();
    let mut guard = store.init(config(), "init", 1).unwrap();
    let journal = dir(root.path()).join("events.jsonl");
    // Simulate an unconfirmed partial append visible on disk to the owning writer.
    fs::OpenOptions::new()
        .append(true)
        .open(&journal)
        .unwrap()
        .write_all(b"{")
        .unwrap();
    assert_eq!(code(guard.resume("resume", 2)), "io");
    assert_eq!(guard.state().sequence, 1);
    assert_eq!(code(guard.stop("stop", 3)), "corrupt_session");
    drop(guard);
    let mut guard = store.open("test-session", true).unwrap();
    guard.resume("resume", 4).unwrap();
    assert_eq!(guard.state().sequence, 2);
}

#[test]
fn hash_mismatch_and_oversized_journals_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let store = SessionStore::new(root.path()).unwrap();
    drop(store.init(config(), "init", 1).unwrap());
    let journal = dir(root.path()).join("events.jsonl");
    let mut event: serde_json::Value =
        serde_json::from_slice(&fs::read(&journal).unwrap()).unwrap();
    event["operation_id"] = serde_json::json!("changed");
    let mut bytes = serde_json::to_vec(&event).unwrap();
    bytes.push(b'\n');
    fs::write(&journal, bytes).unwrap();
    assert_eq!(code(store.open("test-session", true)), "corrupt_session");
    fs::OpenOptions::new()
        .write(true)
        .open(&journal)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    assert_eq!(code(store.open("test-session", false)), "storage_limit");
}
