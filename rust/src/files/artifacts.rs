use crate::error::{CoreError, CoreResult};
use crate::model::core::*;
use crate::storage::Repository;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MAX_TEXT_BYTES: u64 = 1024 * 1024;
pub const RETENTION_BYTES: u64 = 256 * 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactSummary {
    pub id: ArtifactId,
    pub conversation_id: ConversationId,
    pub name: String,
    pub bytes: u64,
}
#[derive(Clone)]
pub struct ArtifactStore {
    repo: Repository,
    root: PathBuf,
}
impl ArtifactStore {
    pub fn open(repo: Repository, root: &Path) -> CoreResult<Self> {
        std::fs::create_dir_all(root).map_err(|_| CoreError::Storage)?;
        let store = Self {
            repo,
            root: root.to_path_buf(),
        };
        let root = store.root.clone();
        store.repo.with_connection(move |conn| {
            let tx = conn
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(storage)?;
            let saved = rows(&tx)?;
            for entry in std::fs::read_dir(&root).map_err(storage)? {
                let entry = entry.map_err(storage)?;
                let path = entry.path();
                if !owned_path(&path) {
                    continue;
                }
                if path.extension().is_some_and(|s| s == "deleting") {
                    let original = path.with_extension("txt");
                    if saved.iter().any(|a| original == file_path(&root, a.id)) {
                        std::fs::rename(&path, &original).map_err(storage)?;
                        continue;
                    }
                }
                if path.extension().is_some_and(|s| s != "txt")
                    || !saved.iter().any(|a| path == file_path(&root, a.id))
                {
                    std::fs::remove_file(path).map_err(storage)?;
                }
            }
            tx.commit().map_err(storage)
        })?;
        Ok(store)
    }
    pub(crate) fn private_locations(&self) -> Vec<String> {
        self.root
            .parent()
            .and_then(|p| p.to_str())
            .map(|p| vec![p.to_owned()])
            .unwrap_or_default()
    }
    pub fn create(
        &self,
        owner: ConversationId,
        name: &str,
        text: &str,
    ) -> CoreResult<ArtifactSummary> {
        safe_name(name)?;
        if text.len() as u64 > MAX_TEXT_BYTES {
            return Err(CoreError::Limit);
        }
        let summary = ArtifactSummary {
            id: ArtifactId::new(),
            conversation_id: owner,
            name: name.into(),
            bytes: text.len() as u64,
        };
        let text = text.to_owned();
        let root = self.root.clone();
        let artifact = summary.clone();
        self.repo.with_connection(move |conn| {
            // IMMEDIATE serializes quota admission across repository connections, not just clones.
            let tx = conn
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(storage)?;
            require_owner(&tx, owner)?;
            let saved = rows(&tx)?;
            let mut retained = saved.iter().map(|a| a.bytes).sum::<u64>();
            for entry in std::fs::read_dir(&root).map_err(storage)? {
                let path = entry.map_err(storage)?.path();
                if owned_path(&path) && !saved.iter().any(|a| path == file_path(&root, a.id)) {
                    retained = retained
                        .checked_add(std::fs::symlink_metadata(path).map_err(storage)?.len())
                        .ok_or(CoreError::Limit)?;
                }
            }
            if retained
                .checked_add(artifact.bytes)
                .ok_or(CoreError::Limit)?
                > RETENTION_BYTES
            {
                return Err(CoreError::Limit);
            }
            let pending = root.join(format!("{}.part", artifact.id.0));
            let final_path = file_path(&root, artifact.id);
            let result = (|| {
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&pending)
                    .map_err(storage)?;
                file.write_all(text.as_bytes()).map_err(storage)?;
                file.sync_all().map_err(storage)?;
                std::fs::rename(&pending, &final_path).map_err(storage)?;
                std::fs::File::open(&root)
                    .and_then(|f| f.sync_all())
                    .map_err(storage)?;
                tx.execute(
                    "INSERT INTO artifacts(id,conversation_id,payload) VALUES(?1,?2,?3)",
                    rusqlite::params![
                        artifact.id.0.to_string(),
                        owner.0.to_string(),
                        serde_json::to_string(&artifact).map_err(storage)?
                    ],
                )
                .map_err(storage)?;
                tx.execute(
                    "INSERT INTO events(conversation_id,payload) VALUES(?1,'artifact-created')",
                    [owner.0.to_string()],
                )
                .map_err(storage)?;
                tx.commit().map_err(storage)?;
                Ok(artifact)
            })();
            if result.is_err() {
                // Only these freshly allocated names can be provisional. Restart retries cleanup.
                for path in [&pending, &final_path] {
                    if path.exists() {
                        let _ = std::fs::remove_file(path);
                    }
                }
            }
            result
        })
    }
    pub fn list(&self, owner: ConversationId) -> CoreResult<Vec<ArtifactSummary>> {
        self.repo.with_connection(move |conn| {
            require_owner(conn, owner)?;
            Ok(rows(conn)?
                .into_iter()
                .filter(|a| a.conversation_id == owner)
                .collect())
        })
    }
    pub fn all(&self) -> CoreResult<Vec<ArtifactSummary>> {
        self.repo.with_connection(|conn| rows(conn))
    }
    pub fn read(
        &self,
        owner: ConversationId,
        id: ArtifactId,
    ) -> CoreResult<(ArtifactSummary, String, String)> {
        let root = self.root.clone();
        self.repo.with_connection(move |conn| {
            let artifact = rows(conn)?
                .into_iter()
                .find(|a| a.id == id && a.conversation_id == owner)
                .ok_or(CoreError::Conflict)?;
            let path = file_path(&root, id);
            let meta = std::fs::symlink_metadata(&path).map_err(storage)?;
            if !meta.is_file() || meta.len() != artifact.bytes || meta.len() > MAX_TEXT_BYTES {
                return Err(CoreError::Storage);
            }
            let text = std::fs::read_to_string(&path).map_err(storage)?;
            Ok((
                artifact,
                text,
                path.to_str().ok_or(CoreError::Storage)?.to_owned(),
            ))
        })
    }
    pub fn delete_conversation(
        &self,
        owner: ConversationId,
        confirmed_unknown: bool,
    ) -> CoreResult<()> {
        let root = self.root.clone();
        self.repo.with_connection(move |conn| {
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(storage)?;
            require_owner(&tx, owner)?;
            let active: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM runs WHERE conversation_id=?1 AND state='Running')", [owner.0.to_string()], |r|r.get(0)).map_err(storage)?;
            let unknown: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM attempts a JOIN runs r ON r.id=a.run_id WHERE r.conversation_id=?1 AND a.state IN ('Dispatching','OutcomeUnknown'))", [owner.0.to_string()], |r|r.get(0)).map_err(storage)?;
            if active || (unknown && !confirmed_unknown) { return Err(CoreError::Conflict); }
            let owned: Vec<_> = rows(&tx)?.into_iter().filter(|a|a.conversation_id==owner).collect();
            let mut staged = Vec::new();
            let result = (|| {
                for artifact in owned {
                    let original = file_path(&root, artifact.id);
                    let tombstone = original.with_extension("deleting");
                    std::fs::rename(&original, &tombstone).map_err(storage)?;
                    staged.push((original,tombstone));
                }
                std::fs::File::open(&root).and_then(|f|f.sync_all()).map_err(storage)?;
                let owner = owner.0.to_string();
                tx.execute("DELETE FROM pages WHERE attachment_id IN (SELECT id FROM attachments WHERE conversation_id=?1)",[&owner]).map_err(storage)?;
                tx.execute("DELETE FROM derived_context WHERE conversation_id=?1",[&owner]).map_err(storage)?;
                tx.execute("DELETE FROM attempts WHERE run_id IN (SELECT id FROM runs WHERE conversation_id=?1)",[&owner]).map_err(storage)?;
                for table in ["runs","messages","attachments","artifacts","disclosure_acknowledgments","conversation_snapshots"] {
                    tx.execute(&format!("DELETE FROM {table} WHERE conversation_id=?1"),[&owner]).map_err(storage)?;
                }
                tx.execute("DELETE FROM events WHERE conversation_id=?1",[&owner]).map_err(storage)?;
                tx.execute("DELETE FROM conversations WHERE id=?1",[&owner]).map_err(storage)?;
                tx.execute("INSERT INTO events(conversation_id,payload) VALUES(?1,'conversation-deleted')",[&owner]).map_err(storage)?;
                tx.commit().map_err(storage)
            })();
            if let Err(error) = result {
                // Same reconciliation protocol as single-artifact deletion, including partial staging.
                for (original,tombstone) in staged { let _ = std::fs::rename(tombstone,original); }
                return Err(error);
            }
            for (_,tombstone) in staged { std::fs::remove_file(tombstone).map_err(storage)?; }
            std::fs::File::open(&root).and_then(|f|f.sync_all()).map_err(storage)
        })
    }
    pub fn delete(&self, owner: ConversationId, id: ArtifactId) -> CoreResult<()> {
        let root = self.root.clone();
        self.repo.with_connection(move |conn| {
            let tx = conn
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(storage)?;
            if !rows(&tx)?
                .iter()
                .any(|a| a.id == id && a.conversation_id == owner)
            {
                return Err(CoreError::Conflict);
            }
            let original = file_path(&root, id);
            let tombstone = original.with_extension("deleting");
            // A crash before commit restores this file; after commit it becomes an owned orphan.
            std::fs::rename(&original, &tombstone).map_err(storage)?;
            let committed = (|| {
                std::fs::File::open(&root)
                    .and_then(|f| f.sync_all())
                    .map_err(storage)?;
                tx.execute(
                    "DELETE FROM artifacts WHERE id=?1 AND conversation_id=?2",
                    rusqlite::params![id.0.to_string(), owner.0.to_string()],
                )
                .map_err(storage)?;
                tx.execute(
                    "INSERT INTO events(conversation_id,payload) VALUES(?1,'artifact-deleted')",
                    [owner.0.to_string()],
                )
                .map_err(storage)?;
                tx.commit().map_err(storage)
            })();
            if let Err(error) = committed {
                // If restoration also fails, restart reconciliation retains the saved row and restores it.
                let _ = std::fs::rename(&tombstone, &original);
                return Err(error);
            }
            std::fs::remove_file(tombstone).map_err(storage)
        })
    }
}

fn storage(_: impl std::fmt::Debug) -> CoreError {
    CoreError::Storage
}
fn safe_name(name: &str) -> CoreResult<()> {
    if name.len() > 128
        || name.trim() != name
        || name.starts_with('.')
        || name
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
        || !(name.ends_with(".md") || name.ends_with(".txt"))
    {
        return Err(CoreError::InvalidInput);
    }
    Ok(())
}
fn file_path(root: &Path, id: ArtifactId) -> PathBuf {
    root.join(format!("{}.txt", id.0))
}
fn owned_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e == "txt" || e == "part" || e == "deleting")
        && path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok_and(|id| id.to_string() == s))
}
fn require_owner(conn: &rusqlite::Connection, owner: ConversationId) -> CoreResult<()> {
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM conversations WHERE id=?1)",
            [owner.0.to_string()],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if exists {
        Ok(())
    } else {
        Err(CoreError::Conflict)
    }
}
fn rows(conn: &rusqlite::Connection) -> CoreResult<Vec<ArtifactSummary>> {
    let mut stmt = conn
        .prepare("SELECT payload FROM artifacts ORDER BY rowid")
        .map_err(storage)?;
    let values = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(storage)?;
    values
        .map(|v| serde_json::from_str(&v.map_err(storage)?).map_err(storage))
        .collect()
}
impl crate::agent::LocalFileTools for ArtifactStore {
    fn definitions(
        &self,
        repo: &Repository,
        profile: ProfileId,
    ) -> CoreResult<Vec<ToolDefinition>> {
        definitions(repo, profile)
    }
    fn invoke(&self, owner: ConversationId, call: ToolCall) -> crate::agent::ToolFuture {
        let store = self.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Create {
                    name: String,
                    content: String,
                }
                if call.key.server != ToolServer::LocalFiles || call.key.name != "create_artifact" {
                    return Err(CoreError::InvalidInput);
                }
                let args: Create = serde_json::from_str(&call.arguments_json)
                    .map_err(|_| CoreError::InvalidInput)?;
                let artifact = match store.create(owner, &args.name, &args.content) {
                    Ok(artifact) => artifact,
                    Err(error) => {
                        return Ok(crate::mcp::ToolOutcome {
                            is_error: true,
                            text: vec![error.to_string()],
                            structured_content: None,
                            unsupported_content: vec![],
                        })
                    }
                };
                Ok(crate::mcp::ToolOutcome {
                    is_error: false,
                    text: vec![format!(
                        "Created {} ({} bytes), id {}",
                        artifact.name, artifact.bytes, artifact.id.0
                    )],
                    structured_content: Some(serde_json::to_value(artifact).map_err(storage)?),
                    unsupported_content: vec![],
                })
            })
            .await
            .map_err(storage)?
        })
    }
}
pub fn definitions(repo: &Repository, profile: ProfileId) -> CoreResult<Vec<ToolDefinition>> {
    Ok(vec![ToolDefinition::from_discovery(
        repo,
        ToolKey {
            profile_id: profile,
            server: ToolServer::LocalFiles,
            name: "create_artifact".into(),
        },
        r#"{"type":"object","properties":{"name":{"type":"string","maxLength":128},"content":{"type":"string"}},"required":["name","content"],"additionalProperties":false}"#,
    )?])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (
        tempfile::TempDir,
        Repository,
        ConversationId,
        ConversationId,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::open(&dir.path().join("core.db")).unwrap();
        let endpoint = EndpointId::new();
        repo.save_endpoint(EndpointConfig {
            id: endpoint,
            name: "test".into(),
            base_url: "https://example.test".into(),
            model: "m".into(),
            context_tokens: 4096,
            max_output_tokens: 128,
            temperature: 0.0,
            allow_http: false,
            credential: None,
        })
        .unwrap();
        let profile = ProfileId::new();
        repo.save_profile(Profile {
            mode: crate::model::core::ProfileMode::Agent,
            id: profile,
            name: "test".into(),
            endpoint_id: endpoint,
            server_ids: vec![],
            instructions: String::new(),
            limits: RunLimits::default(),
        })
        .unwrap();
        let a = repo.create_conversation(profile).unwrap();
        let b = repo.create_conversation(profile).unwrap();
        (dir, repo, a, b)
    }
    #[test]
    fn files_persist_opaque_owned_utf8_and_reconcile_only_owned_orphans() {
        let (dir, repo, a, b) = fixture();
        let root = dir.path().join("artifacts");
        let store = ArtifactStore::open(repo.clone(), &root).unwrap();
        let artifact = store
            .create(a, "notes.md", "# <script>literal</script> é")
            .expect("generated file");
        assert!(store.read(b, artifact.id).is_err());
        let (_, text, path) = store.read(a, artifact.id).unwrap();
        assert_eq!(text, "# <script>literal</script> é");
        assert!(!path.contains("notes.md"));
        let orphan = root.join(format!("{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(&orphan, b"orphan").unwrap();
        let pending = root.join(format!("{}.part", uuid::Uuid::new_v4()));
        std::fs::write(&pending, b"partial").unwrap();
        std::fs::write(root.join("unowned"), b"keep").unwrap();
        drop(store);
        drop(repo);
        let repo = Repository::open(&dir.path().join("core.db")).unwrap();
        let store = ArtifactStore::open(repo, &root).unwrap();
        assert_eq!(store.list(a).unwrap(), vec![artifact]);
        assert_eq!(store.read(a, store.list(a).unwrap()[0].id).unwrap().1, text);
        assert!(!orphan.exists());
        assert!(!pending.exists());
        assert!(root.join("unowned").exists());
    }
    #[test]
    fn files_concurrent_quota_reservations_never_delete_history() {
        let (dir, repo, a, b) = fixture();
        let root = dir.path().join("artifacts");
        let store = ArtifactStore::open(repo, &root).unwrap();
        let independent = ArtifactStore::open(
            Repository::open(&dir.path().join("core.db")).unwrap(),
            &root,
        )
        .unwrap();
        let saved = store.create(a, "saved.md", "history").unwrap();
        let orphan = root.join(format!("{}.part", uuid::Uuid::new_v4()));
        std::fs::File::create(&orphan)
            .unwrap()
            .set_len(RETENTION_BYTES - saved.bytes - 1)
            .unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|index| {
                let s = if index == 0 {
                    store.clone()
                } else {
                    independent.clone()
                };
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    s.create(b, "new.txt", "x")
                })
            })
            .collect();
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert!(results.contains(&Err(CoreError::Limit)));
        assert_eq!(store.read(a, saved.id).unwrap().1, "history");
        assert!(store.delete(b, saved.id).is_err());
        let other = store.list(b).unwrap()[0].clone();
        store.delete(b, other.id).unwrap();
        assert_eq!(store.read(a, saved.id).unwrap().1, "history");
    }
    #[test]
    fn files_interrupted_delete_restores_saved_metadata_and_reclaims_committed_tombstone() {
        let (dir, repo, a, _) = fixture();
        let root = dir.path().join("artifacts");
        let store = ArtifactStore::open(repo.clone(), &root).unwrap();
        let saved = store.create(a, "keep.txt", "saved history").unwrap();
        let tombstone = root.join(format!("{}.deleting", saved.id.0));
        std::fs::rename(file_path(&root, saved.id), &tombstone).unwrap();
        let orphan = root.join(format!("{}.deleting", uuid::Uuid::new_v4()));
        std::fs::write(&orphan, b"deleted").unwrap();
        let store = ArtifactStore::open(repo.clone(), &root).unwrap();
        assert_eq!(
            store
                .read(a, saved.id)
                .expect("rollback deletion keeps saved file")
                .1,
            "saved history"
        );
        assert!(!tombstone.exists());
        assert!(!orphan.exists());
        repo.with_connection(|conn|conn.execute_batch("CREATE TRIGGER reject_delete BEFORE DELETE ON artifacts BEGIN SELECT RAISE(FAIL, 'test failure'); END;").map_err(storage)).unwrap();
        assert_eq!(store.delete(a, saved.id), Err(CoreError::Storage));
        assert_eq!(store.read(a, saved.id).unwrap().1, "saved history");
    }
    #[test]
    fn files_metadata_failure_rolls_back_and_removes_provisional_file() {
        let (dir, repo, a, _) = fixture();
        let root = dir.path().join("artifacts");
        let store = ArtifactStore::open(repo.clone(), &root).unwrap();
        let saved = store.create(a, "saved.txt", "keep").unwrap();
        repo.with_connection(|conn|conn.execute_batch("CREATE TRIGGER reject_artifact BEFORE INSERT ON artifacts BEGIN SELECT RAISE(FAIL, 'test storage full'); END;").map_err(storage)).unwrap();
        assert_eq!(store.create(a, "failed.md", "new"), Err(CoreError::Storage));
        assert_eq!(store.list(a).unwrap(), vec![saved.clone()]);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        assert_eq!(store.read(a, saved.id).unwrap().1, "keep");
    }
    #[test]
    fn files_safe_names_one_mib_boundary_and_write_failure_have_no_metadata() {
        let (dir, repo, a, _) = fixture();
        let root = dir.path().join("artifacts");
        let store = ArtifactStore::open(repo, &root).unwrap();
        for name in [
            "../escape.md",
            "/tmp/x.txt",
            "a/b.md",
            "a\\b.txt",
            "binary.pdf",
            ".md",
            "bad\n.txt",
        ] {
            assert!(store.create(a, name, "x").is_err(), "{name}");
        }
        assert!(store.create(ConversationId::new(), "a.txt", "x").is_err());
        let text = "é".repeat(MAX_TEXT_BYTES as usize / 2);
        store
            .create(a, "boundary.md", &text)
            .expect("exact UTF8 byte cap");
        assert_eq!(
            store.create(a, "too.txt", &(text + "x")),
            Err(CoreError::Limit)
        );
        let before = store.list(a).unwrap();
        std::fs::rename(&root, dir.path().join("held")).unwrap();
        std::fs::write(&root, b"not a directory").unwrap();
        assert!(store.create(a, "failed.txt", "x").is_err());
        assert_eq!(store.list(a).unwrap(), before);
    }
}
