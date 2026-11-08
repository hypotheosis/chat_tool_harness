mod history;
mod migrations;
pub use history::ConversationSummary;
#[cfg(test)]
mod history_tests;

use std::path::Path;
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use uuid::Uuid;

use crate::error::{CoreError, CoreResult};
use crate::model::core::*;

type Job = Box<dyn FnOnce(&mut Connection) + Send + 'static>;

struct Worker {
    sender: mpsc::Sender<Job>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        let (replacement, receiver) = mpsc::channel();
        let sender = std::mem::replace(&mut self.sender, replacement);
        drop(sender);
        drop(receiver);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Synchronous facade backed by one owned SQLite worker. Calls block waiting for its reply;
/// async/UI callers must marshal this facade off their executor.
#[derive(Clone)]
pub struct Repository {
    worker: Arc<Worker>,
}

impl Repository {
    pub fn open(path: &Path) -> CoreResult<Self> {
        let path = path.to_path_buf();
        let (sender, receiver) = mpsc::channel::<Job>();
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name("core-sqlite".into())
            .spawn(move || {
                let opened = Connection::open(&path)
                    .map_err(|_| CoreError::Storage)
                    .and_then(|mut connection| {
                        connection
                            .execute_batch("PRAGMA foreign_keys = ON")
                            .map_err(|_| CoreError::Storage)?;
                        migrations::apply(&mut connection).map_err(|_| CoreError::Storage)?;
                        Ok(connection)
                    });
                match opened {
                    Ok(mut connection) => {
                        if ready_sender.send(Ok(())).is_ok() {
                            for job in receiver {
                                job(&mut connection);
                            }
                        }
                    }
                    Err(error) => {
                        let _ = ready_sender.send(Err(error));
                    }
                }
            })
            .map_err(|_| CoreError::Storage)?;
        let ready = ready_receiver.recv().map_err(|_| CoreError::Storage)?;
        if let Err(error) = ready {
            let _ = handle.join();
            return Err(error);
        }
        Ok(Self {
            worker: Arc::new(Worker {
                sender,
                handle: Some(handle),
            }),
        })
    }

    pub fn save_endpoint(&self, config: EndpointConfig) -> CoreResult<EndpointId> {
        validate_endpoint(&config)?;
        self.with_connection(move |connection| {
            connection
                .execute(
                    "INSERT OR REPLACE INTO endpoints(id, payload) VALUES(?1, ?2)",
                    params![id(config.id), json(&config)?],
                )
                .map_err(|_| CoreError::Storage)?;
            Ok(config.id)
        })
    }
    pub fn save_server(&self, config: ServerConfig) -> CoreResult<ServerId> {
        validate_server(&config)?;
        self.with_connection(move |connection| {
            connection
                .execute(
                    "INSERT OR REPLACE INTO servers(id, payload) VALUES(?1, ?2)",
                    params![id(config.id), json(&config)?],
                )
                .map_err(|_| CoreError::Storage)?;
            Ok(config.id)
        })
    }
    pub fn save_profile(&self, profile: Profile) -> CoreResult<ProfileId> {
        if profile.name.trim().is_empty()
            || profile
                .server_ids
                .iter()
                .enumerate()
                .any(|(index, server)| profile.server_ids[..index].contains(server))
        {
            return Err(CoreError::InvalidInput);
        }
        self.with_connection(move |connection| {
            let transaction = connection.transaction().map_err(|_| CoreError::Storage)?;
            let exists: Option<String> = transaction
                .query_row(
                    "SELECT id FROM endpoints WHERE id=?1",
                    params![id(profile.endpoint_id)],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| CoreError::Storage)?;
            if exists.is_none() {
                return Err(CoreError::Conflict);
            }
            let revision: i64 = transaction
                .query_row(
                    "SELECT revision FROM profiles WHERE id=?1",
                    params![id(profile.id)],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map_err(|_| CoreError::Storage)?
                .unwrap_or(0)
                + 1;
            transaction
                .execute(
                    "INSERT OR REPLACE INTO profiles(id, payload, revision) VALUES(?1, ?2, ?3)",
                    params![id(profile.id), json(&profile)?, revision],
                )
                .map_err(|_| CoreError::Storage)?;
            transaction
                .execute(
                    "DELETE FROM profile_servers WHERE profile_id=?1",
                    params![id(profile.id)],
                )
                .map_err(|_| CoreError::Storage)?;
            for server in &profile.server_ids {
                transaction
                    .execute(
                        "INSERT INTO profile_servers(profile_id, server_id) VALUES(?1, ?2)",
                        params![id(profile.id), id(*server)],
                    )
                    .map_err(|_| CoreError::Conflict)?;
            }
            transaction.commit().map_err(|_| CoreError::Storage)?;
            Ok(profile.id)
        })
    }
    pub fn create_conversation(&self, profile_id: ProfileId) -> CoreResult<ConversationId> {
        let conversation = ConversationId::new();
        self.with_connection(move |connection| {
            let transaction = connection.transaction().map_err(|_| CoreError::Storage)?;
            let mut snapshot = profile_snapshot(&transaction, profile_id)?;
            snapshot.revision = 1;
            transaction.execute("INSERT INTO conversations(id, current_revision) VALUES(?1, 1)", params![id(conversation)]).map_err(|_| CoreError::Storage)?;
            transaction.execute("INSERT INTO conversation_snapshots(conversation_id, revision, payload) VALUES(?1, 1, ?2)", params![id(conversation), json(&snapshot)?]).map_err(|_| CoreError::Storage)?;
            transaction.commit().map_err(|_| CoreError::Storage)?;
            Ok(conversation)
        })
    }
    pub fn conversation(&self, conversation: ConversationId) -> CoreResult<ConversationSnapshot> {
        self.with_connection(move |connection| {
            let profile: ProfileSnapshot = connection
                .query_row(
                    "SELECT s.payload FROM conversation_snapshots s JOIN conversations c ON c.id=s.conversation_id AND c.current_revision=s.revision WHERE c.id=?1",
                    params![id(conversation)],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|_| CoreError::Storage)?
                .map(|value| from_json(&value))
                .transpose()?
                .ok_or(CoreError::Conflict)?;
            let mut statement = connection
                .prepare("SELECT id, state FROM runs WHERE conversation_id=?1 ORDER BY rowid")
                .map_err(|_| CoreError::Storage)?;
            let rows = statement
                .query_map(params![id(conversation)], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|_| CoreError::Storage)?;
            let mut runs = Vec::new();
            for row in rows {
                let (run_id, state) = row.map_err(|_| CoreError::Storage)?;
                runs.push(RunRecord {
                    id: RunId(parse(&run_id)?),
                    state: run_state(&state)?,
                    attempts: attempts(connection, &run_id)?,
                });
            }
            let mut statement = connection.prepare("SELECT id,payload FROM messages WHERE conversation_id=?1 ORDER BY ordinal").map_err(|_|CoreError::Storage)?;
            let messages = statement.query_map([id(conversation)], |r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).map_err(|_|CoreError::Storage)?.map(|r| {let (message_id,payload)=r.map_err(|_|CoreError::Storage)?; Ok(crate::agent::context::HistoryMessage {id:MessageId(parse(&message_id)?),message:from_json(&payload)?})}).collect::<CoreResult<Vec<_>>>()?;
            let mut statement = connection.prepare("SELECT payload FROM derived_context WHERE conversation_id=?1 ORDER BY rowid").map_err(|_|CoreError::Storage)?;
            let summaries = statement.query_map([id(conversation)], |r|r.get::<_,String>(0)).map_err(|_|CoreError::Storage)?.map(|r|from_json(&r.map_err(|_|CoreError::Storage)?)).collect::<CoreResult<Vec<_>>>()?;
            Ok(ConversationSnapshot {
                id: conversation,
                profile,
                runs,
                messages,
                summaries,
            })
        })
    }
    pub fn apply_profile(
        &self,
        conversation: ConversationId,
        profile_id: ProfileId,
    ) -> CoreResult<()> {
        self.with_connection(move |connection| {
            let transaction = connection.transaction().map_err(|_| CoreError::Storage)?;
            let mut snapshot = profile_snapshot(&transaction, profile_id)?;
            let current: i64 = transaction.query_row("SELECT current_revision FROM conversations WHERE id=?1", params![id(conversation)], |row| row.get(0)).optional().map_err(|_| CoreError::Storage)?.ok_or(CoreError::Conflict)?;
            let revision = current.checked_add(1).ok_or(CoreError::Limit)?;
            snapshot.revision = revision;
            transaction.execute("INSERT INTO conversation_snapshots(conversation_id, revision, payload) VALUES(?1, ?2, ?3)", params![id(conversation), revision, json(&snapshot)?]).map_err(|_| CoreError::Storage)?;
            transaction.execute("UPDATE conversations SET current_revision=?1 WHERE id=?2", params![revision, id(conversation)]).map_err(|_| CoreError::Storage)?;
            transaction.commit().map_err(|_| CoreError::Storage)?;
            Ok(())
        })
    }
    /// Rebuilds the conversation's profile snapshot from current settings and
    /// records a new revision only if anything changed. Returns whether it did.
    /// A deleted profile, endpoint or server fails with `Configuration` and
    /// notes a `RefreshProfile` failure detail in the caller's failure scope.
    pub fn refresh_profile(&self, conversation: ConversationId) -> CoreResult<bool> {
        let outcome = self.with_connection(move |connection| {
            let transaction = connection.transaction().map_err(|_| CoreError::Storage)?;
            let (current, payload): (i64, String) = transaction
                .query_row(
                    "SELECT c.current_revision, s.payload FROM conversations c JOIN conversation_snapshots s ON s.conversation_id=c.id AND s.revision=c.current_revision WHERE c.id=?1",
                    params![id(conversation)],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|_| CoreError::Storage)?
                .ok_or(CoreError::Conflict)?;
            let existing: ProfileSnapshot = from_json(&payload)?;
            let mut fresh = match profile_snapshot(&transaction, existing.profile_id) {
                Ok(fresh) => fresh,
                Err(CoreError::Conflict) => return Ok(Err(existing.name)),
                Err(error) => return Err(error),
            };
            fresh.revision = existing.revision;
            if fresh == existing {
                return Ok(Ok(false));
            }
            let revision = current.checked_add(1).ok_or(CoreError::Limit)?;
            fresh.revision = revision;
            transaction.execute("INSERT INTO conversation_snapshots(conversation_id, revision, payload) VALUES(?1, ?2, ?3)", params![id(conversation), revision, json(&fresh)?]).map_err(|_| CoreError::Storage)?;
            transaction.execute("UPDATE conversations SET current_revision=?1 WHERE id=?2", params![revision, id(conversation)]).map_err(|_| CoreError::Storage)?;
            transaction.commit().map_err(|_| CoreError::Storage)?;
            Ok(Ok(true))
        })?;
        // Note on the calling thread: the SQLite worker thread has no failure scope.
        outcome.map_err(|name| {
            crate::failure::note(crate::failure::FailureDetail {
                target: crate::failure::FailureTarget::Profile { name },
                step: crate::failure::FailureStep::RefreshProfile,
                url: None,
                status: None,
                reason: Some("profile, endpoint or server no longer exists".into()),
            });
            CoreError::Configuration
        })
    }
    pub fn conversation_profile_revision(
        &self,
        conversation: ConversationId,
        revision: i64,
    ) -> CoreResult<ProfileSnapshot> {
        self.with_connection(move |connection| {
            let payload: String = connection.query_row("SELECT payload FROM conversation_snapshots WHERE conversation_id=?1 AND revision=?2", params![id(conversation), revision], |row| row.get(0)).optional().map_err(|_| CoreError::Storage)?.ok_or(CoreError::Conflict)?;
            from_json(&payload)
        })
    }
    pub fn run_profile_revision(&self, run: RunId) -> CoreResult<i64> {
        self.with_connection(move |connection| {
            connection
                .query_row(
                    "SELECT snapshot_revision FROM runs WHERE id=?1",
                    params![id(run)],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| CoreError::Storage)?
                .ok_or(CoreError::Conflict)
        })
    }
    pub fn recover_interrupted(&self) -> CoreResult<Vec<RunId>> {
        self.with_connection(|connection| {
            let transaction = connection.transaction().map_err(|_| CoreError::Storage)?;
            let mut statement = transaction.prepare("SELECT id FROM runs WHERE state IN ('Running', 'WaitingApproval', 'Paused')").map_err(|_| CoreError::Storage)?;
            let ids: Vec<String> = statement.query_map([], |row| row.get(0)).map_err(|_| CoreError::Storage)?.collect::<Result<_, _>>().map_err(|_| CoreError::Storage)?;
            drop(statement);
            transaction.execute("UPDATE runs SET state='Interrupted' WHERE state IN ('Running', 'WaitingApproval', 'Paused')", []).map_err(|_| CoreError::Storage)?;
            transaction.execute("UPDATE attempts SET state='OutcomeUnknown' WHERE state='Dispatching'", []).map_err(|_| CoreError::Storage)?;
            transaction.commit().map_err(|_| CoreError::Storage)?;
            ids.iter().map(|value| parse(value).map(RunId)).collect()
        })
    }
}

fn profile_snapshot(connection: &Connection, profile_id: ProfileId) -> CoreResult<ProfileSnapshot> {
    let (profile_json, revision): (String, i64) = connection
        .query_row(
            "SELECT payload, revision FROM profiles WHERE id=?1",
            params![id(profile_id)],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| CoreError::Storage)?
        .ok_or(CoreError::Conflict)?;
    let profile: Profile = from_json(&profile_json)?;
    let endpoint_json: String = connection
        .query_row(
            "SELECT payload FROM endpoints WHERE id=?1",
            params![id(profile.endpoint_id)],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| CoreError::Storage)?
        .ok_or(CoreError::Conflict)?;
    let endpoint = from_json(&endpoint_json)?;
    let mut servers = Vec::new();
    for server_id in &profile.server_ids {
        let value: String = connection
            .query_row(
                "SELECT payload FROM servers WHERE id=?1",
                params![id(*server_id)],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| CoreError::Storage)?
            .ok_or(CoreError::Conflict)?;
        servers.push(from_json(&value)?);
    }
    Ok(ProfileSnapshot {
        mode: profile.mode,
        profile_id,
        revision,
        name: profile.name,
        endpoint,
        servers,
        instructions: profile.instructions,
        limits: profile.limits,
    })
}
fn attempts(connection: &Connection, run: &str) -> CoreResult<Vec<AttemptRecord>> {
    let mut statement = connection
        .prepare("SELECT id, state FROM attempts WHERE run_id=?1 ORDER BY rowid")
        .map_err(|_| CoreError::Storage)?;
    let result = statement
        .query_map(params![run], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|_| CoreError::Storage)?
        .map(|row| {
            let (id_value, state) = row.map_err(|_| CoreError::Storage)?;
            Ok(AttemptRecord {
                id: AttemptId(parse(&id_value)?),
                state: attempt_state(&state)?,
            })
        })
        .collect();
    result
}
impl Repository {
    pub(crate) fn with_connection<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Connection) -> CoreResult<T> + Send + 'static,
    ) -> CoreResult<T> {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.worker
            .sender
            .send(Box::new(move |connection| {
                let _ = sender.send(operation(connection));
            }))
            .map_err(|_| CoreError::Storage)?;
        receiver.recv().map_err(|_| CoreError::Storage)?
    }
    #[cfg(test)]
    pub(crate) fn conversation_for_run_for_test(
        &self,
        run: RunId,
    ) -> CoreResult<ConversationSnapshot> {
        let conversation = self.with_connection(move |connection| {
            let value: String = connection
                .query_row(
                    "SELECT conversation_id FROM runs WHERE id=?1",
                    [id(run)],
                    |row| row.get(0),
                )
                .map_err(|_| CoreError::Storage)?;
            Ok(ConversationId(parse(&value)?))
        })?;
        self.conversation(conversation)
    }
    #[cfg(test)]
    pub(crate) fn run_for_attempt_for_test(&self, attempt: AttemptId) -> CoreResult<RunId> {
        self.with_connection(move |connection| {
            let run: String = connection
                .query_row(
                    "SELECT run_id FROM attempts WHERE id=?1",
                    params![id(attempt)],
                    |row| row.get(0),
                )
                .map_err(|_| CoreError::Storage)?;
            Ok(RunId(parse(&run)?))
        })
    }
    #[cfg(test)]
    pub(crate) fn schema_version_for_test(&self) -> CoreResult<i64> {
        self.with_connection(|connection| {
            connection
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|_| CoreError::Storage)
        })
    }
    #[cfg(test)]
    pub(crate) fn insert_run_for_test(
        &self,
        run: RunId,
        conversation: ConversationId,
        state: RunState,
    ) -> CoreResult<()> {
        self.with_connection(move |connection| {
            connection
                .execute(
                    "INSERT INTO runs(id, conversation_id, snapshot_revision, state) VALUES(?1, ?2, (SELECT current_revision FROM conversations WHERE id=?2), ?3)",
                    params![id(run), id(conversation), format!("{state:?}")],
                )
                .map_err(|_| CoreError::Storage)?;
            Ok(())
        })
    }
    #[cfg(test)]
    pub(crate) fn insert_attempt_for_test(
        &self,
        attempt: AttemptId,
        run: RunId,
        state: AttemptState,
    ) -> CoreResult<()> {
        self.with_connection(move |connection| {
            connection
                .execute(
                    "INSERT INTO attempts(id, run_id, state) VALUES(?1, ?2, ?3)",
                    params![id(attempt), id(run), format!("{state:?}")],
                )
                .map_err(|_| CoreError::Storage)?;
            Ok(())
        })
    }
}

trait SqlId {
    fn sql_id(self) -> String;
}
macro_rules! sql_id { ($($type:ident),* $(,)?) => { $(impl SqlId for $type { fn sql_id(self) -> String { self.0.to_string() } })* }; }
sql_id!(
    ConversationId,
    ProfileId,
    EndpointId,
    ServerId,
    RunId,
    AttemptId,
    AttachmentId,
    ArtifactId,
    CredentialRef
);
fn id<T: SqlId>(value: T) -> String {
    value.sql_id()
}
fn json<T: Serialize>(value: &T) -> CoreResult<String> {
    serde_json::to_string(value).map_err(|_| CoreError::Storage)
}
fn from_json<T: DeserializeOwned>(value: &str) -> CoreResult<T> {
    serde_json::from_str(value).map_err(|_| CoreError::Storage)
}
fn parse(value: &str) -> CoreResult<Uuid> {
    Uuid::parse_str(value).map_err(|_| CoreError::Storage)
}
fn validate_url(url: &str, allow_http: bool) -> CoreResult<()> {
    let parsed = url::Url::parse(url).map_err(|_| CoreError::InvalidInput)?;
    if parsed.host_str().is_none()
        || !(parsed.scheme() == "https" || (allow_http && parsed.scheme() == "http"))
    {
        return Err(CoreError::InvalidInput);
    }
    Ok(())
}
fn valid_http_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
}
fn validate_server(config: &ServerConfig) -> CoreResult<()> {
    if config.name.trim().is_empty()
        || config
            .secret_headers
            .iter()
            .any(|(name, _)| !valid_http_token(name))
    {
        return Err(CoreError::InvalidInput);
    }
    validate_url(&config.url, config.allow_http)
}
fn validate_endpoint(config: &EndpointConfig) -> CoreResult<()> {
    if config.name.trim().is_empty()
        || config.model.trim().is_empty()
        || config.context_tokens == 0
        || config.max_output_tokens == 0
        || !config.temperature.is_finite()
    {
        return Err(CoreError::InvalidInput);
    }
    validate_url(&config.base_url, config.allow_http)
}
fn run_state(value: &str) -> CoreResult<RunState> {
    match value {
        "Running" => Ok(RunState::Running),
        "WaitingApproval" => Ok(RunState::WaitingApproval),
        "Paused" => Ok(RunState::Paused),
        "Completed" => Ok(RunState::Completed),
        "Cancelled" => Ok(RunState::Cancelled),
        "Failed" => Ok(RunState::Failed),
        "Interrupted" => Ok(RunState::Interrupted),
        _ => Err(CoreError::Storage),
    }
}
fn attempt_state(value: &str) -> CoreResult<AttemptState> {
    match value {
        "Proposed" => Ok(AttemptState::Proposed),
        "Denied" => Ok(AttemptState::Denied),
        "Approved" => Ok(AttemptState::Approved),
        "Dispatching" => Ok(AttemptState::Dispatching),
        "Succeeded" => Ok(AttemptState::Succeeded),
        "Failed" => Ok(AttemptState::Failed),
        "OutcomeUnknown" => Ok(AttemptState::OutcomeUnknown),
        _ => Err(CoreError::Storage),
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
