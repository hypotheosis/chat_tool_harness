use crate::{
    error::{CoreError, CoreResult},
    files::artifacts::{ArtifactStore, ArtifactSummary, MAX_TEXT_BYTES},
    model::core::*,
    net::{CredentialProvider, SecretValue},
    storage::Repository,
};
use rusqlite::OptionalExtension;

#[derive(Debug, Default, Clone, Copy)]
pub struct ExportOptions {
    pub include_tool_payloads: bool,
}
pub struct ExportBundle {
    pub transcript: ArtifactSummary,
}
struct ExportInput {
    title: String,
    messages: Vec<crate::agent::context::StoredMessage>,
    partials: Vec<(String, String, Option<String>)>,
    attempts: Vec<(String, Option<String>, Option<String>)>,
    summaries: Vec<crate::agent::context::DerivedContext>,
    references: Vec<CredentialRef>,
    exclusions: Vec<String>,
}
fn storage(_: impl std::fmt::Debug) -> CoreError {
    CoreError::Storage
}
fn input(repo: &Repository, owner: ConversationId) -> CoreResult<ExportInput> {
    repo.with_connection(move |conn| {
        let tx=conn.transaction().map_err(storage)?;
        let owner=owner.0.to_string();
        let title=tx.query_row("SELECT title FROM conversations WHERE id=?1",[&owner],|r|r.get(0)).optional().map_err(storage)?.ok_or(CoreError::Conflict)?;
        fn values<T:serde::de::DeserializeOwned>(conn:&rusqlite::Connection,sql:&str,owner:&str)->CoreResult<Vec<T>> {
            let mut stmt=conn.prepare(sql).map_err(storage)?;
            let rows=stmt.query_map([owner],|r|r.get::<_,String>(0)).map_err(storage)?;
            rows.map(|r|serde_json::from_str(&r.map_err(storage)?).map_err(storage)).collect()
        }
        let messages=values(&tx,"SELECT payload FROM messages WHERE conversation_id=?1 ORDER BY ordinal",&owner)?;
        let summaries=values(&tx,"SELECT payload FROM derived_context WHERE conversation_id=?1 ORDER BY rowid",&owner)?;
        let mut stmt=tx.prepare("SELECT state,partial_text,failure FROM runs WHERE conversation_id=?1 ORDER BY rowid").map_err(storage)?;
        let partials=stmt.query_map([&owner],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(storage)?.collect::<Result<Vec<_>,_>>().map_err(storage)?;
        let mut stmt=tx.prepare("SELECT a.state,a.call_json,a.result_json FROM attempts a JOIN runs r ON r.id=a.run_id WHERE r.conversation_id=?1 ORDER BY r.rowid,a.rowid").map_err(storage)?;
        let attempts=stmt.query_map([&owner],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(storage)?.collect::<Result<Vec<_>,_>>().map_err(storage)?;
        let snapshots:Vec<ProfileSnapshot>=values(&tx,"SELECT payload FROM conversation_snapshots WHERE conversation_id=?1 ORDER BY revision",&owner)?;
        let endpoints:Vec<EndpointConfig>=values(&tx,"SELECT payload FROM endpoints WHERE ?1 IS NOT NULL",&owner)?;
        let servers:Vec<ServerConfig>=values(&tx,"SELECT payload FROM servers WHERE ?1 IS NOT NULL",&owner)?;
        let mut references=Vec::new();
        let mut exclusions=Vec::new();
        let mut endpoint = |e:EndpointConfig| { if let Some(r)=e.credential {references.push(r);} exclusions.push(e.base_url); };
        for e in endpoints { endpoint(e); }
        for snapshot in &snapshots { endpoint(snapshot.endpoint.clone()); }
        for server in servers.into_iter().chain(snapshots.into_iter().flat_map(|s|s.servers)) {
            references.extend(server.secret_headers.into_iter().map(|(_,r)|r));
            exclusions.push(server.url);
        }
        references.sort_by_key(|r|r.0); references.dedup();
        exclusions.extend(references.iter().map(|r|r.0.to_string()));
        Ok(ExportInput{title,messages,partials,attempts,summaries,references,exclusions})
    })
}

pub async fn export_transcript<P: CredentialProvider>(
    repo: &Repository,
    store: &ArtifactStore,
    owner: ConversationId,
    options: ExportOptions,
    credentials: &P,
) -> CoreResult<ExportBundle> {
    let repo = repo.clone();
    let mut input = tokio::task::spawn_blocking(move || input(&repo, owner))
        .await
        .map_err(storage)??;
    // References are resolved only for this user action. Neither values nor lookup errors are logged.
    let mut secrets: Vec<SecretValue> = Vec::new();
    let deadline = tokio::time::Instant::now() + crate::net::CONNECT_TIMEOUT;
    for reference in &input.references {
        let value = tokio::time::timeout_at(deadline, credentials.resolve(*reference))
            .await
            .map_err(|_| CoreError::Authentication)?
            .map_err(|_| CoreError::Authentication)?;
        if value.as_str().is_empty() {
            return Err(CoreError::Authentication);
        }
        secrets.push(value);
    }
    input.exclusions.extend(store.private_locations());
    let mut redactions: Vec<&str> = secrets
        .iter()
        .map(|s| s.as_str())
        .chain(input.exclusions.iter().map(String::as_str))
        .filter(|s| !s.is_empty())
        .collect();
    redactions.sort_by_key(|s| std::cmp::Reverse(s.len()));
    let mut redactor = Redactor::new(&redactions)?;
    let mut redact = |text: &str| redactor.text(text, 0);
    let mut text=String::from("Transcript — not a restorable backup.\nConfigured secrets are redacted; unknown or user-entered sensitive content and unrecognized encodings may remain. Review before Save/Share.\nTool summaries by default; detailed payloads require opt-in. Stop/deletion never rolls back remote effects.\n\n");
    text.push_str(if options.include_tool_payloads {
        "Export detail: DetailedToolPayloads (explicit opt-in; sensitive).\n\n"
    } else {
        "Export detail: ToolSummaries (arguments/results omitted).\n\n"
    });
    text.push_str(&format!("Title: {}\n\n", redact(&input.title)?));
    for message in &input.messages {
        match message.role.as_str() {
            "system" => continue,
            "tool" if !options.include_tool_payloads => {
                text.push_str("Tool result omitted (summary only).\n\n")
            }
            "tool" | "user" | "assistant" => text.push_str(&format!(
                "{}:\n{}\n\n",
                message.role,
                redact(&message.content)?
            )),
            _ => return Err(CoreError::Storage),
        }
        for call in &message.calls {
            text.push_str(&format!(
                "Tool request: {} (see outcome below)",
                redact(&call.key.name)?
            ));
            if options.include_tool_payloads {
                text.push_str(&format!(": {}", redact(&call.arguments_json)?));
            }
            text.push_str("\n\n");
        }
        if text.len() as u64 > MAX_TEXT_BYTES {
            return Err(CoreError::Limit);
        }
    }
    for (status, call, result) in &input.attempts {
        let call: Option<crate::agent::context::StoredCall> = call
            .as_ref()
            .map(|call| serde_json::from_str(call).map_err(storage))
            .transpose()?;
        let name = call
            .as_ref()
            .map(|c| redact(&c.key.name))
            .transpose()?
            .unwrap_or_else(|| "unrecorded tool".into());
        text.push_str(&format!(
            "Tool outcome: {name} — {status}{}\n",
            if status == "OutcomeUnknown" || status == "Dispatching" {
                " — unknown; no remote rollback claimed"
            } else {
                ""
            }
        ));
        if options.include_tool_payloads {
            if let Some(call) = call {
                text.push_str(&format!("Arguments: {}\n", redact(&call.arguments_json)?));
            }
            if let Some(result) = result {
                text.push_str(&format!("Result: {}\n", redact(result)?));
            }
        }
    }
    for (state, partial, failure) in &input.partials {
        text.push_str(&format!("Run status: {state}\n"));
        if !partial.is_empty() {
            text.push_str(&format!(
                "Partial assistant message (not completed):\n{}\n",
                redact(partial)?
            ));
        }
        if let Some(failure) = failure {
            text.push_str(&format!("Error: {}\n", redact(failure)?));
        }
    }
    for summary in &input.summaries {
        text.push_str(&format!(
            "Derived summary (not original messages; partial coverage: {}):\n{}\n",
            summary.partial_coverage,
            redact(&summary.text)?
        ));
    }
    if text.len() as u64 > MAX_TEXT_BYTES {
        return Err(CoreError::Limit);
    }
    let store = store.clone();
    let transcript =
        tokio::task::spawn_blocking(move || store.create(owner, "transcript.md", &text))
            .await
            .map_err(storage)??;
    Ok(ExportBundle { transcript })
}
// Shared across one export, including nested JSON carried inside MCP text strings.
const REDACTION_DEPTH: usize = 12;
const REDACTION_NODES: usize = 16_384;
const REDACTION_WORK: usize = 8 * 1024 * 1024;
const REDACTED: &str = "[REDACTED]";

struct Redactor<'a> {
    patterns: Vec<(&'a str, String)>,
    scalars: Vec<serde_json::Value>,
    nodes: usize,
    work: usize,
}
impl<'a> Redactor<'a> {
    fn new(secrets: &[&'a str]) -> CoreResult<Self> {
        let mut redactor = Self {
            patterns: Vec::new(),
            scalars: Vec::new(),
            nodes: REDACTION_NODES,
            work: REDACTION_WORK,
        };
        for &secret in secrets {
            redactor.spend(secret.len())?;
            let encoded = serde_json::to_string(secret).map_err(storage)?;
            redactor.spend(encoded.len())?;
            // Only scalar credentials have an additional non-string JSON representation.
            if matches!(
                secret.as_bytes().first(),
                Some(b'-' | b'0'..=b'9' | b't' | b'f' | b'n')
            ) {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(secret) {
                    if matches!(
                        value,
                        serde_json::Value::Number(_)
                            | serde_json::Value::Bool(_)
                            | serde_json::Value::Null
                    ) {
                        redactor.scalars.push(value);
                    }
                }
            }
            let escaped = encoded
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .ok_or(CoreError::Storage)?
                .to_owned();
            redactor.patterns.push((secret, escaped));
        }
        Ok(redactor)
    }
    fn spend(&mut self, amount: usize) -> CoreResult<()> {
        self.work = self.work.checked_sub(amount).ok_or(CoreError::Limit)?;
        Ok(())
    }
    fn plain(&mut self, text: &str) -> CoreResult<String> {
        self.spend(text.len())?;
        let mut result = text.to_owned();
        for index in 0..self.patterns.len() {
            for escaped in [false, true] {
                let length = if escaped {
                    self.patterns[index].1.len()
                } else {
                    self.patterns[index].0.len()
                };
                self.spend(result.len().saturating_add(length))?;
                let needle = if escaped {
                    self.patterns[index].1.as_str()
                } else {
                    self.patterns[index].0
                };
                result = result.replace(needle, REDACTED);
                self.spend(result.len())?;
            }
        }
        Ok(result)
    }
    fn preflight(&self, text: &str, depth: usize) -> CoreResult<()> {
        if depth > REDACTION_DEPTH {
            return Err(CoreError::Limit);
        }
        if !matches!(
            text.trim_start().as_bytes().first(),
            Some(b'{' | b'[' | b'"')
        ) {
            return Ok(());
        }
        // Bound structure before serde allocates a tree. Quoted/escaped delimiters are data;
        // their JSON will be checked separately if a decoded string contains another document.
        let (mut quoted, mut escaped, mut nesting, mut tokens) = (false, false, 0usize, 1usize);
        for byte in text.bytes() {
            if quoted {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    quoted = false;
                }
            } else {
                match byte {
                    b'"' => {
                        quoted = true;
                        tokens += 1;
                    }
                    b'{' | b'[' => {
                        nesting += 1;
                        tokens += 1;
                        if depth.saturating_add(nesting) > REDACTION_DEPTH {
                            return Err(CoreError::Limit);
                        }
                    }
                    b'}' | b']' => nesting = nesting.saturating_sub(1),
                    b',' | b':' => tokens += 1,
                    _ => {}
                }
            }
            if tokens > self.nodes {
                return Err(CoreError::Limit);
            }
        }
        Ok(())
    }
    fn text(&mut self, text: &str, depth: usize) -> CoreResult<String> {
        self.spend(text.len())?;
        self.preflight(text, depth)?;
        match serde_json::from_str::<serde_json::Value>(text) {
            Ok(value) => {
                let value = self.walk(value, depth)?;
                let encoded = serde_json::to_string_pretty(&value).map_err(storage)?;
                self.spend(encoded.len())?;
                Ok(encoded)
            }
            Err(_) => self.plain(text),
        }
    }
    fn walk(&mut self, value: serde_json::Value, depth: usize) -> CoreResult<serde_json::Value> {
        if depth > REDACTION_DEPTH {
            return Err(CoreError::Limit);
        }
        self.nodes = self.nodes.checked_sub(1).ok_or(CoreError::Limit)?;
        use serde_json::Value;
        Ok(match value {
            Value::String(s) => Value::String(self.text(&s, depth + 1)?),
            Value::Array(a) => Value::Array(
                a.into_iter()
                    .map(|v| self.walk(v, depth + 1))
                    .collect::<CoreResult<_>>()?,
            ),
            Value::Object(o) => Value::Object(
                o.into_iter()
                    .map(|(k, v)| Ok((self.text(&k, depth + 1)?, self.walk(v, depth + 1)?)))
                    .collect::<CoreResult<_>>()?,
            ),
            scalar => {
                let representation = scalar.to_string();
                self.spend(representation.len().saturating_mul(self.scalars.len()))?;
                if self.scalars.contains(&scalar) {
                    Value::String(REDACTED.into())
                } else {
                    let redacted = self.plain(&representation)?;
                    if redacted == representation {
                        scalar
                    } else {
                        Value::String(redacted)
                    }
                }
            }
        })
    }
}
