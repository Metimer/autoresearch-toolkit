//! Independent Git snapshots and immutable candidate artifacts. No project command is run.
use crate::{
    session::{self, SessionError, SessionGuard, SessionStatus},
    SessionConfig,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, SessionError>;
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_FILES: usize = 4096;
const MANIFEST_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalChanges {
    Exclude,
    Include,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileEntry {
    pub sha256: String,
    pub bytes: u64,
    pub mode: u32,
}
pub(crate) type Inventory = BTreeMap<String, FileEntry>;
pub(crate) type Contents = BTreeMap<String, (FileEntry, Vec<u8>)>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Workspace {
        local_changes: LocalChanges,
    },
    Candidate {
        candidate: String,
        hypothesis: String,
    },
    Seal {
        candidate: String,
    },
    Export {
        candidate: String,
        output: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceIdentity {
    root: String,
    commit: String,
    local_changes: LocalChanges,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    format_version: u32,
    operation_id: String,
    request: Request,
    config_sha256: String,
    pub(crate) base_sha256: Option<String>,
    source: Option<SourceIdentity>,
    pub(crate) files: Inventory,
    export_proof: Option<ExportIntegrity>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceResult {
    pub artifact: String,
    pub sha256: String,
    pub path: PathBuf,
    pub files: usize,
    pub already_applied: bool,
    pub evaluated: bool,
}

impl SessionGuard {
    /// Freeze a baseline from the declared commit or an explicitly included working tree.
    pub fn create_workspace(
        &mut self,
        operation: &str,
        policy: LocalChanges,
        now: u64,
    ) -> Result<WorkspaceResult> {
        let request = Request::Workspace {
            local_changes: policy,
        };
        if let Some(result) = self.retry_artifact("workspace", operation, &request, now)? {
            return Ok(result);
        }
        self.workspace_gate(now)?;
        let source = self
            .project_root()
            .join(&self.config().source.repository)
            .canonicalize()?;
        let git = Git::source(&source)?;
        let root = git.text(&["rev-parse", "--show-toplevel"], None)?;
        if Path::new(&root).canonicalize()? != source {
            return fail(
                "invalid_source",
                "source.repository must name the worktree root",
            );
        }
        let commit = self.config().source.commit.to_ascii_lowercase();
        if git.text(&["cat-file", "-t", &commit], None)? != "commit" {
            return fail("invalid_source", "source ID must identify a commit");
        }
        let committed = source_commit(&git, &commit, self.config())?;
        let files = if policy == LocalChanges::Include {
            if git.text(&["rev-parse", "HEAD"], None)? != commit {
                return fail(
                    "invalid_source",
                    "including local files requires HEAD to equal the declared commit",
                );
            }
            let first = source_worktree(&git, &source, &committed, self.config())?;
            let second = source_worktree(&git, &source, &committed, self.config())?;
            if first != second || git.text(&["rev-parse", "HEAD"], None)? != commit {
                return fail(
                    "source_changed",
                    "source files changed while the snapshot was being captured",
                );
            }
            first
        } else {
            committed
        };
        check_protected(&files, self.config())?;
        let manifest = Manifest {
            format_version: 1,
            operation_id: operation.into(),
            request,
            config_sha256: self.state().config_sha256.clone(),
            base_sha256: None,
            source: Some(SourceIdentity {
                root,
                commit,
                local_changes: policy,
            }),
            files: inventory(&files),
            export_proof: None,
        };
        let parent = self.artifacts_dir()?;
        let staging = tempfile::Builder::new()
            .prefix(".building-")
            .tempdir_in(&parent)?;
        write_tree(&staging.path().join("tree"), &files)?;
        let repository = staging.path().join("repository.git");
        let isolated = Git::init(&repository)?;
        let tree = isolated.store_tree(&files)?;
        isolated.run(&["update-ref", "refs/autoresearch/base", &tree], None)?;
        sync_tree(&repository)?;
        self.finish_artifact("workspace", staging, manifest, now)
    }

    /// Create an editable directory from the frozen baseline. No attempt is executed.
    pub fn prepare_candidate(
        &mut self,
        operation: &str,
        candidate: &str,
        hypothesis: &str,
        now: u64,
    ) -> Result<WorkspaceResult> {
        candidate_id(candidate)?;
        if hypothesis.trim().is_empty() || hypothesis.len() > 4096 || hypothesis.contains('\0') {
            return fail(
                "invalid_identifier",
                "hypothesis must contain 1..4096 bytes without NUL",
            );
        }
        let key = format!("candidate-{candidate}");
        let request = Request::Candidate {
            candidate: candidate.into(),
            hypothesis: hypothesis.into(),
        };
        if let Some(result) = self.retry_artifact(&key, operation, &request, now)? {
            return Ok(result);
        }
        self.workspace_gate(now)?;
        if self.state().artifacts.keys().any(|key| {
            key.strip_prefix("candidate-")
                .is_some_and(|id| !self.state().artifacts.contains_key(&format!("sealed-{id}")))
        }) {
            return fail(
                "conflict",
                "seal the current candidate before preparing another",
            );
        }
        let reference = self.state().accepted.as_deref().unwrap_or("workspace");
        let (base, digest, base_path) = self.load_artifact(reference)?;
        let files = verify_snapshot(&base_path, &base.files, self.config())?;
        let manifest = Manifest {
            format_version: 1,
            operation_id: operation.into(),
            request,
            config_sha256: self.state().config_sha256.clone(),
            base_sha256: Some(digest),
            source: None,
            files: inventory(&files),
            export_proof: None,
        };
        let parent = self.artifacts_dir()?;
        let staging = tempfile::Builder::new()
            .prefix(".building-")
            .tempdir_in(parent)?;
        write_tree(&staging.path().join("tree"), &files)?;
        self.finish_artifact(&key, staging, manifest, now)
    }

    /// Seal a copied snapshot, checking scope and protected content against the baseline.
    pub fn seal_candidate(
        &mut self,
        operation: &str,
        candidate: &str,
        now: u64,
    ) -> Result<WorkspaceResult> {
        candidate_id(candidate)?;
        let key = format!("sealed-{candidate}");
        let request = Request::Seal {
            candidate: candidate.into(),
        };
        if let Some(result) = self.retry_artifact(&key, operation, &request, now)? {
            return Ok(result);
        }
        self.workspace_gate(now)?;
        let reference = self.state().accepted.as_deref().unwrap_or("workspace");
        let (base, base_hash, base_path) = self.load_artifact(reference)?;
        verify_snapshot(&base_path, &base.files, self.config())?;
        let (prepared, _, candidate_path) =
            self.load_artifact(&format!("candidate-{candidate}"))?;
        if prepared.base_sha256.as_ref() != Some(&base_hash) {
            return fail("corrupt_artifact", "candidate belongs to another baseline");
        }
        let files = scan(&candidate_path.join("tree"), self.config(), true)?;
        check_scope(&base.files, &inventory(&files), self.config())?;
        check_protected(&files, self.config())?;
        let parent = self.artifacts_dir()?;
        let staging = tempfile::Builder::new()
            .prefix(".building-")
            .tempdir_in(parent)?;
        write_tree(&staging.path().join("tree"), &files)?;
        if scan(&candidate_path.join("tree"), self.config(), true)? != files {
            return fail(
                "source_changed",
                "candidate changed while sealing; stop editing and retry",
            );
        }
        let manifest = Manifest {
            format_version: 1,
            operation_id: operation.into(),
            request,
            config_sha256: self.state().config_sha256.clone(),
            base_sha256: Some(base_hash),
            source: None,
            files: inventory(&files),
            export_proof: None,
        };
        self.finish_artifact(&key, staging, manifest, now)
    }

    /// Export a sealed, explicitly unevaluated candidate to a new directory.
    /// The bundle includes a baseline tree so local changes are reproducible too.
    pub fn export_candidate(
        &mut self,
        operation: &str,
        candidate: &str,
        output: &Path,
        now: u64,
    ) -> Result<WorkspaceResult> {
        candidate_id(candidate)?;
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .canonicalize()?;
        let name = output
            .file_name()
            .ok_or_else(|| SessionError::new("unsafe_path", "output must name a new directory"))?;
        let output = parent.join(name);
        if output.components().any(|component| {
            component.as_os_str().to_str().is_some_and(|part| {
                part.eq_ignore_ascii_case(".git") || part.eq_ignore_ascii_case(".auto")
            })
        }) {
            return fail(
                "unsafe_path",
                "export cannot be placed in Git or engine metadata",
            );
        }
        if output.starts_with(self.owned_path()) {
            return fail("unsafe_path", "export must be outside the session storage");
        }
        let (base, _base_hash, base_path) = self.load_artifact("workspace")?;
        let source = Path::new(
            &base
                .source
                .as_ref()
                .ok_or_else(|| {
                    SessionError::new("corrupt_artifact", "baseline has no source identity")
                })?
                .root,
        );
        if output.starts_with(source) {
            return fail(
                "unsafe_path",
                "export must be outside the source repository",
            );
        }
        let request = Request::Export {
            candidate: candidate.into(),
            output: utf8_path(&output)?,
        };
        let key = format!("export-{}", session::hash(operation.as_bytes()));
        session::identifier(&key)?;
        if let Some(result) = self.retry_artifact(&key, operation, &request, now)? {
            verify_export(&output, &result.sha256, self.config())?;
            return Ok(WorkspaceResult {
                path: output,
                ..result
            });
        }
        // Exports are allowed after stop/expiry, but still share the session writer lock.
        if now < self.state().last_event_unix_ms {
            return fail("clock_regressed", "clock moved backwards");
        }
        let base_files = verify_snapshot(&base_path, &base.files, self.config())?;
        let (sealed, sealed_hash, sealed_path) =
            self.load_artifact(&format!("sealed-{candidate}"))?;
        let files = verify_snapshot(&sealed_path, &sealed.files, self.config())?;
        check_scope(&base.files, &sealed.files, self.config())?;
        let git_dir = tempfile::tempdir()?;
        let git = Git::init(&git_dir.path().join("repository.git"))?;
        let before = git.store_tree(&base_files)?;
        let after = git.store_tree(&files)?;
        let patch = git.run(
            &[
                "diff",
                "--binary",
                "--full-index",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                &before,
                &after,
                "--",
            ],
            None,
        )?;
        git.run(&["read-tree", &before], None)?;
        if !patch.is_empty() {
            git.run(
                &["apply", "--cached", "--binary", "--whitespace=nowarn"],
                Some(patch.clone()),
            )?;
        }
        if git.text(&["write-tree"], None)? != after {
            return fail(
                "corrupt_artifact",
                "export patch does not reproduce the sealed Git tree",
            );
        }
        let manifest = Manifest {
            format_version: 1,
            operation_id: operation.into(),
            request,
            config_sha256: self.state().config_sha256.clone(),
            base_sha256: Some(sealed_hash),
            source: base.source.clone(),
            files: sealed.files.clone(),
            export_proof: Some(ExportIntegrity {
                patch_sha256: session::hash(&patch),
                base_files: inventory(&base_files),
            }),
        };
        let digest = manifest_hash(&manifest)?;
        if output.symlink_metadata().is_ok() {
            // Retry a published output whose journal receipt was not yet committed.
            verify_export(&output, &digest, self.config())?;
        } else {
            let staging = tempfile::Builder::new()
                .prefix(".exporting-")
                .tempdir_in(&parent)?;
            write_tree(&staging.path().join("base"), &base_files)?;
            session::write_new(&staging.path().join("candidate.patch"), &patch)?;
            session::write_new(
                &staging.path().join("manifest.json"),
                &manifest_bytes(&manifest)?,
            )?;
            session::write_new(&staging.path().join("README.txt"), b"UNEVALUATED CANDIDATE: no tests or benchmarks have run.\nCopy base/ to a new directory, initialize Git there, stage its files, then run:\ngit -c core.autocrlf=false apply --index --binary /absolute/path/to/candidate.patch\nAn empty patch represents an unchanged candidate; skip apply in that case.\nFile content and executable bits are preserved; empty directories and other metadata are not.\n")?;
            if directory_bytes(staging.path())? > self.config().budget.max_artifact_bytes {
                return fail("storage_limit", "export exceeds artifact budget");
            }
            sync_tree(staging.path())?;
            // No overwrite, even for an empty destination created concurrently.
            publish_directory(staging.path(), &output)?;
            session::sync_directory(&parent)?;
        }
        let artifact_parent = self.artifacts_dir()?;
        let receipt = tempfile::Builder::new()
            .prefix(".building-")
            .tempdir_in(artifact_parent)?;
        let result = self.finish_artifact(&key, receipt, manifest, now)?;
        Ok(WorkspaceResult {
            path: output,
            ..result
        })
    }

    fn workspace_gate(&self, now: u64) -> Result<()> {
        if self.state().status == SessionStatus::Stopped
            || self.state().in_flight.is_some()
            || self.state().execution.is_some()
        {
            return fail(
                "conflict",
                "workspace changes require a session that is not stopped and has no work in flight",
            );
        }
        if now < self.state().last_event_unix_ms {
            return fail("clock_regressed", "clock moved backwards");
        }
        if self.view(now).deadline_expired {
            return fail("budget_exhausted", "session deadline expired");
        }
        Ok(())
    }
    fn artifacts_dir(&self) -> Result<PathBuf> {
        let path = self.owned_path().join("artifacts");
        owned_directory(&path)?;
        Ok(path)
    }
    pub(crate) fn load_artifact(&self, key: &str) -> Result<(Manifest, String, PathBuf)> {
        let parent = self.owned_path().join("artifacts");
        session::check_directory(&parent)?;
        let path = parent.join(key);
        let manifest = read_manifest(&path)?;
        let digest = manifest_hash(&manifest)?;
        if self.state().artifacts.get(key) != Some(&digest)
            || manifest.config_sha256 != self.state().config_sha256
        {
            return fail(
                "corrupt_artifact",
                "artifact is uncommitted or does not match the journal",
            );
        }
        Ok((manifest, digest, path))
    }
    fn retry_artifact(
        &mut self,
        key: &str,
        operation: &str,
        request: &Request,
        now: u64,
    ) -> Result<Option<WorkspaceResult>> {
        let recorded = self.artifact_operation(operation, key)?;
        let parent = self.artifacts_dir()?;
        let path = parent.join(key);
        match path.symlink_metadata() {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if recorded.is_some() || self.state().artifacts.contains_key(key) {
                    return fail("corrupt_artifact", "a journaled artifact is missing");
                }
                if self.state().artifacts.len() >= 256 {
                    return fail("storage_limit", "session artifact count limit reached");
                }
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
            Ok(_) => (),
        }
        let manifest = read_manifest(&path)?;
        let digest = manifest_hash(&manifest)?;
        if manifest.operation_id != operation
            || &manifest.request != request
            || manifest.config_sha256 != self.state().config_sha256
        {
            return fail(
                "conflict",
                "artifact exists for another request or operation",
            );
        }
        if recorded.as_ref().is_some_and(|d| d != &digest)
            || self
                .state()
                .artifacts
                .get(key)
                .is_some_and(|d| d != &digest)
        {
            return fail(
                "corrupt_artifact",
                "artifact fingerprint differs from the journal",
            );
        }
        if matches!(request, Request::Workspace { .. } | Request::Seal { .. }) {
            verify_snapshot(&path, &manifest.files, self.config())?;
        }
        if let Request::Export { output, .. } = request {
            verify_export(Path::new(output), &digest, self.config())?;
        }
        if matches!(request, Request::Candidate { .. }) {
            session::check_directory(&path.join("tree"))?;
        }
        self.publish_artifact(operation, key, &digest, now)?;
        Ok(Some(result(key, &digest, &path, &manifest, true)))
    }
    fn finish_artifact(
        &mut self,
        key: &str,
        staging: tempfile::TempDir,
        manifest: Manifest,
        now: u64,
    ) -> Result<WorkspaceResult> {
        let digest = manifest_hash(&manifest)?;
        session::write_new(
            &staging.path().join("manifest.json"),
            &manifest_bytes(&manifest)?,
        )?;
        sync_tree(staging.path())?;
        let parent = self.artifacts_dir()?;
        // Includes staging, frozen trees, candidates, Git objects and old uncommitted stages.
        if directory_bytes(&parent)? > self.config().budget.max_artifact_bytes {
            return fail(
                "storage_limit",
                "session workspace artifacts exceed the byte budget",
            );
        }
        let path = parent.join(key);
        publish_directory(staging.path(), &path)?;
        session::sync_directory(&parent)?;
        self.publish_artifact(&manifest.operation_id, key, &digest, now)?;
        Ok(result(key, &digest, &path, &manifest, false))
    }
}

fn result(
    key: &str,
    digest: &str,
    path: &Path,
    manifest: &Manifest,
    already_applied: bool,
) -> WorkspaceResult {
    WorkspaceResult {
        artifact: key.into(),
        sha256: digest.into(),
        path: if matches!(manifest.request, Request::Export { .. }) {
            path.into()
        } else {
            path.join("tree")
        },
        files: manifest.files.len(),
        already_applied,
        evaluated: false,
    }
}
fn read_manifest(path: &Path) -> Result<Manifest> {
    session::check_directory(path)?;
    let bytes = session::read_bounded(&path.join("manifest.json"), MANIFEST_LIMIT)?;
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|_| SessionError::new("corrupt_artifact", "invalid artifact manifest"))?;
    if manifest.format_version != 1 || manifest.files.len() > MAX_FILES {
        return fail(
            "corrupt_artifact",
            "unsupported artifact version or file count",
        );
    }
    for (path, entry) in &manifest.files {
        safe_path(path)?;
        if ![0o100644, 0o100755].contains(&entry.mode) {
            return fail("corrupt_artifact", "unsupported file mode");
        }
    }
    Ok(manifest)
}
fn manifest_bytes(manifest: &Manifest) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|_| SessionError::new("corrupt_artifact", "cannot encode manifest"))?;
    if bytes.len() > MANIFEST_LIMIT {
        return fail("storage_limit", "artifact manifest exceeds 4 MiB");
    }
    Ok(bytes)
}
fn manifest_hash(manifest: &Manifest) -> Result<String> {
    Ok(session::hash(&serde_json::to_vec(manifest).map_err(
        |_| SessionError::new("corrupt_artifact", "cannot encode manifest"),
    )?))
}
pub(crate) fn inventory(files: &Contents) -> Inventory {
    files
        .iter()
        .map(|(p, (entry, _))| (p.clone(), entry.clone()))
        .collect()
}
fn fail<T>(code: &'static str, message: &str) -> Result<T> {
    Err(SessionError::new(code, message))
}
fn utf8_path(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| SessionError::new("unsafe_path", "filesystem paths must be UTF-8"))
}
fn candidate_id(id: &str) -> Result<()> {
    session::identifier(id)?;
    if id.len() > 64 {
        return fail("invalid_identifier", "candidate ID exceeds 64 bytes");
    }
    Ok(())
}
fn safe_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.len() > 4096
        || path.split('/').count() > 64
        || path
            .chars()
            .any(|c| c.is_control() || "\\:*?[]".contains(c))
        || path.split('/').any(|p| {
            p.is_empty()
                || p == "."
                || p == ".."
                || p.eq_ignore_ascii_case(".git")
                || p.eq_ignore_ascii_case(".auto")
                || p.trim_end_matches([' ', '.']) != p
        })
    {
        return fail(
            "unsafe_path",
            "unsupported path, reserved component or traversal",
        );
    }
    // Unicode filenames are accepted; reject combining marks to avoid normalization
    // aliases on macOS. ASCII case collisions are checked for every directory too.
    if path.chars().any(|c| ('\u{0300}'..='\u{036f}').contains(&c)) {
        return fail("unsafe_path", "decomposed filenames are not supported");
    }
    Ok(())
}
fn covers(paths: &[String], file: &str) -> bool {
    paths
        .iter()
        .any(|p| p == file || (p.ends_with('/') && file.starts_with(p)))
}
pub(crate) fn check_scope(
    base: &Inventory,
    files: &Inventory,
    config: &SessionConfig,
) -> Result<()> {
    for path in base.keys().chain(files.keys()) {
        if base.get(path) != files.get(path)
            && (!covers(&config.scope.allowed_paths, path)
                || covers(&config.scope.protected_paths, path))
        {
            return fail(
                "scope_violation",
                "candidate changed a protected or out-of-scope file",
            );
        }
    }
    Ok(())
}
fn check_protected(files: &Contents, config: &SessionConfig) -> Result<()> {
    for (path, expected) in &config.scope.protected_sha256 {
        if !files
            .get(path)
            .is_some_and(|(entry, _)| entry.sha256.eq_ignore_ascii_case(expected))
        {
            return fail(
                "scope_violation",
                "a declared protected file hash does not match the snapshot",
            );
        }
    }
    Ok(())
}
fn check_inventory(files: &Contents, config: &SessionConfig) -> Result<()> {
    if files.len() > MAX_FILES
        || files.values().map(|(e, _)| e.bytes).sum::<u64>()
            > config.budget.max_artifact_bytes.min(MAX_BYTES as u64)
    {
        return fail("storage_limit", "snapshot exceeds file count or byte limit");
    }
    let mut components = BTreeMap::new();
    for path in files.keys() {
        safe_path(path)?;
        let mut prefix = String::new();
        for part in path.split('/') {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            if let Some(prior) = components.insert(prefix.to_lowercase(), prefix.clone()) {
                if prior != prefix {
                    return fail("unsafe_path", "case-colliding paths are not supported");
                }
            }
        }
    }
    Ok(())
}
fn file_bytes(root: &Path, path: &str, limit: usize) -> Result<Option<(FileEntry, Vec<u8>)>> {
    safe_path(path)?;
    let mut current = root.to_path_buf();
    let parts: Vec<_> = path.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        current.push(part);
        let meta = match current.symlink_metadata() {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        if i + 1 < parts.len() {
            if !meta.is_dir() {
                if meta.is_file() {
                    return Ok(None);
                }
                return fail("unsafe_path", "parent path is not a real directory");
            }
        } else {
            if meta.is_dir() {
                return Ok(None);
            }
            let bytes = session::read_bounded(&current, limit)?;
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt;
                if meta.permissions().mode() & 0o100 != 0 {
                    0o100755
                } else {
                    0o100644
                }
            };
            #[cfg(not(unix))]
            let mode = 0o100644;
            return Ok(Some((
                FileEntry {
                    sha256: session::hash(&bytes),
                    bytes: bytes.len() as u64,
                    mode,
                },
                bytes,
            )));
        }
    }
    unreachable!()
}
pub(crate) fn scan(root: &Path, config: &SessionConfig, skip_generated: bool) -> Result<Contents> {
    session::check_directory(root)?;
    let mut files = BTreeMap::new();
    let mut stack = vec![String::new()];
    let mut visited = 0;
    let limit = config.budget.max_artifact_bytes.min(MAX_BYTES as u64) as usize;
    let mut bytes_used = 0;
    while let Some(prefix) = stack.pop() {
        for entry in fs::read_dir(root.join(&prefix))? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| SessionError::new("unsafe_path", "filenames must be UTF-8"))?;
            let path = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            safe_path(&path)?;
            visited += 1;
            if visited > MAX_FILES * 2 {
                return fail("storage_limit", "too many filesystem entries");
            }
            let meta = entry.path().symlink_metadata()?;
            if meta.is_dir() {
                stack.push(path);
            } else {
                let value = file_bytes(root, &path, limit.saturating_sub(bytes_used))?.ok_or_else(
                    || SessionError::new("source_changed", "file disappeared during snapshot"),
                )?;
                bytes_used += value.1.len();
                if !(skip_generated && covers(&config.scope.generated_paths, &path)) {
                    files.insert(path, value);
                }
            }
        }
    }
    check_inventory(&files, config)?;
    Ok(files)
}
pub(crate) fn verify_snapshot(
    path: &Path,
    expected: &Inventory,
    config: &SessionConfig,
) -> Result<Contents> {
    let files = scan(&path.join("tree"), config, false)?;
    if &inventory(&files) != expected {
        return fail(
            "corrupt_artifact",
            "frozen snapshot content or executable modes changed",
        );
    }
    Ok(files)
}
pub(crate) fn write_tree(root: &Path, files: &Contents) -> Result<()> {
    owned_directory(root)?;
    for (name, (entry, bytes)) in files {
        safe_path(name)?;
        let path = root.join(name);
        let mut parent = root.to_path_buf();
        for part in name.split('/').take(name.split('/').count() - 1) {
            parent.push(part);
            owned_directory(&parent)?;
        }
        session::write_new(&path, bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                &path,
                fs::Permissions::from_mode(if entry.mode == 0o100755 { 0o755 } else { 0o644 }),
            )?;
        }
        session::regular_open(&path, false)?.sync_all()?;
    }
    sync_tree(root)
}
pub(crate) fn owned_directory(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => session::sync_directory(path.parent().unwrap())?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    session::check_directory(path)
}
fn sync_tree(root: &Path) -> Result<()> {
    session::check_directory(root)?;
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.symlink_metadata()?.is_dir() {
            sync_tree(&path)?;
        } else {
            session::regular_open(&path, false)?.sync_all()?;
        }
    }
    session::sync_directory(root)
}
pub(crate) fn directory_bytes(root: &Path) -> Result<u64> {
    fn walk(root: &Path, depth: usize, count: &mut usize) -> Result<u64> {
        if depth > 70 || *count > 100_000 {
            return fail("storage_limit", "artifact directory limit exceeded");
        }
        session::check_directory(root)?;
        let mut total = 0_u64;
        for entry in fs::read_dir(root)? {
            let path = entry?.path();
            *count += 1;
            let meta = path.symlink_metadata()?;
            let size = if meta.is_dir() {
                walk(&path, depth + 1, count)?
            } else {
                session::regular_open(&path, false)?.metadata()?.len()
            };
            total = total
                .checked_add(size)
                .ok_or_else(|| SessionError::new("storage_limit", "artifact size overflow"))?;
        }
        Ok(total)
    }
    walk(root, 0, &mut 0)
}
fn publish_directory(staging: &Path, destination: &Path) -> Result<()> {
    // Atomic exclusive reservation prevents overwriting another publisher, including
    // an empty existing directory. A crash before rename leaves an empty reservation
    // for inspection, never a journaled artifact.
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(destination) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return fail("conflict", "destination already exists")
        }
        Err(e) => return Err(e.into()),
    }
    fs::rename(staging, destination)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportIntegrity {
    patch_sha256: String,
    base_files: Inventory,
}
fn verify_export(output: &Path, digest: &str, config: &SessionConfig) -> Result<()> {
    let manifest = read_manifest(output)?;
    if manifest_hash(&manifest)? != digest {
        return fail(
            "conflict",
            "export destination already contains another result",
        );
    }
    let integrity = manifest
        .export_proof
        .ok_or_else(|| SessionError::new("corrupt_artifact", "missing export proof"))?;
    if session::hash(&session::read_bounded(
        &output.join("candidate.patch"),
        MAX_BYTES * 3,
    )?) != integrity.patch_sha256
    {
        return fail("corrupt_artifact", "export patch was modified");
    }
    if inventory(&scan(&output.join("base"), config, false)?) != integrity.base_files {
        return fail("corrupt_artifact", "export baseline changed");
    }
    Ok(())
}

struct Git {
    directory: PathBuf,
    bare: bool,
}
impl Git {
    fn source(path: &Path) -> Result<Self> {
        session::check_directory(path)?;
        Ok(Self {
            directory: path.into(),
            bare: false,
        })
    }
    fn init(path: &Path) -> Result<Self> {
        let parent = path.parent().unwrap();
        let initializer = Self {
            directory: parent.into(),
            bare: false,
        };
        initializer.run(
            &[
                "init",
                "--bare",
                "--template=",
                "--object-format=sha1",
                path.to_str().ok_or_else(|| {
                    SessionError::new("unsafe_path", "Git directory must be UTF-8")
                })?,
            ],
            None,
        )?;
        Ok(Self {
            directory: path.into(),
            bare: true,
        })
    }
    fn text(&self, args: &[&str], input: Option<Vec<u8>>) -> Result<String> {
        String::from_utf8(self.run(args, input)?)
            .map(|s| s.trim_end_matches('\n').to_owned())
            .map_err(|_| SessionError::new("git_failed", "Git returned non-UTF-8 metadata"))
    }
    fn run(&self, args: &[&str], input: Option<Vec<u8>>) -> Result<Vec<u8>> {
        let mut command = Command::new("git");
        command
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_ATTR_NOSYSTEM", "1")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C")
            .env("GIT_ALLOW_PROTOCOL", "")
            .arg("--no-pager");
        for setting in [
            "core.hooksPath=/dev/null",
            "core.fsmonitor=false",
            "core.untrackedCache=false",
            "core.attributesFile=/dev/null",
            "core.autocrlf=false",
            "core.safecrlf=false",
            "core.fileMode=true",
            "protocol.allow=never",
            "gc.auto=0",
            "maintenance.auto=false",
        ] {
            command.args(["-c", setting]);
        }
        if self.bare {
            command.arg(format!("--git-dir={}", utf8_path(&self.directory)?));
        } else {
            command.arg("-C").arg(&self.directory);
        }
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn()?;
        let mut stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let writer = std::thread::spawn(move || -> std::io::Result<()> {
            if let Some(input) = input {
                stdin.write_all(&input)?;
            }
            Ok(())
        });
        let reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stdout
                .take((MAX_BYTES * 3 + 1) as u64)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        });
        let errors = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stderr.take(65537).read_to_end(&mut bytes).map(|_| bytes)
        });
        let start = Instant::now();
        let mut timed_out = false;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => (),
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(error);
                }
            }
            if start.elapsed() > Duration::from_secs(30) {
                timed_out = true;
                let _ = child.kill();
                break child.wait();
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let written = writer
            .join()
            .map_err(|_| SessionError::new("git_failed", "Git input thread failed"))?;
        let bytes = reader
            .join()
            .map_err(|_| SessionError::new("git_failed", "Git output thread failed"))??;
        let errors = errors
            .join()
            .map_err(|_| SessionError::new("git_failed", "Git error thread failed"))??;
        if timed_out {
            return fail("git_failed", "Git plumbing command exceeded 30 seconds");
        }
        if bytes.len() > MAX_BYTES * 3 || errors.len() > 65536 {
            return fail("storage_limit", "Git output exceeded its limit");
        }
        if !status?.success() {
            return fail(
                "git_failed",
                "Git plumbing command failed; check the repository and installed Git",
            );
        }
        written?;
        Ok(bytes)
    }
    fn store_tree(&self, files: &Contents) -> Result<String> {
        let mut blobs = BTreeMap::new();
        for (path, (entry, bytes)) in files {
            let oid = self.text(
                &["hash-object", "-w", "--stdin", "--no-filters"],
                Some(bytes.clone()),
            )?;
            blobs.insert(path.clone(), (entry.mode, oid));
        }
        self.mktree(&blobs)
    }
    fn mktree(&self, entries: &BTreeMap<String, (u32, String)>) -> Result<String> {
        let mut children: BTreeMap<String, BTreeMap<String, (u32, String)>> = BTreeMap::new();
        let mut input = Vec::new();
        for (path, (mode, oid)) in entries {
            if let Some((directory, rest)) = path.split_once('/') {
                children
                    .entry(directory.into())
                    .or_default()
                    .insert(rest.into(), (*mode, oid.clone()));
            } else {
                input.extend_from_slice(format!("{mode:o} blob {oid}\t{path}\0").as_bytes());
            }
        }
        for (directory, entries) in children {
            let oid = self.mktree(&entries)?;
            input.extend_from_slice(format!("40000 tree {oid}\t{directory}\0").as_bytes());
        }
        self.text(&["mktree", "-z"], Some(input))
    }
}
fn source_commit(git: &Git, commit: &str, config: &SessionConfig) -> Result<Contents> {
    let listing = git.run(&["ls-tree", "-r", "-z", "--full-tree", commit], None)?;
    let mut files = BTreeMap::new();
    for line in listing.split(|b| *b == 0).filter(|line| !line.is_empty()) {
        let text = std::str::from_utf8(line)
            .map_err(|_| SessionError::new("unsafe_path", "tracked paths must be UTF-8"))?;
        let (header, path) = text
            .split_once('\t')
            .ok_or_else(|| SessionError::new("git_failed", "invalid tree listing"))?;
        safe_path(path)?;
        let fields: Vec<_> = header.split(' ').collect();
        if fields.len() != 3 || fields[1] != "blob" || !["100644", "100755"].contains(&fields[0]) {
            return fail(
                "unsafe_path",
                "symlinks, submodules and special tracked modes are not supported",
            );
        }
        if covers(&config.scope.generated_paths, path) {
            return fail(
                "scope_violation",
                "declared generated paths must not contain committed source files",
            );
        }
        let size: u64 = git
            .text(&["cat-file", "-s", fields[2]], None)?
            .parse()
            .map_err(|_| SessionError::new("git_failed", "invalid blob size"))?;
        let used: u64 = files
            .values()
            .map(|(entry, _): &(FileEntry, Vec<u8>)| entry.bytes)
            .sum();
        if size
            > config
                .budget
                .max_artifact_bytes
                .min(MAX_BYTES as u64)
                .saturating_sub(used)
        {
            return fail("storage_limit", "source snapshot exceeds byte budget");
        }
        let bytes = git.run(&["cat-file", "blob", fields[2]], None)?;
        files.insert(
            path.into(),
            (
                FileEntry {
                    sha256: session::hash(&bytes),
                    bytes: bytes.len() as u64,
                    mode: u32::from_str_radix(fields[0], 8).unwrap(),
                },
                bytes,
            ),
        );
        check_inventory(&files, config)?;
    }
    Ok(files)
}
fn source_worktree(
    git: &Git,
    root: &Path,
    committed: &Contents,
    config: &SessionConfig,
) -> Result<Contents> {
    let listing = git.run(
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
        None,
    )?;
    let mut names: BTreeSet<String> = committed.keys().cloned().collect();
    for path in listing.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        let path = std::str::from_utf8(path)
            .map_err(|_| SessionError::new("unsafe_path", "source paths must be UTF-8"))?;
        if path
            .split('/')
            .next()
            .is_some_and(|p| p.eq_ignore_ascii_case(".auto"))
        {
            continue;
        }
        names.insert(path.into());
    }
    let mut files = BTreeMap::new();
    let mut used = 0;
    let limit = config.budget.max_artifact_bytes.min(MAX_BYTES as u64) as usize;
    for path in names {
        safe_path(&path)?;
        if covers(&config.scope.generated_paths, &path) {
            continue;
        }
        if let Some(value) = file_bytes(root, &path, limit.saturating_sub(used))? {
            used += value.1.len();
            files.insert(path, value);
        }
    }
    check_inventory(&files, config)?;
    Ok(files)
}
