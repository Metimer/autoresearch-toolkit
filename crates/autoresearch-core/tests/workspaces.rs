use autoresearch_core::{
    session::{SessionError, SessionGuard, SessionStore},
    workspace::LocalChanges,
    SessionConfig, ValidatedConfig,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let out = Command::new("git")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Metimer")
        .env("GIT_AUTHOR_EMAIL", "metinamerwane@gmail.com")
        .env("GIT_COMMITTER_NAME", "Metimer")
        .env("GIT_COMMITTER_EMAIL", "metinamerwane@gmail.com")
        .args([
            "-c",
            "core.autocrlf=false",
            "-c",
            "core.filemode=true",
            "-C",
        ])
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}
fn write(root: &Path, path: &str, bytes: &[u8]) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
fn fingerprint(root: &Path) -> BTreeMap<String, (Vec<u8>, u32)> {
    fn visit(root: &Path, path: &Path, out: &mut BTreeMap<String, (Vec<u8>, u32)>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            let meta = path.symlink_metadata().unwrap();
            if meta.is_dir() {
                visit(root, &path, out)
            } else {
                #[cfg(unix)]
                let mode = {
                    use std::os::unix::fs::PermissionsExt;
                    meta.permissions().mode()
                };
                #[cfg(not(unix))]
                let mode = 0;
                let bytes = if meta.is_symlink() {
                    fs::read_link(&path)
                        .unwrap()
                        .to_string_lossy()
                        .as_bytes()
                        .to_vec()
                } else {
                    fs::read(&path).unwrap()
                };
                out.insert(
                    path.strip_prefix(root).unwrap().to_string_lossy().into(),
                    (bytes, mode),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    source: PathBuf,
    config: ValidatedConfig,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("pilot");
        let source = temp.path().join("source with spaces");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&source).unwrap();
        git(&source, &["init", "--template="]);
        write(&source, "src/main.txt", b"old\n");
        write(&source, "src/remove.txt", b"remove\n");
        write(&source, "src/rename.txt", b"rename\n");
        write(&source, "src/binary.bin", b"\0\x01\x02\xff");
        write(&source, "tests/check.txt", b"protected\n");
        write(&source, "README.txt", b"outside scope\n");
        write(&source, ".gitignore", b"ignored.txt\nbuild/\n");
        git(&source, &["add", "--all"]);
        git(&source, &["commit", "-m", "fixture"]);
        let commit = String::from_utf8(git(&source, &["rev-parse", "HEAD"]))
            .unwrap()
            .trim()
            .to_owned();
        let mut config: SessionConfig =
            serde_json::from_str(include_str!("../../../examples/session.json")).unwrap();
        config.session_id = "isolation".into();
        config.source.repository = source.to_string_lossy().into();
        config.source.commit = commit;
        config.budget.deadline_unix_ms = 4_000_000_000_000;
        config.scope.protected_paths = vec!["tests/".into()];
        config.scope.protected_sha256.insert(
            "tests/check.txt".into(),
            format!("{:x}", Sha256::digest(b"protected\n")),
        );
        Self {
            _temp: temp,
            root,
            source,
            config: ValidatedConfig::try_from(config).unwrap(),
        }
    }
    fn session(&self) -> SessionGuard {
        SessionStore::new(&self.root)
            .unwrap()
            .init(self.config.clone(), "init", 1)
            .unwrap()
    }
    fn reopen(&self) -> SessionGuard {
        SessionStore::new(&self.root)
            .unwrap()
            .open("isolation", false)
            .unwrap()
    }
    fn output(&self) -> PathBuf {
        self._temp.path().join("export")
    }
    fn artifacts(&self) -> PathBuf {
        self.root.join(".auto/engine/sessions/isolation/artifacts")
    }
}
fn code<T>(result: Result<T, SessionError>) -> &'static str {
    match result {
        Ok(_) => panic!("expected error"),
        Err(e) => e.code,
    }
}

#[test]
fn isolated_workspace_leaves_dirty_source_and_all_git_metadata_unchanged() {
    let f = Fixture::new();
    write(&f.source, "src/main.txt", b"staged\n");
    git(&f.source, &["add", "src/main.txt"]);
    write(&f.source, "src/main.txt", b"working\n");
    write(&f.source, "src/untracked.txt", b"new\n");
    write(&f.source, "ignored.txt", b"private\n");
    let before = fingerprint(&f.source);
    let mut session = f.session();
    let base = session
        .create_workspace("workspace", LocalChanges::Exclude, 2)
        .unwrap();
    assert_eq!(fs::read(base.path.join("src/main.txt")).unwrap(), b"old\n");
    assert!(!base.path.join("src/untracked.txt").exists());
    assert!(!f
        .artifacts()
        .join("workspace/repository.git/objects/info/alternates")
        .exists());
    let tree = git(
        &f.artifacts().join("workspace/repository.git"),
        &["rev-parse", "refs/autoresearch/base"],
    );
    assert_eq!(tree.len(), 41);
    assert_eq!(fingerprint(&f.source), before);
    assert!(
        session
            .create_workspace("workspace", LocalChanges::Exclude, 3)
            .unwrap()
            .already_applied
    );
    assert_eq!(
        code(session.create_workspace("workspace", LocalChanges::Include, 3)),
        "conflict"
    );
    let candidate = session
        .prepare_candidate("prepare", "one", "test isolation", 4)
        .unwrap();
    write(&candidate.path, "src/main.txt", b"changed\n");
    session.seal_candidate("seal", "one", 5).unwrap();
    session
        .export_candidate("export", "one", &f.output(), 6)
        .unwrap();
    assert_eq!(
        fingerprint(&f.source),
        before,
        "HEAD, index, refs, objects and user files must be byte-identical"
    );
    assert_eq!(session.state().attempts_used, 0);
}

#[test]
fn include_policy_captures_worktree_deletions_and_nonignored_untracked_files() {
    let f = Fixture::new();
    write(&f.source, "src/main.txt", b"staged\n");
    git(&f.source, &["add", "src/main.txt"]);
    write(&f.source, "src/main.txt", b"working\n");
    fs::remove_file(f.source.join("src/remove.txt")).unwrap();
    write(&f.source, "src/new.txt", b"new\n");
    write(&f.source, "ignored.txt", b"ignored\n");
    write(&f.source, "build/generated.txt", b"generated\n");
    let before = fingerprint(&f.source);
    let mut session = f.session();
    let base = session
        .create_workspace("workspace", LocalChanges::Include, 2)
        .unwrap();
    assert_eq!(
        fs::read(base.path.join("src/main.txt")).unwrap(),
        b"working\n"
    );
    assert!(!base.path.join("src/remove.txt").exists());
    assert!(base.path.join("src/new.txt").exists());
    assert!(!base.path.join("ignored.txt").exists());
    assert!(!base.path.join("build").exists());
    assert_eq!(fingerprint(&f.source), before);
}

#[test]
fn protected_hashes_and_source_identity_are_checked_before_publication() {
    let mut f = Fixture::new();
    write(&f.source, "tests/check.txt", b"changed\n");
    let mut session = f.session();
    assert_eq!(
        code(session.create_workspace("workspace", LocalChanges::Include, 2)),
        "scope_violation"
    );
    assert!(!f.artifacts().join("workspace").exists());
    drop(session);
    let raw = f.config.get().clone();
    let mut bad = raw.clone();
    bad.session_id = "bad".into();
    bad.source.commit = "f".repeat(40);
    f.config = ValidatedConfig::try_from(bad).unwrap();
    let mut session = f.session();
    assert_eq!(
        code(session.create_workspace("workspace", LocalChanges::Exclude, 2)),
        "git_failed"
    );
    drop(session);
    git(&f.source, &["add", "--all"]);
    git(&f.source, &["commit", "-m", "later"]);
    let mut config = raw;
    config.session_id = "head-mismatch".into();
    f.config = ValidatedConfig::try_from(config).unwrap();
    let mut session = f.session();
    assert_eq!(
        code(session.create_workspace("workspace", LocalChanges::Include, 2)),
        "invalid_source"
    );
}

#[test]
fn sealing_rejects_outside_scope_protection_and_links_even_under_generated_paths() {
    let f = Fixture::new();
    let mut session = f.session();
    session
        .create_workspace("workspace", LocalChanges::Exclude, 2)
        .unwrap();
    let candidate = session
        .prepare_candidate("prepare", "one", "policy", 3)
        .unwrap();
    for path in ["tests/check.txt", "README.txt", "outside.txt"] {
        let old = fs::read(candidate.path.join(path)).ok();
        write(&candidate.path, path, b"forbidden\n");
        assert_eq!(
            code(session.seal_candidate("seal", "one", 4)),
            "scope_violation"
        );
        if let Some(old) = old {
            write(&candidate.path, path, &old)
        } else {
            fs::remove_file(candidate.path.join(path)).unwrap();
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        fs::create_dir(candidate.path.join("build")).unwrap();
        symlink(&f.source, candidate.path.join("build/link")).unwrap();
        assert!(session.seal_candidate("seal", "one", 4).is_err());
        fs::remove_file(candidate.path.join("build/link")).unwrap();
        fs::hard_link(
            f.source.join("src/main.txt"),
            candidate.path.join("src/link.txt"),
        )
        .unwrap();
        assert!(session.seal_candidate("seal", "one", 4).is_err());
        fs::remove_file(candidate.path.join("src/link.txt")).unwrap();
    }
    write(&candidate.path, "build/generated.txt", b"not included\n");
    write(&candidate.path, "src/.git/config", b"reserved\n");
    assert_eq!(
        code(session.seal_candidate("seal", "one", 4)),
        "unsafe_path"
    );
    fs::remove_dir_all(candidate.path.join("src/.git")).unwrap();
    let sealed = session.seal_candidate("seal", "one", 5).unwrap();
    assert!(!sealed.path.join("build").exists());
}

#[test]
fn binary_add_delete_rename_and_executable_changes_roundtrip_after_source_is_gone() {
    let f = Fixture::new();
    let mut session = f.session();
    session
        .create_workspace("workspace", LocalChanges::Exclude, 2)
        .unwrap();
    let candidate = session
        .prepare_candidate("prepare", "one", "roundtrip", 3)
        .unwrap();
    write(&candidate.path, "src/main.txt", b"changed\r\n");
    write(&candidate.path, "src/new file.txt", b"new\n");
    write(&candidate.path, "src/binary.bin", b"\0\x03\xff\x04");
    fs::remove_file(candidate.path.join("src/remove.txt")).unwrap();
    fs::rename(
        candidate.path.join("src/rename.txt"),
        candidate.path.join("src/renamed.txt"),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            candidate.path.join("src/main.txt"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    let sealed = session.seal_candidate("seal", "one", 4).unwrap();
    let expected = fingerprint(&sealed.path);
    write(
        &candidate.path,
        "src/main.txt",
        b"later edits must not leak\n",
    );
    assert!(
        session
            .seal_candidate("seal", "one", 5)
            .unwrap()
            .already_applied
    );
    fs::rename(&f.source, f._temp.path().join("source-offline")).unwrap();
    drop(session);
    let mut session = f.reopen();
    session.stop("stop", 6).unwrap();
    let output = f.output();
    let exported = session
        .export_candidate("export", "one", &output, 7)
        .unwrap();
    assert!(!exported.evaluated);
    let applied = f._temp.path().join("apply");
    copy_dir(&output.join("base"), &applied);
    git(&applied, &["init", "--template="]);
    git(&applied, &["add", "--all"]);
    git(
        &applied,
        &[
            "apply",
            "--index",
            "--binary",
            output.join("candidate.patch").to_str().unwrap(),
        ],
    );
    fs::remove_dir_all(applied.join(".git")).unwrap();
    assert_eq!(fingerprint(&applied), expected);
    assert!(
        session
            .export_candidate("export", "one", &output, 8)
            .unwrap()
            .already_applied
    );
    write(&output, "candidate.patch", b"tampered\n");
    assert_eq!(
        code(session.export_candidate("export", "one", &output, 9)),
        "corrupt_artifact"
    );
}
fn copy_dir(source: &Path, destination: &Path) {
    fs::create_dir(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let path = entry.unwrap().path();
        let dest = destination.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_dir(&path, &dest)
        } else {
            fs::copy(path, dest).unwrap();
        }
    }
}

#[test]
fn frozen_snapshot_changes_cannot_be_exported_and_existing_outputs_are_preserved() {
    let f = Fixture::new();
    let mut session = f.session();
    session
        .create_workspace("workspace", LocalChanges::Exclude, 2)
        .unwrap();
    session
        .prepare_candidate("prepare", "one", "policy", 3)
        .unwrap();
    let sealed = session.seal_candidate("seal", "one", 4).unwrap();
    fs::create_dir(f.output()).unwrap();
    write(&f.output(), "keep.txt", b"existing\n");
    let original = fingerprint(&f.output());
    assert!(session
        .export_candidate("export", "one", &f.output(), 5)
        .is_err());
    assert_eq!(fingerprint(&f.output()), original);
    assert_eq!(
        code(session.export_candidate("source-export", "one", &f.source.join("export"), 5)),
        "unsafe_path"
    );
    write(&sealed.path, "src/main.txt", b"tampered\n");
    assert_eq!(
        code(session.export_candidate("other", "one", &f._temp.path().join("other"), 5)),
        "corrupt_artifact"
    );
}

#[test]
fn operation_retries_and_projection_failure_preserve_published_artifacts() {
    let f = Fixture::new();
    let mut session = f.session();
    let state = f.root.join(".auto/engine/sessions/isolation/state.json");
    fs::remove_file(&state).unwrap();
    fs::create_dir(&state).unwrap();
    assert_eq!(
        code(session.create_workspace("workspace", LocalChanges::Exclude, 2)),
        "projection_failed"
    );
    assert!(f.artifacts().join("workspace/manifest.json").exists());
    drop(session);
    fs::remove_dir(&state).unwrap();
    let mut session = f.reopen();
    let retried = session
        .create_workspace("workspace", LocalChanges::Exclude, 3)
        .unwrap();
    assert!(retried.already_applied);
    assert_eq!(session.state().sequence, 2);
    assert_eq!(
        code(session.prepare_candidate("workspace", "one", "same operation", 4)),
        "conflict"
    );
    session
        .prepare_candidate("prepare", "one", "candidate", 4)
        .unwrap();
    assert_eq!(
        code(session.prepare_candidate("two", "two", "parallel", 5)),
        "conflict"
    );
    assert_eq!(
        code(session.prepare_candidate("prepare", "one", "different hypothesis", 5)),
        "conflict"
    );
    assert!(
        session
            .prepare_candidate("prepare", "one", "candidate", 5)
            .unwrap()
            .already_applied
    );
    session.seal_candidate("seal", "one", 6).unwrap();
    session.prepare_candidate("two", "two", "next", 7).unwrap();
    let seq = session.state().sequence;
    drop(session);
    let session = f.reopen();
    assert_eq!(session.state().sequence, seq);
    assert_eq!(session.state().artifacts.len(), 4);
}

#[test]
fn unsupported_source_links_modes_and_snapshot_limits_fail_closed() {
    let mut f = Fixture::new();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("main.txt", f.source.join("src/link")).unwrap();
        git(&f.source, &["add", "src/link"]);
        git(&f.source, &["commit", "-m", "link"]);
        let mut config = f.config.get().clone();
        config.source.commit = String::from_utf8(git(&f.source, &["rev-parse", "HEAD"]))
            .unwrap()
            .trim()
            .into();
        f.config = ValidatedConfig::try_from(config).unwrap();
        let mut session = f.session();
        assert_eq!(
            code(session.create_workspace("workspace", LocalChanges::Exclude, 2)),
            "unsafe_path"
        );
    }
    let mut f = Fixture::new();
    let mut config = f.config.get().clone();
    config.budget.max_output_bytes = 1;
    config.budget.max_artifact_bytes = 1;
    f.config = ValidatedConfig::try_from(config).unwrap();
    let mut session = f.session();
    assert_eq!(
        code(session.create_workspace("workspace", LocalChanges::Exclude, 2)),
        "storage_limit"
    );
}

#[test]
fn filters_hooks_and_fsmonitor_do_not_run_during_source_capture() {
    let mut f = Fixture::new();
    write(
        &f.source,
        ".gitattributes",
        b"*.txt filter=trap diff=trap\n",
    );
    git(&f.source, &["add", ".gitattributes"]);
    git(&f.source, &["commit", "-m", "attributes"]);
    let trap = f._temp.path().join("trap.sh");
    fs::write(
        &trap,
        format!(
            "#!/bin/sh\ntouch '{}'\nexit 99\n",
            f._temp.path().join("executed").display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&trap, fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(
        &f.source,
        &["config", "filter.trap.clean", trap.to_str().unwrap()],
    );
    git(
        &f.source,
        &["config", "filter.trap.smudge", trap.to_str().unwrap()],
    );
    git(
        &f.source,
        &["config", "core.fsmonitor", trap.to_str().unwrap()],
    );
    git(
        &f.source,
        &["config", "diff.trap.command", trap.to_str().unwrap()],
    );
    let mut config = f.config.get().clone();
    config.source.commit = String::from_utf8(git(&f.source, &["rev-parse", "HEAD"]))
        .unwrap()
        .trim()
        .into();
    f.config = ValidatedConfig::try_from(config).unwrap();
    let mut session = f.session();
    session
        .create_workspace("workspace", LocalChanges::Include, 2)
        .unwrap();
    assert!(!f._temp.path().join("executed").exists());
}

#[test]
fn orphan_snapshot_after_uncertain_journal_append_can_be_reconciled_once() {
    use std::io::Write;
    let f = Fixture::new();
    let mut session = f.session();
    let journal = f.root.join(".auto/engine/sessions/isolation/events.jsonl");
    fs::OpenOptions::new()
        .append(true)
        .open(&journal)
        .unwrap()
        .write_all(b"{")
        .unwrap();
    assert_eq!(
        code(session.create_workspace("workspace", LocalChanges::Exclude, 2)),
        "io"
    );
    assert!(f.artifacts().join("workspace/manifest.json").exists());
    assert!(!session.state().artifacts.contains_key("workspace"));
    drop(session);
    let mut session = SessionStore::new(&f.root)
        .unwrap()
        .open("isolation", true)
        .unwrap();
    assert!(
        session
            .create_workspace("workspace", LocalChanges::Exclude, 3)
            .unwrap()
            .already_applied
    );
    assert_eq!(session.state().sequence, 2);
    assert_eq!(session.state().artifacts.len(), 1);
    assert!(
        session
            .create_workspace("workspace", LocalChanges::Exclude, 4)
            .unwrap()
            .already_applied
    );
    assert_eq!(session.state().sequence, 2);
}

#[test]
fn an_empty_existing_export_directory_is_never_overwritten() {
    let f = Fixture::new();
    let mut session = f.session();
    session
        .create_workspace("workspace", LocalChanges::Exclude, 2)
        .unwrap();
    session
        .prepare_candidate("prepare", "one", "unchanged", 3)
        .unwrap();
    session.seal_candidate("seal", "one", 4).unwrap();
    fs::create_dir(f.output()).unwrap();
    assert!(session
        .export_candidate("export", "one", &f.output(), 5)
        .is_err());
    assert_eq!(fs::read_dir(f.output()).unwrap().count(), 0);
}

#[test]
fn a_source_used_as_pilot_does_not_capture_its_own_session_storage() {
    let mut f = Fixture::new();
    f.root = f.source.clone();
    let mut config = f.config.get().clone();
    config.source.repository = ".".into();
    f.config = ValidatedConfig::try_from(config).unwrap();
    let index = fs::read(f.source.join(".git/index")).unwrap();
    let mut session = f.session();
    let base = session
        .create_workspace("workspace", LocalChanges::Include, 2)
        .unwrap();
    assert!(!base.path.join(".auto").exists());
    assert_eq!(fs::read(f.source.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(base.path.join("src/main.txt")).unwrap(), b"old\n");
}

#[test]
fn export_projection_failure_retries_without_overwriting_the_bundle() {
    let f = Fixture::new();
    let mut session = f.session();
    session
        .create_workspace("workspace", LocalChanges::Exclude, 2)
        .unwrap();
    let candidate = session
        .prepare_candidate("prepare", "one", "retry export", 3)
        .unwrap();
    write(&candidate.path, "src/main.txt", b"changed\n");
    session.seal_candidate("seal", "one", 4).unwrap();
    let state = f.root.join(".auto/engine/sessions/isolation/state.json");
    fs::remove_file(&state).unwrap();
    fs::create_dir(&state).unwrap();
    assert_eq!(
        code(session.export_candidate("export", "one", &f.output(), 5)),
        "projection_failed"
    );
    let output = fingerprint(&f.output());
    drop(session);
    fs::remove_dir(&state).unwrap();
    let mut session = f.reopen();
    assert!(
        session
            .export_candidate("export", "one", &f.output(), 6)
            .unwrap()
            .already_applied
    );
    assert_eq!(fingerprint(&f.output()), output);
    assert_eq!(session.state().sequence, 5);
}
