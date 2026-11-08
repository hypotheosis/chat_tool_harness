use crate::error::{CoreError, CoreResult};
use crate::failure::FailureDetail;
use crate::inference::{ChatMessage, ChatRole};
use crate::model::core::*;
use crate::storage::Repository;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

/// A run failure as persisted in `runs.failure`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredFailure {
    pub code: CoreError,
    /// What failed and where; none when nothing noted the failure, so no
    /// target is guessed.
    #[serde(default)]
    pub detail: Option<FailureDetail>,
}
impl StoredFailure {
    /// Parses a stored value; values written before details existed were the
    /// plain `CoreError` display text and load as that code without detail.
    fn parse(stored: String) -> Self {
        serde_json::from_str(&stored).unwrap_or_else(|_| Self {
            code: CoreError::ALL
                .into_iter()
                .find(|code| code.to_string() == stored)
                .unwrap_or(CoreError::Protocol),
            detail: None,
        })
    }
    /// One-line display text: the noted reason, or the code's text.
    pub fn display(&self) -> String {
        self.detail
            .as_ref()
            .and_then(|d| d.reason.clone())
            .unwrap_or_else(|| self.code.to_string())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredMessage {
    pub role: String,
    pub content: String,
    pub calls: Vec<StoredCall>,
    pub call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_envelope: Option<MessageId>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCall {
    pub key: ToolKey,
    pub provider_call_id: String,
    pub arguments_json: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_envelope: Option<MessageId>,
}
impl From<&ToolCall> for StoredCall {
    fn from(call: &ToolCall) -> Self {
        Self {
            key: call.key.clone(),
            provider_call_id: call.provider_call_id.clone(),
            arguments_json: call.arguments_json.clone(),
            source_envelope: None,
        }
    }
}
impl StoredCall {
    pub fn call(&self) -> ToolCall {
        ToolCall {
            key: self.key.clone(),
            provider_call_id: self.provider_call_id.clone(),
            arguments_json: self.arguments_json.clone(),
        }
    }
}
impl StoredMessage {
    pub fn chat(self) -> CoreResult<ChatMessage> {
        let role = match self.role.as_str() {
            "system" => ChatRole::System,
            "user" => ChatRole::User,
            "assistant" => ChatRole::Assistant,
            "tool" => ChatRole::Tool,
            _ => return Err(CoreError::Storage),
        };
        Ok(ChatMessage {
            role,
            content: self.content,
            tool_calls: self.calls.iter().map(StoredCall::call).collect(),
            tool_call_id: self.call_id,
        })
    }
}
#[derive(Debug, Clone)]
pub struct PendingAttempt {
    pub id: AttemptId,
    pub call: ToolCall,
    pub state: AttemptState,
    pub binding: Option<ApprovalBinding>,
}
#[derive(Debug, Clone)]
pub struct RunData {
    pub id: RunId,
    pub conversation: ConversationId,
    pub profile: ProfileSnapshot,
    pub state: RunState,
    pub inference_count: u32,
    pub attempt_count: u32,
    pub active_ms: u64,
    pub messages: Vec<StoredMessage>,
    pub attempts: Vec<PendingAttempt>,
    pub partial_text: String,
    pub failure: Option<StoredFailure>,
    pub context: super::context::RunContext,
    /// The user message that started a turn run; none for summary runs.
    pub input_message_id: Option<String>,
}
fn sql_error(_: rusqlite::Error) -> CoreError {
    CoreError::Storage
}
fn json<T: Serialize>(value: &T) -> CoreResult<String> {
    serde_json::to_string(value).map_err(|_| CoreError::Storage)
}
fn parse<T: serde::de::DeserializeOwned>(value: &str) -> CoreResult<T> {
    serde_json::from_str(value).map_err(|_| CoreError::Storage)
}
fn uuid(value: String) -> CoreResult<uuid::Uuid> {
    uuid::Uuid::parse_str(&value).map_err(|_| CoreError::Storage)
}
fn state(value: &str) -> CoreResult<RunState> {
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
fn event(
    tx: &rusqlite::Transaction<'_>,
    conversation: ConversationId,
    payload: &str,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO events(conversation_id,payload) VALUES(?1,?2)",
        params![conversation.0.to_string(), payload],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// Rejects an empty or oversized user message.
pub fn validate_turn_text(text: &str) -> CoreResult<()> {
    if text.trim().is_empty() || text.len() > crate::inference::MAX_TEXT_BYTES {
        return Err(CoreError::InvalidInput);
    }
    Ok(())
}
pub fn start(
    repo: &Repository,
    run: RunId,
    conversation: ConversationId,
    text: String,
) -> CoreResult<()> {
    validate_turn_text(&text)?;
    repo.with_connection(move |conn| {
        let tx = conn.transaction().map_err(sql_error)?;
        let revision: i64 = tx.query_row("SELECT current_revision FROM conversations WHERE id=?1", [conversation.0.to_string()], |r|r.get(0)).optional().map_err(sql_error)?.ok_or(CoreError::Conflict)?;
        let message = StoredMessage { role:"user".into(), content:text, calls:vec![], call_id:None, source_envelope:None };
        let input_id=uuid::Uuid::new_v4().to_string();
        tx.execute("INSERT INTO runs(id,conversation_id,snapshot_revision,state,input_message_id) VALUES(?1,?2,?3,'Running',?4)", params![run.0.to_string(),conversation.0.to_string(),revision,input_id]).map_err(sql_error)?;
        tx.execute("INSERT INTO messages(id,conversation_id,snapshot_revision,ordinal,payload) VALUES(?1,?2,?3,(SELECT count(*) FROM messages WHERE conversation_id=?2),?4)",params![input_id,conversation.0.to_string(),revision,json(&message)?]).map_err(sql_error)?;
        event(&tx, conversation, &format!("run:{}:Running",run.0))?;
        tx.commit().map_err(sql_error)?;
        Ok(())
    })
}
fn insert_message(
    tx: &rusqlite::Transaction<'_>,
    conversation: ConversationId,
    revision: i64,
    id: MessageId,
    message: &StoredMessage,
) -> CoreResult<()> {
    tx.execute("INSERT INTO messages(id,conversation_id,snapshot_revision,ordinal,payload) VALUES(?1,?2,?3,(SELECT count(*) FROM messages WHERE conversation_id=?2),?4)", params![id.0.to_string(),conversation.0.to_string(),revision,json(message)?]).map_err(sql_error)?;
    Ok(())
}
pub fn load(repo: &Repository, run: RunId) -> CoreResult<RunData> {
    repo.with_connection(move |conn| {
        let (conversation, revision, status, inference_count, attempt_count, active_ms,partial_text,failure,input_message_id): (String,i64,String,i64,i64,i64,String,Option<String>,Option<String>) = conn.query_row("SELECT conversation_id,snapshot_revision,state,inference_count,attempt_count,active_ms,partial_text,failure,input_message_id FROM runs WHERE id=?1",[run.0.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?))).optional().map_err(sql_error)?.ok_or(CoreError::Conflict)?;
        let conversation = ConversationId(uuid(conversation)?);
        let profile: String = conn.query_row("SELECT payload FROM conversation_snapshots WHERE conversation_id=?1 AND revision=?2", params![conversation.0.to_string(),revision],|r|r.get(0)).map_err(sql_error)?;
        let mut stmt = conn.prepare("SELECT payload FROM messages WHERE conversation_id=?1 AND ordinal >= (SELECT min(ordinal) FROM messages WHERE conversation_id=?1 AND id=(SELECT input_message_id FROM runs WHERE id=?2)) ORDER BY ordinal").map_err(sql_error)?;
        let messages: Vec<StoredMessage> = stmt.query_map(params![conversation.0.to_string(),run.0.to_string()],|r|r.get::<_,String>(0)).map_err(sql_error)?.map(|row|parse(&row.map_err(sql_error)?)).collect::<CoreResult<_>>()?;
        drop(stmt);
        if messages.is_empty() && conn.query_row("SELECT input_message_id FROM runs WHERE id=?1",[run.0.to_string()],|r|r.get::<_,Option<String>>(0)).map_err(sql_error)?.is_some(){return Err(CoreError::Storage);}
        let mut stmt = conn.prepare("SELECT id,state,call_json,approval_binding FROM attempts WHERE run_id=?1 ORDER BY rowid").map_err(sql_error)?;
        let attempts = stmt.query_map([run.0.to_string()],|r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<String>>(3)?))).map_err(sql_error)?.map(|row| {
            let (id,status,call,binding)=row.map_err(sql_error)?;
            Ok(PendingAttempt { id:AttemptId(uuid(id)?),state:attempt_state(&status)?,call:parse::<StoredCall>(&call)?.call(),binding:binding.map(|v|parse(&v)).transpose()? })
        }).collect::<CoreResult<Vec<_>>>()?;
        let context:String=conn.query_row("SELECT context_json FROM runs WHERE id=?1",[run.0.to_string()],|r|r.get(0)).map_err(sql_error)?;
        Ok(RunData { context:parse(&context)?, id:run, conversation, profile:parse(&profile)?, state:state(&status)?, inference_count:u32::try_from(inference_count).map_err(|_|CoreError::Storage)?,attempt_count:u32::try_from(attempt_count).map_err(|_|CoreError::Storage)?,active_ms:u64::try_from(active_ms).map_err(|_|CoreError::Storage)?,messages,attempts,partial_text,failure:failure.map(StoredFailure::parse),input_message_id })
    })
}
/// Moves a resumable run from `from` to Running and, in the same transaction,
/// points it at its conversation's current snapshot revision. A run whose
/// state changed meanwhile (for example a concurrent cancel) is a Conflict
/// and keeps its revision.
pub fn resume_on_current(repo: &Repository, run: RunId, from: RunState) -> CoreResult<()> {
    if !matches!(
        from,
        RunState::WaitingApproval | RunState::Paused | RunState::Interrupted
    ) {
        return Err(CoreError::Conflict);
    }
    let from = format!("{from:?}");
    repo.with_connection(move |conn| {
        let tx = conn.transaction().map_err(sql_error)?;
        let (conversation, status): (String, String) = tx
            .query_row(
                "SELECT conversation_id,state FROM runs WHERE id=?1",
                [run.0.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?
            .ok_or(CoreError::Conflict)?;
        if status != from {
            return Err(CoreError::Conflict);
        }
        let changed = tx
            .execute(
                "UPDATE runs SET state='Running',snapshot_revision=(SELECT c.current_revision FROM conversations c WHERE c.id=runs.conversation_id) WHERE id=?1 AND state=?2",
                params![run.0.to_string(), from],
            )
            .map_err(sql_error)?;
        if changed != 1 {
            return Err(CoreError::Conflict);
        }
        event(
            &tx,
            ConversationId(uuid(conversation)?),
            &format!("run:{}:Running", run.0),
        )?;
        tx.commit().map_err(sql_error)?;
        Ok(())
    })
}
pub fn account(repo: &Repository, run: RunId, total: u64) -> CoreResult<()> {
    repo.with_connection(move |conn| {
        let total = i64::try_from(total).map_err(|_| CoreError::Limit)?;
        conn.execute(
            "UPDATE runs SET active_ms=max(active_ms,?1) WHERE id=?2",
            params![total, run.0.to_string()],
        )
        .map_err(sql_error)?;
        Ok(())
    })
}
pub fn cancel(repo: &Repository, run: RunId) -> CoreResult<()> {
    repo.with_connection(move |conn| {
        let tx = conn.transaction().map_err(sql_error)?;
        let (conversation, status): (String, String) = tx
            .query_row(
                "SELECT conversation_id,state FROM runs WHERE id=?1",
                [run.0.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?
            .ok_or(CoreError::Conflict)?;
        if !matches!(
            status.as_str(),
            "Running" | "WaitingApproval" | "Paused" | "Interrupted" | "Cancelled"
        ) {
            return Err(CoreError::Conflict);
        }
        tx.execute(
            "UPDATE runs SET state='Cancelled' WHERE id=?1",
            [run.0.to_string()],
        )
        .map_err(sql_error)?;
        event(
            &tx,
            ConversationId(uuid(conversation)?),
            &format!("run:{}:Cancelled", run.0),
        )?;
        tx.commit().map_err(sql_error)?;
        Ok(())
    })
}
pub fn settle(
    repo: &Repository,
    run: RunId,
    status: RunState,
    error: Option<CoreError>,
    detail: Option<FailureDetail>,
    total: u64,
) -> CoreResult<()> {
    settle_with(repo, run, status, error, detail, total, false)
}
/// Settles like [`settle`] but keeps a failure already stored for the run.
pub fn settle_keeping_failure(
    repo: &Repository,
    run: RunId,
    status: RunState,
    error: Option<CoreError>,
    total: u64,
) -> CoreResult<()> {
    settle_with(repo, run, status, error, None, total, true)
}
fn settle_with(
    repo: &Repository,
    run: RunId,
    status: RunState,
    error: Option<CoreError>,
    detail: Option<FailureDetail>,
    total: u64,
    keep_existing: bool,
) -> CoreResult<()> {
    repo.with_connection(move |conn| {
        let tx = conn.transaction().map_err(sql_error)?;
        let (conversation, previous): (String, String) = tx
            .query_row(
                "SELECT conversation_id,state FROM runs WHERE id=?1",
                [run.0.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(sql_error)?;
        // A stop is not a diagnosable failure; keep only its code.
        let stopped =
            previous == "Cancelled" || matches!(status, RunState::Cancelled | RunState::Interrupted);
        let detail = detail.filter(|_| !stopped);
        // Only a note says what failed; without one the code stands alone
        // rather than blaming a guessed target.
        let failure = error
            .map(|code| json(&StoredFailure { code, detail }))
            .transpose()?;
        let status = if previous == "Cancelled" {
            previous
        } else {
            format!("{status:?}")
        };
        tx.execute(
            "UPDATE attempts SET state='OutcomeUnknown' WHERE run_id=?1 AND state='Dispatching'",
            [run.0.to_string()],
        )
        .map_err(sql_error)?;
        tx.execute(
            if keep_existing {
                "UPDATE runs SET state=?1,failure=COALESCE(failure,?2),active_ms=max(active_ms,?3) WHERE id=?4"
            } else {
                "UPDATE runs SET state=?1,failure=?2,active_ms=max(active_ms,?3) WHERE id=?4"
            },
            params![
                status,
                failure,
                i64::try_from(total).map_err(|_| CoreError::Limit)?,
                run.0.to_string()
            ],
        )
        .map_err(sql_error)?;
        event(
            &tx,
            ConversationId(uuid(conversation)?),
            &format!("run:{}:{status}", run.0),
        )?;
        tx.commit().map_err(sql_error)?;
        Ok(())
    })
}
pub fn start_summary(
    repo: &Repository,
    run: RunId,
    conversation: ConversationId,
    selection: super::context::ContextSelection,
) -> CoreResult<()> {
    let snapshot = repo.conversation(conversation)?;
    super::context::validate_selection(&snapshot, &selection)?;
    if selection.message_ids.is_empty() && selection.summary_ids.is_empty() {
        return Err(CoreError::InvalidInput);
    }
    let page_references = snapshot
        .summaries
        .iter()
        .filter(|s| selection.summary_ids.contains(&s.id))
        .flat_map(|s| s.page_references.clone())
        .collect();
    let summary = super::context::DerivedContext {
        id: MessageId::new(),
        run_id: run,
        text: String::new(),
        partial_coverage: selection.message_ids.len() != snapshot.messages.len()
            || snapshot
                .summaries
                .iter()
                .filter(|s| selection.summary_ids.contains(&s.id))
                .any(|s| s.partial_coverage),
        selection: selection.clone(),
        page_references,
        unresolved_attempts: super::context::unresolved(&snapshot),
    };
    let context = super::context::RunContext {
        purpose: super::context::ContextPurpose::Summary,
        selection: Some(selection),
        summary: Some(summary),
        ..Default::default()
    };
    repo.with_connection(move |conn| {
        let tx=conn.transaction().map_err(sql_error)?;
        tx.execute("INSERT INTO runs(id,conversation_id,snapshot_revision,state,context_json) VALUES(?1,?2,?3,'Running',?4)",params![run.0.to_string(),conversation.0.to_string(),snapshot.profile.revision,json(&context)?]).map_err(sql_error)?;
        event(&tx,conversation,&format!("run:{}:summary:Running",run.0))?;
        tx.commit().map_err(sql_error)
    })
}
pub fn context_usage(
    repo: &Repository,
    run: RunId,
    usage: super::context::ContextUsage,
) -> CoreResult<()> {
    repo.with_connection(move |conn| {
        let tx = conn.transaction().map_err(sql_error)?;
        let (conversation, context): (String, String) = tx
            .query_row(
                "SELECT conversation_id,context_json FROM runs WHERE id=?1",
                [run.0.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(sql_error)?;
        let mut context: super::context::RunContext = parse(&context)?;
        context.usage = Some(usage);
        tx.execute(
            "UPDATE runs SET context_json=?1 WHERE id=?2",
            params![json(&context)?, run.0.to_string()],
        )
        .map_err(sql_error)?;
        event(
            &tx,
            ConversationId(uuid(conversation)?),
            &format!("run:{}:context", run.0),
        )?;
        tx.commit().map_err(sql_error)
    })
}
pub fn messages_after(
    repo: &Repository,
    conversation: ConversationId,
    ordinal: i64,
) -> CoreResult<Vec<MessageId>> {
    repo.with_connection(move |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id FROM messages WHERE conversation_id=?1 AND ordinal>?2 ORDER BY ordinal",
            )
            .map_err(sql_error)?;
        let ids = stmt
            .query_map(params![conversation.0.to_string(), ordinal], |r| {
                r.get::<_, String>(0)
            })
            .map_err(sql_error)?
            .map(|r| Ok(MessageId(uuid(r.map_err(sql_error)?)?)))
            .collect::<CoreResult<Vec<_>>>()?;
        Ok(ids)
    })
}
pub fn apply_selection(
    repo: &Repository,
    run: RunId,
    selection: super::context::ContextSelection,
) -> CoreResult<()> {
    let data = load(repo, run)?;
    if data.state != RunState::Paused {
        return Err(CoreError::Conflict);
    }
    let snapshot = repo.conversation(data.conversation)?;
    super::context::validate_selection(&snapshot, &selection)?;
    let conversation = data.conversation;
    let (input,through):(Option<String>,i64)=repo.with_connection(move |conn| {
        conn.query_row("SELECT input_message_id,coalesce((SELECT max(ordinal) FROM messages WHERE conversation_id=?2),-1) FROM runs WHERE id=?1",params![run.0.to_string(),conversation.0.to_string()],|r|Ok((r.get(0)?,r.get(1)?))).map_err(sql_error)
    })?;
    let input = input.map(|id| uuid(id).map(MessageId)).transpose()?;
    if input.is_some_and(|id| !selection.message_ids.contains(&id)) {
        return Err(CoreError::InvalidInput);
    }
    // Explicitly omitting completed exchanges is allowed. Pending requests must keep
    // their assistant envelope so newly appended results never become orphan tools.
    let unresolved: Vec<_> = data
        .attempts
        .iter()
        .filter(|a| {
            matches!(
                a.state,
                AttemptState::Proposed
                    | AttemptState::Approved
                    | AttemptState::Dispatching
                    | AttemptState::OutcomeUnknown
            )
        })
        .collect();
    if snapshot
        .messages
        .iter()
        .skip_while(|m| Some(m.id) != input)
        .any(|m| {
            m.message.calls.iter().any(|c| {
                unresolved
                    .iter()
                    .any(|a| a.call.provider_call_id == c.provider_call_id && a.call.key == c.key)
            }) && !selection.message_ids.contains(&m.id)
        })
    {
        return Err(CoreError::InvalidInput);
    }
    if data.context.purpose == super::context::ContextPurpose::Summary {
        return Err(CoreError::Conflict);
    }
    repo.with_connection(move |conn| {
        let tx = conn.transaction().map_err(sql_error)?;
        let mut context = data.context;
        context.selection = Some(selection);
        context.selection_through_ordinal = Some(through);
        context.usage = None;
        let changed = tx
            .execute(
                "UPDATE runs SET context_json=?1 WHERE id=?2 AND state='Paused'",
                params![json(&context)?, run.0.to_string()],
            )
            .map_err(sql_error)?;
        if changed != 1 {
            return Err(CoreError::Conflict);
        }
        event(
            &tx,
            data.conversation,
            &format!("run:{}:context-selection", run.0),
        )?;
        tx.commit().map_err(sql_error)
    })
}
pub fn complete_summary(
    repo: &Repository,
    run: RunId,
    text: String,
    complete: bool,
) -> CoreResult<()> {
    repo.with_connection(move |conn| {
        let tx = conn.transaction().map_err(sql_error)?;
        let (conversation, status, context): (String, String, String) = tx
            .query_row(
                "SELECT conversation_id,state,context_json FROM runs WHERE id=?1",
                [run.0.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(sql_error)?;
        if status != "Running" {
            return Err(CoreError::Cancelled);
        }
        let mut context: super::context::RunContext = parse(&context)?;
        let summary = context.summary.as_mut().ok_or(CoreError::Storage)?;
        summary.text = text;
        summary.partial_coverage |= !complete;
        context.derived_entry = Some(summary.id);
        tx.execute(
            "INSERT INTO derived_context(id,conversation_id,run_id,payload) VALUES(?1,?2,?3,?4)",
            params![
                summary.id.0.to_string(),
                conversation,
                run.0.to_string(),
                json(summary)?
            ],
        )
        .map_err(sql_error)?;
        // Completing the run and publishing its derived entry is one durable operation.
        tx.execute(
            "UPDATE runs SET context_json=?1,partial_text='',state='Completed' WHERE id=?2",
            params![json(&context)?, run.0.to_string()],
        )
        .map_err(sql_error)?;
        event(
            &tx,
            ConversationId(uuid(conversation)?),
            &format!("run:{}:derived-summary", run.0),
        )?;
        tx.commit().map_err(sql_error)
    })
}
pub fn acknowledge_disclosure(
    repo: &Repository,
    conversation: ConversationId,
    acknowledgment: super::context::DisclosureAcknowledgment,
) -> CoreResult<()> {
    repo.with_connection(move |conn| {
        let tx = conn.transaction().map_err(sql_error)?;
        // Only an origin this conversation has actually been shown may be acknowledged.
        // Newest revisions are checked first; an unreadable old revision is skipped
        // rather than blocking acknowledgment of a readable one.
        let shown = {
            let mut statement = tx
                .prepare(
                    "SELECT payload FROM conversation_snapshots WHERE conversation_id=?1 ORDER BY revision DESC",
                )
                .map_err(sql_error)?;
            let mut rows = statement
                .query([conversation.0.to_string()])
                .map_err(sql_error)?;
            let mut shown = false;
            while let Some(row) = rows.next().map_err(sql_error)? {
                let payload: String = row.get(0).map_err(sql_error)?;
                let Ok(profile) = parse::<ProfileSnapshot>(&payload) else {
                    continue;
                };
                if super::context::endpoint_origin(&profile.endpoint)
                    .ok()
                    .as_deref()
                    == Some(acknowledgment.origin.as_str())
                {
                    shown = true;
                    break;
                }
            }
            shown
        };
        if !shown {
            return Err(CoreError::InvalidInput);
        }
        tx.execute(
            "INSERT OR IGNORE INTO acknowledged_origins(origin) VALUES(?1)",
            [&acknowledgment.origin],
        )
        .map_err(sql_error)?;
        event(&tx, conversation, "context:disclosure-acknowledged")?;
        tx.commit().map_err(sql_error)
    })
}
/// The app-wide acknowledgment for `profile`'s inference origin, if one exists.
pub fn disclosure(
    repo: &Repository,
    profile: &ProfileSnapshot,
) -> CoreResult<Option<super::context::DisclosureAcknowledgment>> {
    let origin = super::context::endpoint_origin(&profile.endpoint)?;
    Ok(origin_acknowledged(repo, &origin)?
        .then_some(super::context::DisclosureAcknowledgment { origin }))
}
pub fn origin_acknowledged(repo: &Repository, origin: &str) -> CoreResult<bool> {
    let origin = origin.to_owned();
    repo.with_connection(move |conn| {
        conn.query_row(
            "SELECT 1 FROM acknowledged_origins WHERE origin=?1",
            [origin],
            |_| Ok(()),
        )
        .optional()
        .map(|found| found.is_some())
        .map_err(sql_error)
    })
}
pub fn inference_started(repo: &Repository, run: RunId, active_ms: u64) -> CoreResult<()> {
    repo.with_connection(move |conn| {
        let tx=conn.transaction().map_err(sql_error)?;
        let (status,count,spent,profile):(String,i64,i64,String)=tx.query_row("SELECT r.state,r.inference_count,r.active_ms,s.payload FROM runs r JOIN conversation_snapshots s ON s.conversation_id=r.conversation_id AND s.revision=r.snapshot_revision WHERE r.id=?1",[run.0.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(sql_error)?;
        let snapshot:ProfileSnapshot=parse(&profile)?;
        if status!="Running" {return Err(CoreError::Cancelled);}
        if count>=i64::from(snapshot.limits.max_inference_requests) || u64::try_from(spent).map_err(|_|CoreError::Storage)?.saturating_add(active_ms)>=snapshot.limits.active_timeout_ms {return Err(CoreError::Limit);}
        tx.execute("UPDATE runs SET inference_count=inference_count+1,active_ms=active_ms+?1,partial_text='' WHERE id=?2",params![i64::try_from(active_ms).map_err(|_|CoreError::Limit)?,run.0.to_string()]).map_err(sql_error)?;
        tx.commit().map_err(sql_error)?;
        Ok(())
    })
}
pub fn partial(repo: &Repository, run: RunId, text: String) -> CoreResult<()> {
    repo.with_connection(move |conn| {
        let tx = conn.transaction().map_err(sql_error)?;
        let conversation: String = tx
            .query_row(
                "SELECT conversation_id FROM runs WHERE id=?1",
                [run.0.to_string()],
                |r| r.get(0),
            )
            .map_err(sql_error)?;
        tx.execute(
            "UPDATE runs SET partial_text=partial_text || ?1 WHERE id=?2",
            params![text, run.0.to_string()],
        )
        .map_err(sql_error)?;
        event(
            &tx,
            ConversationId(uuid(conversation)?),
            &format!("run:{}:partial", run.0),
        )?;
        tx.commit().map_err(sql_error)?;
        Ok(())
    })
}
pub fn assistant_and_propose(
    repo: &Repository,
    run: RunId,
    text: String,
    calls: Vec<ToolCall>,
    active_ms: u64,
) -> CoreResult<Vec<AttemptId>> {
    repo.with_connection(move |conn| {
        let tx=conn.transaction().map_err(sql_error)?;
        let (conversation,revision,status,count,profile):(String,i64,String,i64,String)=tx.query_row("SELECT r.conversation_id,r.snapshot_revision,r.state,r.attempt_count,s.payload FROM runs r JOIN conversation_snapshots s ON s.conversation_id=r.conversation_id AND s.revision=r.snapshot_revision WHERE r.id=?1",[run.0.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(sql_error)?;
        let snapshot:ProfileSnapshot=parse(&profile)?;
        if status!="Running" {return Err(CoreError::Cancelled);}
        let count_add=i64::try_from(calls.len()).map_err(|_|CoreError::Limit)?;
        if count.saturating_add(count_add)>i64::from(snapshot.limits.max_tool_attempts) {return Err(CoreError::Limit);}
        let conversation=ConversationId(uuid(conversation)?);
        let envelope=MessageId::new();
        // Persist the exact envelope with the attempt, not just a reusable provider ID.
        let message=StoredMessage {role:"assistant".into(),content:text,calls:calls.iter().map(|call| {let mut call=StoredCall::from(call);call.source_envelope=Some(envelope);call}).collect(),call_id:None,source_envelope:None};
        insert_message(&tx,conversation,revision,envelope,&message)?;
        let mut ids=Vec::new();
        for call in message.calls {
            let attempt=AttemptId::new();
            tx.execute("INSERT INTO attempts(id,run_id,state,call_json) VALUES(?1,?2,'Proposed',?3)",params![attempt.0.to_string(),run.0.to_string(),json(&call)?]).map_err(sql_error)?;
            event(&tx,conversation,&format!("attempt:{}:Proposed",attempt.0))?;
            ids.push(attempt);
        }
        tx.execute("UPDATE runs SET partial_text='',attempt_count=attempt_count+?1,active_ms=active_ms+?2 WHERE id=?3",params![count_add,i64::try_from(active_ms).map_err(|_|CoreError::Limit)?,run.0.to_string()]).map_err(sql_error)?;
        event(&tx,conversation,&format!("run:{}:assistant",run.0))?;
        tx.commit().map_err(sql_error)?;
        Ok(ids)
    })
}
pub fn result(
    repo: &Repository,
    run: RunId,
    attempt: AttemptId,
    to: AttemptState,
    content: Option<String>,
    active_ms: u64,
) -> CoreResult<()> {
    repo.with_connection(move |conn| {
        let tx = conn.transaction().map_err(sql_error)?;
        result_on(&tx, run, attempt, to, content, active_ms)?;
        tx.commit().map_err(sql_error)?;
        Ok(())
    })
}
fn result_on(
    tx: &rusqlite::Transaction<'_>,
    run: RunId,
    attempt: AttemptId,
    to: AttemptState,
    content: Option<String>,
    active_ms: u64,
) -> CoreResult<()> {
    let (conversation,revision,status,call):(String,i64,String,String)=tx.query_row("SELECT r.conversation_id,r.snapshot_revision,a.state,a.call_json FROM attempts a JOIN runs r ON r.id=a.run_id WHERE a.id=?1 AND a.run_id=?2",params![attempt.0.to_string(),run.0.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(sql_error)?.ok_or(CoreError::Conflict)?;
    let valid = matches!(
        (status.as_str(), to),
        (
            "Dispatching",
            AttemptState::Succeeded | AttemptState::Failed | AttemptState::OutcomeUnknown
        ) | ("Proposed", AttemptState::Denied)
    );
    if !valid {
        return Err(CoreError::Conflict);
    }
    let conversation = ConversationId(uuid(conversation)?);
    tx.execute(
        "UPDATE attempts SET state=?1,result_json=?2 WHERE id=?3",
        params![format!("{to:?}"), content, attempt.0.to_string()],
    )
    .map_err(sql_error)?;
    if let Some(content) = content {
        let call: StoredCall = parse(&call)?;
        insert_message(
            tx,
            conversation,
            revision,
            MessageId::new(),
            &StoredMessage {
                role: "tool".into(),
                content,
                calls: vec![],
                call_id: Some(call.provider_call_id),
                source_envelope: call.source_envelope,
            },
        )?;
    }
    tx.execute(
        "UPDATE runs SET active_ms=active_ms+?1 WHERE id=?2",
        params![
            i64::try_from(active_ms).map_err(|_| CoreError::Limit)?,
            run.0.to_string()
        ],
    )
    .map_err(sql_error)?;
    event(tx, conversation, &format!("attempt:{}:{to:?}", attempt.0))?;
    Ok(())
}
/// The run owning an attempt and the attempt's state, if the attempt exists.
fn attempt_run_on(
    conn: &rusqlite::Connection,
    attempt: AttemptId,
) -> CoreResult<Option<(RunId, AttemptState)>> {
    conn.query_row(
        "SELECT run_id,state FROM attempts WHERE id=?1",
        [attempt.0.to_string()],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    )
    .optional()
    .map_err(sql_error)?
    .map(|(run, state)| Ok((RunId(uuid(run)?), attempt_state(&state)?)))
    .transpose()
}
/// Deny a proposed attempt inside the caller's transaction. Denying an attempt
/// that is already denied changes nothing; returns whether anything changed.
fn deny_on(tx: &rusqlite::Transaction<'_>, run: RunId, attempt: AttemptId) -> CoreResult<bool> {
    match attempt_run_on(tx, attempt)? {
        Some((owner, AttemptState::Denied)) if owner == run => Ok(false),
        Some((owner, _)) if owner == run => {
            result_on(
                tx,
                run,
                attempt,
                AttemptState::Denied,
                Some("Tool denied by user".into()),
                0,
            )?;
            Ok(true)
        }
        _ => Err(CoreError::Conflict),
    }
}
pub fn deny(repo: &Repository, attempt: AttemptId) -> CoreResult<RunId> {
    repo.with_connection(move |conn| {
        let tx = conn.transaction().map_err(sql_error)?;
        let (run, _) = attempt_run_on(&tx, attempt)?.ok_or(CoreError::Conflict)?;
        if deny_on(&tx, run, attempt)? {
            tx.commit().map_err(sql_error)?;
        }
        Ok(run)
    })
}
/// The outcome of storing several approval decisions for one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decided {
    /// False when every decision was already in effect.
    pub changed: bool,
    /// The run's state after the decisions.
    pub run_state: RunState,
}
fn check_decisions(approve: &[ApprovalBinding], deny: &[AttemptId]) -> CoreResult<()> {
    let mut seen = std::collections::HashSet::new();
    let ids = approve
        .iter()
        .map(|b| b.attempt_id)
        .chain(deny.iter().copied());
    if approve.is_empty() && deny.is_empty() || !ids.into_iter().all(|id| seen.insert(id)) {
        return Err(CoreError::InvalidInput);
    }
    Ok(())
}
fn run_state_on(conn: &rusqlite::Connection, run: RunId) -> CoreResult<RunState> {
    let status: String = conn
        .query_row(
            "SELECT state FROM runs WHERE id=?1",
            [run.0.to_string()],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql_error)?
        .ok_or(CoreError::Conflict)?;
    state(&status)
}
/// A run's current state.
pub fn run_state(repo: &Repository, run: RunId) -> CoreResult<RunState> {
    repo.with_connection(move |conn| run_state_on(conn, run))
}
fn owned_by(conn: &rusqlite::Connection, run: RunId, attempt: AttemptId) -> CoreResult<()> {
    match attempt_run_on(conn, attempt)? {
        Some((owner, _)) if owner == run => Ok(()),
        _ => Err(CoreError::Conflict),
    }
}
/// Approve and deny attempts of one run in one transaction: any stale binding
/// or foreign attempt fails the whole set and changes nothing. Approving MCP
/// calls needs their definitions observed by a current discovery.
pub fn decide(
    repo: &Repository,
    run: RunId,
    approve: Vec<ApprovalBinding>,
    deny: Vec<AttemptId>,
) -> CoreResult<Decided> {
    check_decisions(&approve, &deny)?;
    repo.with_connection(move |conn| {
        let tx = conn.transaction().map_err(sql_error)?;
        let run_state = run_state_on(&tx, run)?;
        let mut changed = false;
        for binding in &approve {
            owned_by(&tx, run, binding.attempt_id)?;
            changed |= crate::permissions::approve_on(&tx, binding)?;
        }
        for attempt in &deny {
            changed |= deny_on(&tx, run, *attempt)?;
        }
        if changed {
            tx.commit().map_err(sql_error)?;
        }
        Ok(Decided { changed, run_state })
    })
}
/// Whether every decision is already in effect, so no discovery is needed.
pub fn already_decided(
    repo: &Repository,
    run: RunId,
    approve: Vec<ApprovalBinding>,
    deny: Vec<AttemptId>,
) -> CoreResult<bool> {
    check_decisions(&approve, &deny)?;
    repo.with_connection(move |conn| {
        for binding in &approve {
            owned_by(conn, run, binding.attempt_id)?;
            if !crate::permissions::already_approved_on(conn, binding)? {
                return Ok(false);
            }
        }
        for attempt in &deny {
            match attempt_run_on(conn, *attempt)? {
                Some((owner, AttemptState::Denied)) if owner == run => {}
                Some((owner, _)) if owner == run => return Ok(false),
                _ => return Err(CoreError::Conflict),
            }
        }
        Ok(true)
    })
}
pub fn events(repo: &Repository, after: u64) -> CoreResult<Vec<(u64, ConversationId, String)>> {
    repo.with_connection(move |conn| {
        let mut stmt = conn
            .prepare("SELECT id,conversation_id,payload FROM events WHERE id>?1 ORDER BY id")
            .map_err(sql_error)?;
        let events = stmt
            .query_map([i64::try_from(after).map_err(|_| CoreError::Limit)?], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(sql_error)?
            .map(|row| {
                let (seq, conversation, payload) = row.map_err(sql_error)?;
                Ok((
                    u64::try_from(seq).map_err(|_| CoreError::Storage)?,
                    ConversationId(uuid(conversation)?),
                    payload,
                ))
            })
            .collect();
        events
    })
}
pub fn snapshot(repo: &Repository) -> CoreResult<Vec<RunData>> {
    let ids: Vec<RunId> = repo.with_connection(|conn| {
        let mut stmt = conn
            .prepare("SELECT id FROM runs ORDER BY rowid")
            .map_err(sql_error)?;
        let ids = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(sql_error)?
            .map(|r| Ok(RunId(uuid(r.map_err(sql_error)?)?)))
            .collect();
        ids
    })?;
    ids.into_iter().map(|id| load(repo, id)).collect()
}
