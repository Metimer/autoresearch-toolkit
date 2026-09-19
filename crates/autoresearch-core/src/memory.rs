//! Disposable local search index. Journal-bound evidence remains authoritative.
use crate::{
    session::{self, Decision, SessionError, SessionStore},
    workspace,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs};
type Result<T> = std::result::Result<T, SessionError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryRow {
    pub session_id: String,
    pub evaluation: String,
    pub evidence_sha256: String,
    pub candidate: Option<String>,
    pub hypothesis: Option<String>,
    pub changed_paths: Vec<String>,
    pub decision: Decision,
    pub reason: String,
    pub parent_tree_sha256: String,
    pub patch_sha256: Option<String>,
    pub protocol_sha256: Option<String>,
    pub duplicate_key: Option<String>,
    pub duplicate_of: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryIndex {
    pub format_version: u32,
    pub session_sequences: BTreeMap<String, u64>,
    pub rows: Vec<MemoryRow>,
}

impl MemoryIndex {
    /// Text proximity is informative only. Never suppress or schedule work.
    pub fn search(&self, query: &str) -> Vec<&MemoryRow> {
        let query = query.to_lowercase();
        self.rows
            .iter()
            .filter(|row| {
                format!(
                    "{} {} {} {} {}",
                    row.session_id,
                    row.candidate.as_deref().unwrap_or(""),
                    row.hypothesis.as_deref().unwrap_or(""),
                    row.reason,
                    row.changed_paths.join(" ")
                )
                .to_lowercase()
                .contains(&query)
            })
            .collect()
    }
}

impl SessionStore {
    /// Rebuild under all selected session locks. Busy/corrupt sessions fail the
    /// complete query rather than silently providing an incomplete duplicate set.
    pub fn memory(&self) -> Result<MemoryIndex> {
        let directory = self.directory(false)?;
        let mut names = Vec::new();
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| SessionError::new("unsafe_path", "non-UTF-8 session name"))?;
            if name.starts_with('.') {
                continue;
            } // unpublished staging, never history
            session::identifier(&name)?;
            names.push(name);
            if names.len() > 256 {
                return Err(SessionError::new(
                    "storage_limit",
                    "memory supports at most 256 sessions per pilot",
                ));
            }
        }
        names.sort();
        let guards = names
            .iter()
            .map(|name| self.open(name, false))
            .collect::<Result<Vec<_>>>()?;
        let mut index = MemoryIndex {
            format_version: 1,
            session_sequences: BTreeMap::new(),
            rows: Vec::new(),
        };
        let mut duplicates: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut index_bytes = 0_usize;
        for guard in &guards {
            index
                .session_sequences
                .insert(guard.state().session_id.clone(), guard.state().sequence);
            for key in guard
                .state()
                .artifacts
                .keys()
                .filter(|key| key.starts_with("run-"))
            {
                if index.rows.len() >= 4096 {
                    return Err(SessionError::new(
                        "storage_limit",
                        "memory supports at most 4096 evaluations",
                    ));
                }
                let evaluation = guard.evaluation(key)?;
                let report = &evaluation.report;
                let (manifest, digest, _) = guard.load_artifact(&report.parent)?;
                if digest != report.parent_sha256 {
                    return Err(SessionError::new(
                        "corrupt_artifact",
                        "evaluation reference differs from artifact",
                    ));
                }
                let parent = guard.snapshot(&report.parent)?;
                let parent_tree_sha256 =
                    session::hash(&serde_json::to_vec(&manifest.files).unwrap());
                let mut row = MemoryRow {
                    session_id: guard.state().session_id.clone(),
                    evaluation: key.clone(),
                    evidence_sha256: evaluation.sha256,
                    candidate: report.candidate.clone(),
                    hypothesis: None,
                    changed_paths: Vec::new(),
                    decision: report.decision,
                    reason: report.reason.clone(),
                    parent_tree_sha256,
                    patch_sha256: None,
                    protocol_sha256: report.protocol_sha256.clone(),
                    duplicate_key: None,
                    duplicate_of: Vec::new(),
                };
                if let Some(candidate) = &report.candidate {
                    let (sealed, digest, _) = guard.load_artifact(candidate)?;
                    if Some(digest) != report.candidate_sha256 {
                        return Err(SessionError::new(
                            "corrupt_artifact",
                            "evaluation candidate differs from artifact",
                        ));
                    }
                    let files = guard.snapshot(candidate)?;
                    let id = candidate.strip_prefix("sealed-").ok_or_else(|| {
                        SessionError::new("corrupt_artifact", "invalid evaluated candidate key")
                    })?;
                    row.hypothesis = Some(guard.hypothesis(id)?);
                    row.changed_paths = manifest
                        .files
                        .keys()
                        .chain(sealed.files.keys())
                        .filter(|path| manifest.files.get(*path) != sealed.files.get(*path))
                        .cloned()
                        .collect();
                    row.changed_paths.sort();
                    row.changed_paths.dedup();
                    let patch = session::hash(&workspace::verified_patch(&parent, &files)?);
                    // Missing execution-time protocol evidence never gets guessed.
                    if let Some(protocol) = &row.protocol_sha256 {
                        let identity = session::hash(
                            &serde_json::to_vec(&(&row.parent_tree_sha256, &patch, protocol))
                                .unwrap(),
                        );
                        let prior = duplicates.entry(identity.clone()).or_default();
                        row.duplicate_of = prior.clone();
                        prior.push(format!("{}/{}", row.session_id, key));
                        row.duplicate_key = Some(identity);
                    }
                    row.patch_sha256 = Some(patch);
                }
                index_bytes += serde_json::to_vec(&row).unwrap().len();
                if index_bytes > 16 * 1024 * 1024 {
                    return Err(SessionError::new(
                        "storage_limit",
                        "memory rows exceed 16 MiB",
                    ));
                }
                index.rows.push(row);
            }
        }
        Ok(index)
    }

    /// The cache is private, atomic and replaceable; it authorizes nothing.
    pub fn rebuild_memory(&self, operation: &str) -> Result<MemoryIndex> {
        session::identifier(operation)?;
        let parent = self.directory(false)?.parent().unwrap().to_path_buf();
        let _lock = session::acquire(&parent.join("memory.lock"), true)?;
        let index = self.memory()?;
        let bytes = serde_json::to_vec_pretty(&index).unwrap();
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(SessionError::new(
                "storage_limit",
                "memory index exceeds 16 MiB",
            ));
        }
        session::atomic_replace(&parent.join("memory.json"), &bytes)?;
        Ok(index)
    }
}
