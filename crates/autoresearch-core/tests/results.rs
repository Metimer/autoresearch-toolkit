use autoresearch_core::{
    config::{CommandSpec, NetworkPolicy},
    results::Selection,
    session::{Decision, SessionGuard, SessionStore},
    supervisor::now_ms,
    workspace::LocalChanges,
    SessionConfig, ValidatedConfig,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const PRIVATE: &str = "PRIVATE_CANARY_97d81";
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    source: PathBuf,
    config: SessionConfig,
}
fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Metimer")
        .env("GIT_AUTHOR_EMAIL", "metinamerwane@gmail.com")
        .env("GIT_COMMITTER_NAME", "Metimer")
        .env("GIT_COMMITTER_EMAIL", "metinamerwane@gmail.com")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout).unwrap().trim().into()
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("pilot é");
        let source = temp.path().join(PRIVATE);
        fs::create_dir(&root).unwrap();
        fs::create_dir(&source).unwrap();
        fs::create_dir(source.join("src")).unwrap();
        fs::write(source.join("src/value"), "10").unwrap();
        fs::write(source.join("private.txt"), PRIVATE).unwrap();
        fs::write(source.join("bench.sh"), "printf '%s\\n' \"$PRIVATE_TOKEN\"; printf 'METRIC {\"name\":\"bench_ms\",\"value\":%s,\"unit\":\"ms\"}\\n' \"$(cat src/value)\"\n").unwrap();
        git(&source, &["init", "--template="]);
        git(&source, &["add", "."]);
        git(&source, &["commit", "-m", "results fixture"]);
        let mut config: SessionConfig =
            serde_json::from_str(include_str!("../../../examples/session.json")).unwrap();
        config.source.repository = source.to_string_lossy().into();
        config.source.commit = git(&source, &["rev-parse", "HEAD"]);
        config.goal = PRIVATE.into();
        config.scope.protected_paths = vec!["bench.sh".into(), "private.txt".into()];
        config.execution.network = NetworkPolicy::Allowed;
        config
            .execution
            .environment
            .set
            .insert("PRIVATE_TOKEN".into(), PRIVATE.into());
        config.checks = vec![CommandSpec {
            executable: "/bin/sh".into(),
            args: vec!["-c".into(), "exit 0".into()],
            cwd: ".".into(),
        }];
        config.benchmark = CommandSpec {
            executable: "/bin/sh".into(),
            args: vec!["bench.sh".into()],
            cwd: ".".into(),
        };
        config.sampling.warmup = 0;
        config.budget.deadline_unix_ms = 4_000_000_000_000;
        Self {
            _temp: temp,
            root,
            source,
            config,
        }
    }
    fn session(&self, id: &str) -> SessionGuard {
        let mut config = self.config.clone();
        config.session_id = id.into();
        let mut session = SessionStore::new(&self.root)
            .unwrap()
            .init(
                ValidatedConfig::try_from(config).unwrap(),
                "init",
                now_ms().unwrap(),
            )
            .unwrap();
        session
            .create_workspace("workspace", LocalChanges::Exclude, now_ms().unwrap())
            .unwrap();
        assert_eq!(
            session.baseline("baseline").unwrap().report.decision,
            Decision::Qualified
        );
        session
    }
    fn trial(&self, session: &mut SessionGuard, id: &str, value: &str) -> String {
        let prepared = session
            .prepare_candidate(&format!("prepare-{id}"), id, PRIVATE, now_ms().unwrap())
            .unwrap();
        fs::write(prepared.path.join("src/value"), value).unwrap();
        session
            .seal_candidate(&format!("seal-{id}"), id, now_ms().unwrap())
            .unwrap();
        session
            .evaluate_candidate(&format!("evaluate-{id}"), id)
            .unwrap();
        session
            .state()
            .evaluated
            .get(&format!("sealed-{id}"))
            .unwrap()
            .clone()
    }
}

#[test]
fn index_rebuild_search_and_exact_duplicates_bind_parent_patch_and_protocol() {
    let f = Fixture::new();
    let mut first = f.session("first");
    let key = f.trial(&mut first, "fast", "5");
    drop(first);
    let mut second = f.session("second");
    f.trial(&mut second, "fast", "5");
    drop(second);
    let store = SessionStore::new(&f.root).unwrap();
    let index = store.rebuild_memory("index-1").unwrap();
    assert_eq!(index.rows.len(), 4);
    let rows = index.search("src/value");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].duplicate_key, rows[1].duplicate_key);
    assert_eq!(rows[1].duplicate_of, vec![format!("first/{key}")]);
    assert_eq!(index.search(PRIVATE).len(), 2);
    fs::write(
        f.root.join(".auto/engine/memory.json"),
        "corrupt disposable cache",
    )
    .unwrap();
    assert_eq!(store.memory().unwrap().rows.len(), 4);
    store.rebuild_memory("index-2").unwrap();
    let mut third = f.session("third");
    f.trial(&mut third, "fast", "5");
    let changed = f.trial(&mut third, "faster", "2");
    drop(third);
    let index = store.memory().unwrap();
    let row = index.rows.iter().find(|r| r.evaluation == changed).unwrap();
    assert!(row.duplicate_of.is_empty());
    assert_ne!(row.parent_tree_sha256, rows[0].parent_tree_sha256);
    let mut other = Fixture::new();
    other.config.metric.minimum_improvement = 0.5;
    let mut session = other.session("protocol");
    other.trial(&mut session, "fast", "5");
    drop(session);
    let other_index = SessionStore::new(&other.root).unwrap().memory().unwrap();
    let other_row = other_index.search("src/value")[0];
    assert_eq!(other_row.patch_sha256, rows[0].patch_sha256);
    assert_ne!(other_row.duplicate_key, rows[0].duplicate_key);
}

#[test]
fn default_preview_and_export_exclude_private_context_and_retry_without_mutation() {
    let f = Fixture::new();
    let mut session = f.session("private-user");
    let key = f.trial(&mut session, "fast", "5");
    let before = session.state().sequence;
    let prepared = session.prepare_result(&key, Selection::default()).unwrap();
    assert_eq!(session.state().sequence, before);
    let preview = prepared.preview();
    let json = serde_json::to_string(preview).unwrap();
    assert!(!json.contains(PRIVATE));
    assert!(!json.contains("private-user"));
    assert!(!json.contains("PRIVATE_TOKEN"));
    assert!(!preview.manifest.files.contains_key("candidate.patch"));
    assert!(preview.report["samples"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["phase"] == "confirmation"));
    let output = f.root.join("result é");
    let result = session
        .export_result(
            "export",
            &key,
            Selection::default(),
            &output,
            now_ms().unwrap(),
        )
        .unwrap();
    assert_eq!(result.sha256, preview.sha256);
    assert!(!result.already_applied);
    for entry in fs::read_dir(&output).unwrap() {
        let text = fs::read_to_string(entry.unwrap().path()).unwrap();
        assert!(!text.contains(PRIVATE));
        assert!(!text.contains("private-user"));
    }
    let sequence = session.state().sequence;
    assert!(
        session
            .export_result(
                "export",
                &key,
                Selection::default(),
                &output,
                now_ms().unwrap()
            )
            .unwrap()
            .already_applied
    );
    assert_eq!(session.state().sequence, sequence);
    assert!(session
        .export_result(
            "export",
            &key,
            Selection {
                code: true,
                ..Selection::default()
            },
            &output,
            now_ms().unwrap()
        )
        .is_err());
    fs::write(output.join("report.md"), "tampered").unwrap();
    assert!(session
        .export_result(
            "export",
            &key,
            Selection::default(),
            &output,
            now_ms().unwrap()
        )
        .is_err());
    assert_eq!(
        fs::read_to_string(f.source.join("src/value")).unwrap(),
        "10"
    );
}

#[test]
fn selected_code_reproduces_cumulative_candidate_and_preserves_comparison_reference() {
    let f = Fixture::new();
    let mut session = f.session("code");
    f.trial(&mut session, "fast", "5");
    let key = f.trial(&mut session, "faster", "2");
    let selection = Selection {
        code: true,
        protocol: true,
        logs: Vec::new(),
    };
    let output = f.root.join("bundle");
    session
        .export_result("export", &key, selection, &output, now_ms().unwrap())
        .unwrap();
    assert_eq!(
        fs::read_to_string(output.join("reference/src/value")).unwrap(),
        "5"
    );
    let replay = f.root.join("replay");
    fs::create_dir(&replay).unwrap();
    fs::create_dir(replay.join("src")).unwrap();
    for path in ["src/value", "bench.sh", "private.txt"] {
        fs::copy(output.join("base").join(path), replay.join(path)).unwrap();
    }
    git(&replay, &["init", "--template="]);
    git(&replay, &["add", "."]);
    git(
        &replay,
        &[
            "apply",
            "--index",
            "--binary",
            output.join("candidate.patch").to_str().unwrap(),
        ],
    );
    assert_eq!(fs::read_to_string(replay.join("src/value")).unwrap(), "2");
    let protocol: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("protocol.template.json")).unwrap()).unwrap();
    assert_eq!(protocol["source"]["repository"], ".");
    assert_eq!(
        protocol["execution"]["environment"]["set"]["PRIVATE_TOKEN"],
        PRIVATE
    );
    // Requalify the bundled comparison reference in a fresh repository, then
    // measure the reconstructed candidate. No original source checkout is used.
    fs::copy(output.join("reference/src/value"), replay.join("src/value")).unwrap();
    git(&replay, &["add", "."]);
    git(&replay, &["commit", "-m", "reproduce bundled reference"]);
    let mut config: SessionConfig = serde_json::from_value(protocol).unwrap();
    config.source.repository = replay.to_string_lossy().into();
    config.source.commit = git(&replay, &["rev-parse", "HEAD"]);
    let mut reproduction = SessionStore::new(&f.root)
        .unwrap()
        .init(
            ValidatedConfig::try_from(config).unwrap(),
            "init",
            now_ms().unwrap(),
        )
        .unwrap();
    reproduction
        .create_workspace("workspace", LocalChanges::Exclude, now_ms().unwrap())
        .unwrap();
    assert_eq!(
        reproduction.baseline("baseline").unwrap().report.decision,
        Decision::Qualified
    );
    let reproduced_key = f.trial(&mut reproduction, "reproduced", "2");
    let reproduced = reproduction.evaluation(&reproduced_key).unwrap().report;
    assert_eq!(reproduced.decision, Decision::Kept);
    assert_eq!(
        serde_json::to_value(&reproduced.samples).unwrap(),
        serde_json::to_value(&session.evaluation(&key).unwrap().report.samples).unwrap()
    );
}

#[test]
fn selected_logs_are_hash_checked_and_other_files_cannot_be_selected() {
    let f = Fixture::new();
    let mut session = f.session("logs");
    let key = f.trial(&mut session, "fast", "5");
    let report = session.evaluation(&key).unwrap().report;
    let log = report
        .stages
        .iter()
        .find(|s| s.name.contains("exploration") || s.name == "benchmark")
        .unwrap_or(&report.stages[0])
        .stdout
        .clone();
    let selection = Selection {
        logs: vec![log.clone()],
        ..Selection::default()
    };
    let prepared = session.prepare_result(&key, selection.clone()).unwrap();
    assert!(prepared
        .preview()
        .manifest
        .files
        .contains_key(&log.replace("stages/", "logs/")));
    assert!(session
        .prepare_result(
            &key,
            Selection {
                logs: vec!["../../session.json".into()],
                ..Selection::default()
            }
        )
        .is_err());
    fs::write(
        f.root
            .join(".auto/engine/sessions/logs/executions")
            .join(&key)
            .join(log),
        PRIVATE,
    )
    .unwrap();
    assert_eq!(
        session.prepare_result(&key, selection).err().unwrap().code,
        "corrupt_artifact"
    );
    assert!(session.prepare_result(&key, Selection::default()).is_ok());
}

#[test]
fn busy_sessions_and_corrupt_evidence_block_complete_memory_queries() {
    let f = Fixture::new();
    let mut session = f.session("busy");
    let key = f.trial(&mut session, "fast", "5");
    let store = SessionStore::new(&f.root).unwrap();
    assert_eq!(store.memory().unwrap_err().code, "session_busy");
    drop(session);
    fs::write(
        f.root
            .join(".auto/engine/sessions/busy/executions")
            .join(key)
            .join("report.json"),
        "{}",
    )
    .unwrap();
    assert_eq!(store.memory().unwrap_err().code, "corrupt_artifact");
}

#[test]
fn published_bundle_can_reconcile_an_interrupted_receipt_without_overwriting() {
    let f = Fixture::new();
    let mut session = f.session("recovery");
    let key = f.trial(&mut session, "fast", "5");
    let directory = f.root.join(".auto/engine/sessions/recovery");
    let journal = fs::read(directory.join("events.jsonl")).unwrap();
    let projection = fs::read(directory.join("state.json")).unwrap();
    let output = f.root.join("bundle");
    session
        .export_result(
            "export",
            &key,
            Selection::default(),
            &output,
            now_ms().unwrap(),
        )
        .unwrap();
    drop(session);
    // Crash after directory/receipt publication, before the journal append.
    fs::write(directory.join("events.jsonl"), journal).unwrap();
    fs::write(directory.join("state.json"), projection).unwrap();
    let mut session = SessionStore::new(&f.root)
        .unwrap()
        .open("recovery", false)
        .unwrap();
    let original = fs::read(output.join("manifest.json")).unwrap();
    assert!(
        !session
            .export_result(
                "export",
                &key,
                Selection::default(),
                &output,
                now_ms().unwrap()
            )
            .unwrap()
            .already_applied
    );
    assert_eq!(fs::read(output.join("manifest.json")).unwrap(), original);
    let empty = f.root.join("empty");
    fs::create_dir(&empty).unwrap();
    assert!(session
        .export_result(
            "empty",
            &key,
            Selection::default(),
            &empty,
            now_ms().unwrap()
        )
        .is_err());
    assert_eq!(fs::read_dir(empty).unwrap().count(), 0);
    assert!(session
        .export_result(
            "source",
            &key,
            Selection::default(),
            &f.source.join("result"),
            now_ms().unwrap()
        )
        .is_err());
}

#[cfg(unix)]
#[test]
fn index_and_result_files_reject_symlinks_and_hardlinks() {
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let mut session = f.session("links");
    let key = f.trial(&mut session, "fast", "5");
    let invalid_output = f.root.join(std::ffi::OsString::from_vec(vec![b'r', 0xff]));
    assert_eq!(
        session
            .export_result(
                "invalid-path",
                &key,
                Selection::default(),
                &invalid_output,
                now_ms().unwrap()
            )
            .unwrap_err()
            .code,
        "unsafe_path"
    );
    assert!(!invalid_output.exists());
    let output = f.root.join("bundle");
    session
        .export_result(
            "export",
            &key,
            Selection::default(),
            &output,
            now_ms().unwrap(),
        )
        .unwrap();
    let external = f.root.join("external");
    fs::write(&external, "preserve me").unwrap();
    fs::remove_file(output.join("report.md")).unwrap();
    symlink(&external, output.join("report.md")).unwrap();
    assert!(session
        .export_result(
            "export",
            &key,
            Selection::default(),
            &output,
            now_ms().unwrap()
        )
        .is_err());
    assert_eq!(fs::read_to_string(&external).unwrap(), "preserve me");
    drop(session);
    let cache = f.root.join(".auto/engine/memory.json");
    symlink(&external, &cache).unwrap();
    let store = SessionStore::new(&f.root).unwrap();
    assert!(store.rebuild_memory("index").is_err());
    fs::remove_file(&cache).unwrap();
    fs::hard_link(&external, &cache).unwrap();
    assert!(store.rebuild_memory("index").is_err());
    assert_eq!(fs::read_to_string(external).unwrap(), "preserve me");
}
