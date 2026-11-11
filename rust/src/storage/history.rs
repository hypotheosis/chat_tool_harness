use super::*;
#[derive(Debug, Clone)]
pub struct ConversationSummary {
    pub id: ConversationId,
    pub title: String,
    pub has_unknown_outcomes: bool,
    pub archived: bool,
}
impl Repository {
    /// Conversations whose title or message text contains `query`, archived
    /// ones included and marked; `archived_only` keeps just the archived ones.
    pub fn search_conversations(
        &self,
        query: &str,
        archived_only: bool,
        offset: u32,
        limit: u32,
    ) -> CoreResult<Vec<ConversationSummary>> {
        let query = query.to_owned();
        self.with_connection(move |conn| {
            // instr is literal (no LIKE wildcards), including quotes, %, _, and UTF-8.
            let mut stmt = conn.prepare("SELECT c.id,c.title,EXISTS(SELECT 1 FROM attempts a JOIN runs r ON r.id=a.run_id WHERE r.conversation_id=c.id AND a.state IN ('OutcomeUnknown','Dispatching')),c.archived FROM conversations c WHERE (?4=0 OR c.archived=1) AND (instr(c.title,?1)>0 OR EXISTS(SELECT 1 FROM messages m WHERE m.conversation_id=c.id AND instr(json_extract(m.payload,'$.content'),?1)>0)) ORDER BY c.rowid LIMIT ?2 OFFSET ?3").map_err(|_|CoreError::Storage)?;
            let rows = stmt.query_map(params![query,limit.min(100),offset,archived_only], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,bool>(2)?,r.get::<_,bool>(3)?))).map_err(|_|CoreError::Storage)?;
            rows.map(|r| { let (value,title,has_unknown_outcomes,archived)=r.map_err(|_|CoreError::Storage)?; Ok(ConversationSummary{id:ConversationId(parse(&value)?),title,has_unknown_outcomes,archived}) }).collect()
        })
    }
    pub fn rename_conversation(&self, owner: ConversationId, title: &str) -> CoreResult<()> {
        if title.trim().is_empty() || title.len() > 256 || title.chars().any(char::is_control) {
            return Err(CoreError::InvalidInput);
        }
        let title = title.to_owned();
        self.with_connection(move |conn| {
            let tx = conn.transaction().map_err(|_| CoreError::Storage)?;
            if tx
                .execute(
                    "UPDATE conversations SET title=?1 WHERE id=?2",
                    params![title, id(owner)],
                )
                .map_err(|_| CoreError::Storage)?
                != 1
            {
                return Err(CoreError::Conflict);
            }
            tx.execute(
                "INSERT INTO events(conversation_id,payload) VALUES(?1,'conversation-renamed')",
                [id(owner)],
            )
            .map_err(|_| CoreError::Storage)?;
            tx.commit().map_err(|_| CoreError::Storage)
        })
    }
    /// Archives (`true`) or restores (`false`) a conversation, keeping all of
    /// its history. Archiving is refused, as deletion is, while one of its runs
    /// is Running; the coordinator also refuses while its execution is still
    /// held. Setting the state it already has changes nothing.
    pub fn set_conversation_archived(
        &self,
        owner: ConversationId,
        archived: bool,
    ) -> CoreResult<()> {
        self.with_connection(move |conn| {
            let tx = conn
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|_| CoreError::Storage)?;
            let current: bool = tx
                .query_row(
                    "SELECT archived FROM conversations WHERE id=?1",
                    [id(owner)],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|_| CoreError::Storage)?
                .ok_or(CoreError::Conflict)?;
            if current == archived {
                return Ok(());
            }
            if archived {
                let active: bool = tx
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM runs WHERE conversation_id=?1 AND state='Running')",
                        [id(owner)],
                        |r| r.get(0),
                    )
                    .map_err(|_| CoreError::Storage)?;
                if active {
                    return Err(CoreError::Conflict);
                }
            }
            tx.execute(
                "UPDATE conversations SET archived=?1 WHERE id=?2",
                params![archived, id(owner)],
            )
            .map_err(|_| CoreError::Storage)?;
            tx.execute(
                "INSERT INTO events(conversation_id,payload) VALUES(?1,?2)",
                params![
                    id(owner),
                    if archived {
                        "conversation-archived"
                    } else {
                        "conversation-restored"
                    }
                ],
            )
            .map_err(|_| CoreError::Storage)?;
            tx.commit().map_err(|_| CoreError::Storage)
        })
    }
}
