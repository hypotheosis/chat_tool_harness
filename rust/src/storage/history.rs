use super::*;
#[derive(Debug, Clone)]
pub struct ConversationSummary {
    pub id: ConversationId,
    pub title: String,
    pub has_unknown_outcomes: bool,
}
impl Repository {
    pub fn search_conversations(
        &self,
        query: &str,
        offset: u32,
        limit: u32,
    ) -> CoreResult<Vec<ConversationSummary>> {
        let query = query.to_owned();
        self.with_connection(move |conn| {
            // instr is literal (no LIKE wildcards), including quotes, %, _, and UTF-8.
            let mut stmt = conn.prepare("SELECT c.id,c.title,EXISTS(SELECT 1 FROM attempts a JOIN runs r ON r.id=a.run_id WHERE r.conversation_id=c.id AND a.state IN ('OutcomeUnknown','Dispatching')) FROM conversations c WHERE instr(c.title,?1)>0 OR EXISTS(SELECT 1 FROM messages m WHERE m.conversation_id=c.id AND instr(json_extract(m.payload,'$.content'),?1)>0) ORDER BY c.rowid LIMIT ?2 OFFSET ?3").map_err(|_|CoreError::Storage)?;
            let rows = stmt.query_map(params![query,limit.min(100),offset], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,bool>(2)?))).map_err(|_|CoreError::Storage)?;
            rows.map(|r| { let (value,title,has_unknown_outcomes)=r.map_err(|_|CoreError::Storage)?; Ok(ConversationSummary{id:ConversationId(parse(&value)?),title,has_unknown_outcomes}) }).collect()
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
}
