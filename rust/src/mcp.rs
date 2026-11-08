use crate::error::CoreError;
use crate::model::core::{ProfileId, ServerConfig, ToolCall, ToolDefinition, ToolKey, ToolServer};
use crate::net::{
    next_chunk, read_error_body, Cancellation, CredentialProvider, NetClient,
    ERROR_BODY_READ_TIMEOUT,
};
use crate::permissions;
use crate::storage::Repository;
use reqwest::{header, Method, Response};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use tokio::task::JoinHandle;
use tokio::time::Instant;

pub const MAX_RPC_BYTES: usize = 1024 * 1024;
pub const MAX_SSE_FRAMES: usize = 128;
pub const MAX_DISCOVERY_PAGES: usize = 32;
pub const MAX_DISCOVERY_TOOLS: usize = 512;
pub const MAX_CURSOR_BYTES: usize = 4096;
const SUPPORTED_VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpError {
    InvalidInput,
    UnsupportedTransport,
    UnsupportedVersion,
    Authentication,
    Limit,
    Protocol,
    RemoteProtocol(i64),
    OutcomeUnknown,
    Unavailable,
    Cancelled,
    Storage,
    Conflict,
    ExpiredSession,
}
impl From<CoreError> for McpError {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::InvalidInput | CoreError::Configuration => Self::InvalidInput,
            CoreError::Authentication => Self::Authentication,
            CoreError::Limit => Self::Limit,
            CoreError::Cancelled => Self::Cancelled,
            CoreError::Storage => Self::Storage,
            CoreError::Conflict => Self::Conflict,
            _ => Self::Unavailable,
        }
    }
}
pub type McpResult<T> = Result<T, McpError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDiagnostic {
    pub name: String,
    pub reason: String,
}
#[derive(Debug)]
pub struct DiscoveryResult {
    pub tools: Vec<ToolDefinition>,
    pub diagnostics: Vec<ToolDiagnostic>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct UnsupportedContent {
    pub kind: String,
    pub metadata: Value,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutcome {
    pub is_error: bool,
    pub text: Vec<String>,
    pub structured_content: Option<Value>,
    pub unsupported_content: Vec<UnsupportedContent>,
}

pub struct McpClient<P> {
    net: Arc<NetClient<P>>,
}
impl<P: CredentialProvider + 'static> McpClient<P> {
    pub fn new(provider: P) -> McpResult<Self> {
        Ok(Self {
            net: Arc::new(NetClient::new(provider)?),
        })
    }
    pub async fn connect(
        self,
        profile: ProfileId,
        server: ServerConfig,
        repo: Repository,
        deadline: Instant,
        cancel: Cancellation,
    ) -> McpResult<McpSession<P>> {
        let mut session = McpSession {
            net: self.net,
            server,
            profile,
            repo,
            version: String::new(),
            session_id: None,
            next_id: 1,
            definitions: Vec::new(),
            stale: true,
            closed: false,
            monitor: None,
            #[cfg(test)]
            registry_hook: None,
            shared_stale: Arc::new(AtomicBool::new(true)),
            generation: Arc::new(AtomicU64::new(0)),
            trusted_generation: None,
        };
        session.initialize(deadline, &cancel).await?;
        Ok(session)
    }
}
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegistryMutation {
    Publish,
    Invalidate,
    SetRule,
}
#[cfg(test)]
pub(crate) type RegistryHook = Arc<dyn Fn(RegistryMutation) + Send + Sync>;

pub struct McpSession<P> {
    #[cfg(test)]
    pub(crate) registry_hook: Option<RegistryHook>,
    net: Arc<NetClient<P>>,
    server: ServerConfig,
    profile: ProfileId,
    repo: Repository,
    version: String,
    session_id: Option<String>,
    next_id: u64,
    definitions: Vec<ToolDefinition>,
    stale: bool,
    closed: bool,
    monitor: Option<(Cancellation, JoinHandle<McpResult<()>>)>,
    shared_stale: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
    trusted_generation: Option<u64>,
}
impl<P: CredentialProvider + 'static> McpSession<P> {
    pub fn profile_id(&self) -> ProfileId {
        self.profile
    }
    pub fn protocol_version(&self) -> &str {
        &self.version
    }
    fn id(&mut self) -> McpResult<u64> {
        let id = self.next_id;
        self.next_id = id.checked_add(1).ok_or(McpError::Limit)?;
        Ok(id)
    }
    async fn post(
        &self,
        payload: Value,
        deadline: Instant,
        cancel: &Cancellation,
        initialized: bool,
    ) -> McpResult<Response> {
        let body = serde_json::to_vec(&payload).map_err(|_| McpError::InvalidInput)?;
        let version = initialized.then_some(self.version.as_str());
        let session = initialized.then_some(self.session_id.as_deref()).flatten();
        self.net
            .mcp_request(
                &self.server,
                Method::POST,
                Some(body),
                version,
                session,
                deadline,
                cancel,
            )
            .await
            .map_err(Into::into)
    }
    async fn initialize(&mut self, deadline: Instant, cancel: &Cancellation) -> McpResult<()> {
        crate::failure::set_step(crate::failure::FailureStep::Initialize);
        let id = self.id()?;
        let request = json!({"jsonrpc":"2.0","id":id,"method":"initialize","params":{
            "protocolVersion":SUPPORTED_VERSIONS[0],"capabilities":{},"clientInfo":{"name":"chat_tool_harness","version":"0.1.0"}
        }});
        let mut response = self.post(request, deadline, cancel, false).await?;
        if !response.status().is_success() {
            let status = note_status(&self.server, &mut response, deadline, cancel).await?;
            if matches!(status, 400 | 404 | 405) {
                return Err(McpError::UnsupportedTransport);
            }
            return Err(classify_code(status, false));
        }
        let session = response
            .headers()
            .get("Mcp-Session-Id")
            .map(|v| {
                let value = v.to_str().map_err(|_| McpError::Protocol)?;
                if value.is_empty() || value.bytes().any(|byte| !(0x21..=0x7e).contains(&byte)) {
                    return Err(McpError::Protocol);
                }
                Ok(value.to_owned())
            })
            .transpose()?;
        let value = read_rpc(
            &mut response,
            id,
            deadline,
            cancel,
            &self.net,
            &self.server,
            None,
            None,
        )
        .await?;
        let version = value
            .get("protocolVersion")
            .and_then(Value::as_str)
            .ok_or(McpError::Protocol)?;
        if !SUPPORTED_VERSIONS.contains(&version) {
            return Err(McpError::UnsupportedVersion);
        }
        if !value.get("capabilities").is_some_and(Value::is_object)
            || !value.get("serverInfo").is_some_and(Value::is_object)
        {
            return Err(McpError::Protocol);
        }
        self.version = version.to_owned();
        self.session_id = session;
        let mut response = self
            .post(
                json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
                deadline,
                cancel,
                true,
            )
            .await?;
        if response.status().as_u16() != 202 {
            if !response.status().is_success() {
                let status = note_status(&self.server, &mut response, deadline, cancel).await?;
                return Err(classify_code(status, self.session_id.is_some()));
            }
            return Err(McpError::Protocol);
        }
        self.stale = true;
        self.shared_stale.store(true, Ordering::Release);
        Ok(())
    }
    pub async fn list_tools(
        &mut self,
        deadline: Instant,
        cancel: Cancellation,
    ) -> McpResult<DiscoveryResult> {
        if self.closed {
            return Err(McpError::Conflict);
        }
        self.invalidate().await?;
        let discovery_generation = self.generation.load(Ordering::Acquire);
        let mut raw = Vec::new();
        let mut cursor: Option<String> = None;
        let mut seen = HashSet::new();
        for _ in 0..MAX_DISCOVERY_PAGES {
            let id = self.id()?;
            let params = cursor
                .as_ref()
                .map(|v| json!({"cursor":v}))
                .unwrap_or_else(|| json!({}));
            crate::failure::set_step(crate::failure::FailureStep::ListTools);
            let mut response = self
                .post(
                    json!({"jsonrpc":"2.0","id":id,"method":"tools/list","params":params}),
                    deadline,
                    &cancel,
                    true,
                )
                .await?;
            if !response.status().is_success() {
                let status = response.status().as_u16();
                // Session state changes first so a body-read cancellation cannot leave a dead session trusted.
                let expired = status == 404 && self.session_id.is_some();
                if expired {
                    self.invalidate().await?;
                    self.session_id = None;
                }
                note_status(&self.server, &mut response, deadline, &cancel).await?;
                if expired {
                    self.initialize(deadline, &cancel).await?;
                    return Err(McpError::ExpiredSession);
                }
                return Err(classify_code(status, self.session_id.is_some()));
            }
            let (value, changed) = read_rpc_events(
                &mut response,
                id,
                deadline,
                &cancel,
                &self.net,
                &self.server,
                Some(&self.version),
                self.session_id.as_deref(),
            )
            .await?;
            if changed {
                return Err(McpError::Conflict);
            }
            let page = value
                .get("tools")
                .and_then(Value::as_array)
                .ok_or(McpError::Protocol)?;
            if raw.len() + page.len() > MAX_DISCOVERY_TOOLS {
                return Err(McpError::Limit);
            }
            raw.extend(page.iter().cloned());
            cursor = match value.get("nextCursor") {
                None | Some(Value::Null) => None,
                Some(Value::String(v))
                    if !v.is_empty() && v.len() <= MAX_CURSOR_BYTES && seen.insert(v.clone()) =>
                {
                    Some(v.clone())
                }
                _ => return Err(McpError::Protocol),
            };
            if cursor.is_none() {
                let mut diagnostics = Vec::new();
                let mut valid = Vec::new();
                let mut names = HashSet::new();
                for tool in raw {
                    let name = tool
                        .get("name")
                        .and_then(Value::as_str)
                        .ok_or(McpError::Protocol)?;
                    if name.is_empty() || name.len() > 128 || !names.insert(name.to_owned()) {
                        return Err(McpError::Protocol);
                    }
                    let schema = tool.get("inputSchema").ok_or(McpError::Protocol)?;
                    if !schema.is_object()
                        || permissions::validate_discovery_schema(&schema.to_string()).is_err()
                    {
                        diagnostics.push(ToolDiagnostic {
                            name: name.into(),
                            reason:
                                "unsupported input schema: restricted JSON Schema subset required"
                                    .into(),
                        });
                        continue;
                    }
                    valid.push((
                        ToolKey {
                            profile_id: self.profile,
                            server: ToolServer::Mcp(self.server.id),
                            name: name.into(),
                        },
                        schema.to_string(),
                    ));
                }
                let tools = {
                    let repo = &self.repo;
                    let repo = repo.clone();
                    let profile = self.profile;
                    let server = self.server.id;
                    let config = self.server.clone();
                    let generation = self.generation.clone();
                    #[cfg(test)]
                    let hook = self.registry_hook.clone();
                    tokio::task::spawn_blocking(move || {
                        #[cfg(test)]
                        if let Some(hook) = hook {
                            hook(RegistryMutation::Publish);
                        }
                        permissions::replace_mcp_discovery_if_current(
                            &repo,
                            profile,
                            server,
                            &config,
                            valid,
                            &generation,
                            discovery_generation,
                        )
                    })
                    .await
                    .map_err(|_| McpError::Storage)??
                };
                if self.generation.load(Ordering::Acquire) != discovery_generation {
                    self.invalidate().await?;
                    return Err(McpError::Conflict);
                }
                self.definitions = tools.clone();
                self.stale = false;
                self.trusted_generation = Some(discovery_generation);
                self.shared_stale.store(false, Ordering::Release);
                if self.generation.load(Ordering::Acquire) != discovery_generation {
                    self.invalidate().await?;
                    return Err(McpError::Conflict);
                }
                return Ok(DiscoveryResult { tools, diagnostics });
            }
        }
        Err(McpError::Limit)
    }
    async fn invalidate(&mut self) -> McpResult<()> {
        self.stale = true;
        self.trusted_generation = None;
        self.shared_stale.store(true, Ordering::Release);
        self.definitions.clear();
        {
            let repo = &self.repo;
            let repo = repo.clone();
            let profile = self.profile;
            let server = self.server.id;
            #[cfg(test)]
            let hook = self.registry_hook.clone();
            tokio::task::spawn_blocking(move || {
                #[cfg(test)]
                if let Some(hook) = hook {
                    hook(RegistryMutation::Invalidate);
                }
                permissions::invalidate_mcp_discovery(&repo, profile, server)
            })
            .await
            .map_err(|_| McpError::Storage)??;
        }
        Ok(())
    }
    pub async fn call_tool(
        &mut self,
        call: &ToolCall,
        deadline: Instant,
        cancel: Cancellation,
    ) -> McpResult<ToolOutcome> {
        if self.closed
            || self.stale
            || self.shared_stale.load(Ordering::Acquire)
            || self.trusted_generation != Some(self.generation.load(Ordering::Acquire))
            || call.key.profile_id != self.profile
            || call.key.server != ToolServer::Mcp(self.server.id)
            || !self.definitions.iter().any(|d| d.key == call.key)
        {
            return Err(McpError::Conflict);
        }
        let repo = self.repo.clone();
        let profile = self.profile;
        let server_id = self.server.id;
        let config = self.server.clone();
        tokio::task::spawn_blocking(move || {
            permissions::require_mcp_destination(&repo, profile, server_id, &config)
        })
        .await
        .map_err(|_| McpError::Storage)??;
        let arguments =
            permissions::parse_json(&call.arguments_json).map_err(|_| McpError::InvalidInput)?;
        if !arguments.is_object() {
            return Err(McpError::InvalidInput);
        }
        if cancel.is_cancelled() {
            return Err(McpError::Cancelled);
        }
        let id = self.id()?;
        crate::failure::set_step(crate::failure::FailureStep::CallTool);
        let response = self.post(json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":call.key.name,"arguments":arguments}}), deadline, &cancel, true).await;
        let mut response = match response {
            Ok(v) => v,
            Err(McpError::Cancelled | McpError::Unavailable) => {
                return Err(McpError::OutcomeUnknown)
            }
            Err(e) => return Err(e),
        };
        if !response.status().is_success() {
            // The request may have reached the server, so every failure here is an unknown outcome.
            let status = match note_status(&self.server, &mut response, deadline, &cancel).await {
                Ok(status) => status,
                Err(_) => response.status().as_u16(),
            };
            if status == 404 && self.session_id.is_some() {
                self.invalidate().await?;
                self.session_id = None;
            }
            return Err(McpError::OutcomeUnknown);
        }
        let (value, list_changed) = read_rpc_events(
            &mut response,
            id,
            deadline,
            &cancel,
            &self.net,
            &self.server,
            Some(&self.version),
            self.session_id.as_deref(),
        )
        .await
        .map_err(|error| {
            if matches!(error, McpError::RemoteProtocol(_)) {
                error
            } else {
                McpError::OutcomeUnknown
            }
        })?;
        if list_changed {
            let _ = self.invalidate().await;
        }
        let content = value
            .get("content")
            .and_then(Value::as_array)
            .ok_or(McpError::OutcomeUnknown)?;
        let mut text = Vec::new();
        let mut unsupported_content = Vec::new();
        for item in content {
            let kind = item
                .get("type")
                .and_then(Value::as_str)
                .ok_or(McpError::OutcomeUnknown)?;
            if kind == "text" {
                text.push(
                    item.get("text")
                        .and_then(Value::as_str)
                        .ok_or(McpError::OutcomeUnknown)?
                        .to_owned(),
                );
            } else {
                let mut metadata = item.clone();
                if let Some(obj) = metadata.as_object_mut() {
                    obj.remove("data");
                }
                unsupported_content.push(UnsupportedContent {
                    kind: kind.into(),
                    metadata,
                });
            }
        }
        let is_error = value
            .get("isError")
            .map(Value::as_bool)
            .unwrap_or(Some(false))
            .ok_or(McpError::OutcomeUnknown)?;
        Ok(ToolOutcome {
            is_error,
            text,
            structured_content: value.get("structuredContent").cloned(),
            unsupported_content,
        })
    }
    /// Process one optional GET notification stream until disconnect or cancellation.
    /// A 405 means this server has no GET stream; no background task is retained.
    pub async fn listen_notifications(
        &mut self,
        deadline: Instant,
        cancel: Cancellation,
    ) -> McpResult<()> {
        if self.closed {
            return Err(McpError::Conflict);
        }
        let response = self
            .net
            .mcp_request(
                &self.server,
                Method::GET,
                None,
                Some(&self.version),
                self.session_id.as_deref(),
                deadline,
                &cancel,
            )
            .await?;
        if response.status().as_u16() == 405 {
            return Ok(());
        }
        classify_status(&response, self.session_id.is_some())?;
        let mut response = response;
        read_notification_events(
            &mut response,
            deadline,
            &cancel,
            &self.net,
            &self.server,
            &self.version,
            self.session_id.as_deref(),
            &mut self.stale,
            Some((self.repo.clone(), self.profile, self.server.id)),
            Some(&self.shared_stale),
            Some(&self.generation),
        )
        .await?;
        if self.stale {
            self.definitions.clear();
        }
        Ok(())
    }
    /// Own the optional GET listener independently of subsequent calls and close.
    pub async fn start_notifications(&mut self, deadline: Instant) -> McpResult<()> {
        if self.closed || self.monitor.is_some() {
            return Err(McpError::Conflict);
        }
        let net = self.net.clone();
        let server = self.server.clone();
        let version = self.version.clone();
        let session_id = self.session_id.clone();
        let repo = self.repo.clone();
        let profile = self.profile;
        let stale = self.shared_stale.clone();
        let generation = self.generation.clone();
        let cancel = Cancellation::new();
        let task_cancel = cancel.clone();
        let handle = tokio::spawn(async move {
            let response = net
                .mcp_request(
                    &server,
                    Method::GET,
                    None,
                    Some(&version),
                    session_id.as_deref(),
                    deadline,
                    &task_cancel,
                )
                .await?;
            if response.status().as_u16() == 405 {
                return Ok(());
            }
            classify_status(&response, session_id.is_some())?;
            let mut response = response;
            let mut changed = false;
            read_notification_events(
                &mut response,
                deadline,
                &task_cancel,
                &net,
                &server,
                &version,
                session_id.as_deref(),
                &mut changed,
                Some((repo, profile, server.id)),
                Some(&stale),
                Some(&generation),
            )
            .await?;
            if changed {
                stale.store(true, Ordering::Release);
            }
            Ok(())
        });
        self.monitor = Some((cancel, handle));
        Ok(())
    }
    pub async fn close(&mut self, deadline: Instant, cancel: Cancellation) -> McpResult<()> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        let mut monitor_timeout = false;
        if let Some((monitor_cancel, mut handle)) = self.monitor.take() {
            monitor_cancel.cancel();
            if deadline <= Instant::now()
                || tokio::time::timeout_at(deadline, &mut handle)
                    .await
                    .is_err()
            {
                handle.abort();
                let _ = handle.await;
                monitor_timeout = true;
            }
        }
        self.invalidate().await?;
        if monitor_timeout {
            return Err(McpError::Cancelled);
        }
        if self.session_id.is_some() {
            // Teardown failures must not overwrite the caller's primary failure note.
            let (result, _) = crate::failure::scope(async {
                let response = self
                    .net
                    .mcp_request(
                        &self.server,
                        Method::DELETE,
                        None,
                        Some(&self.version),
                        self.session_id.as_deref(),
                        deadline,
                        &cancel,
                    )
                    .await?;
                if !matches!(response.status().as_u16(), 200..=299 | 405) {
                    classify_status(&response, true)?;
                }
                McpResult::Ok(())
            })
            .await;
            result?;
        }
        Ok(())
    }
}
impl<P> Drop for McpSession<P> {
    fn drop(&mut self) {
        if let Some((cancel, handle)) = self.monitor.take() {
            cancel.cancel();
            handle.abort();
        }
    }
}
fn classify_status(response: &Response, session: bool) -> McpResult<()> {
    match response.status().as_u16() {
        200..=299 => Ok(()),
        status => Err(classify_code(status, session)),
    }
}
/// Error for a non-success status.
fn classify_code(status: u16, session: bool) -> McpError {
    match status {
        401 | 403 => McpError::Authentication,
        404 if session => McpError::ExpiredSession,
        _ => McpError::Protocol,
    }
}
/// Reads a bounded slice of a non-success response body, records the failure
/// note for the current step and returns the status. Only caller cancellation
/// or the caller's deadline turns into an error; a stalled body is ignored.
async fn note_status(
    server: &ServerConfig,
    response: &mut Response,
    deadline: Instant,
    cancel: &Cancellation,
) -> McpResult<u16> {
    let status = response.status().as_u16();
    let url = crate::failure::display_url(response.url());
    let (body, _) = read_error_body(response, deadline, cancel, ERROR_BODY_READ_TIMEOUT).await?;
    crate::failure::note(crate::failure::FailureDetail {
        target: crate::failure::FailureTarget::McpServer {
            name: server.name.clone(),
        },
        step: crate::failure::FailureStep::Request,
        url: Some(url),
        status: Some(status),
        reason: crate::failure::reason_from_body(&body, &[]),
    });
    Ok(status)
}
fn event_delimiter(bytes: &[u8]) -> Option<(usize, usize)> {
    for i in 0..bytes.len() {
        if bytes[i..].starts_with(b"\r\n\r\n") {
            return Some((i, 4));
        }
        if bytes[i..].starts_with(b"\n\n") {
            return Some((i, 2));
        }
        if bytes[i..].starts_with(b"\r\r") {
            return Some((i, 2));
        }
    }
    None
}

#[allow(clippy::too_many_arguments)] // The stream owns transport, registry and cancellation contexts.
async fn read_notification_events<P: CredentialProvider>(
    response: &mut Response,
    deadline: Instant,
    cancel: &Cancellation,
    net: &NetClient<P>,
    server: &ServerConfig,
    version: &str,
    session: Option<&str>,
    stale: &mut bool,
    registry: Option<(Repository, ProfileId, crate::model::core::ServerId)>,
    shared_stale: Option<&AtomicBool>,
    generation: Option<&AtomicU64>,
) -> McpResult<()> {
    if !response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/event-stream"))
    {
        return Err(McpError::Protocol);
    }
    let mut bytes = Vec::new();
    let mut pending = Vec::new();
    let mut frames = 0;
    loop {
        let chunk = next_chunk(response, deadline, cancel, MAX_RPC_BYTES - bytes.len()).await?;
        let Some(chunk) = chunk else {
            break;
        };
        bytes.extend_from_slice(&chunk);
        pending.extend_from_slice(&chunk);
        while let Some((pos, len)) = event_delimiter(&pending) {
            frames += 1;
            if frames > MAX_SSE_FRAMES {
                return Err(McpError::Limit);
            }
            let event: Vec<u8> = pending.drain(..pos + len).collect();
            let data = event
                .split(|b| *b == b'\n')
                .filter_map(|line| {
                    line.strip_suffix(b"\r")
                        .unwrap_or(line)
                        .strip_prefix(b"data:")
                })
                .map(|line| line.strip_prefix(b" ").unwrap_or(line))
                .collect::<Vec<_>>()
                .join(&b'\n');
            if data.is_empty() {
                continue;
            }
            let value: Value = serde_json::from_slice(&data).map_err(|_| McpError::Protocol)?;
            if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
                return Err(McpError::Protocol);
            }
            if value.get("method").and_then(Value::as_str)
                == Some("notifications/tools/list_changed")
            {
                *stale = true;
                if let Some(generation) = generation {
                    generation.fetch_add(1, Ordering::AcqRel);
                }
                if let Some(flag) = shared_stale {
                    flag.store(true, Ordering::Release);
                }
                if let Some((repo, profile, server_id)) = &registry {
                    let repo = repo.clone();
                    let profile = *profile;
                    let server_id = *server_id;
                    tokio::task::spawn_blocking(move || {
                        permissions::invalidate_mcp_discovery(&repo, profile, server_id)
                    })
                    .await
                    .map_err(|_| McpError::Storage)??;
                }
            } else if let Some(id) = value.get("id") {
                let error = json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}});
                let answer = net
                    .mcp_request(
                        server,
                        Method::POST,
                        Some(serde_json::to_vec(&error).map_err(|_| McpError::Protocol)?),
                        Some(version),
                        session,
                        deadline,
                        cancel,
                    )
                    .await?;
                if answer.status().as_u16() != 202 {
                    return Err(McpError::Protocol);
                }
            }
        }
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)] // One RPC response carries identity and transport contexts.
async fn read_rpc<P: CredentialProvider>(
    response: &mut Response,
    id: u64,
    deadline: Instant,
    cancel: &Cancellation,
    net: &NetClient<P>,
    server: &ServerConfig,
    version: Option<&str>,
    session: Option<&str>,
) -> McpResult<Value> {
    read_rpc_events(
        response, id, deadline, cancel, net, server, version, session,
    )
    .await
    .map(|(result, _)| result)
}
#[allow(clippy::too_many_arguments)] // One RPC response carries identity and transport contexts.
async fn read_rpc_events<P: CredentialProvider>(
    response: &mut Response,
    id: u64,
    deadline: Instant,
    cancel: &Cancellation,
    net: &NetClient<P>,
    server: &ServerConfig,
    version: Option<&str>,
    session: Option<&str>,
) -> McpResult<(Value, bool)> {
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let sse = content_type.starts_with("text/event-stream");
    if !sse && !content_type.starts_with("application/json") {
        return Err(McpError::Protocol);
    }
    let mut bytes = Vec::new();
    let mut frame = Vec::new();
    let mut frames = 0;
    let mut list_changed = false;
    loop {
        let chunk = next_chunk(response, deadline, cancel, MAX_RPC_BYTES - bytes.len()).await?;
        match chunk {
            Some(chunk) => {
                bytes.extend_from_slice(&chunk);
                if !sse {
                    continue;
                }
                frame.extend_from_slice(&chunk);
                while let Some((pos, len)) = event_delimiter(&frame) {
                    frames += 1;
                    if frames > MAX_SSE_FRAMES {
                        return Err(McpError::Limit);
                    }
                    let event: Vec<u8> = frame.drain(..pos + len).collect();
                    let mut data = Vec::new();
                    for line in event.split(|b| *b == b'\n') {
                        let line = line.strip_suffix(b"\r").unwrap_or(line);
                        if let Some(value) = line.strip_prefix(b"data:") {
                            if !data.is_empty() {
                                data.push(b'\n');
                            }
                            data.extend_from_slice(value.strip_prefix(b" ").unwrap_or(value));
                        }
                    }
                    if data.is_empty() {
                        continue;
                    }
                    let value: Value =
                        serde_json::from_slice(&data).map_err(|_| McpError::Protocol)?;
                    if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
                        return Err(McpError::Protocol);
                    }
                    if value.get("method").is_some() {
                        if value.get("method").and_then(Value::as_str)
                            == Some("notifications/tools/list_changed")
                        {
                            list_changed = true;
                        } else if let Some(request_id) = value.get("id") {
                            let error = json!({"jsonrpc":"2.0","id":request_id,"error":{"code":-32601,"message":"Method not found"}});
                            let response = net
                                .mcp_request(
                                    server,
                                    Method::POST,
                                    Some(
                                        serde_json::to_vec(&error)
                                            .map_err(|_| McpError::Protocol)?,
                                    ),
                                    version,
                                    session,
                                    deadline,
                                    cancel,
                                )
                                .await?;
                            if response.status().as_u16() != 202 {
                                return Err(McpError::Protocol);
                            }
                        }
                        continue;
                    }
                    return validate_rpc(value, id).map(|result| (result, list_changed));
                }
            }
            None => break,
        }
    }
    if sse {
        return Err(McpError::Unavailable);
    }
    let value = serde_json::from_slice(&bytes).map_err(|_| McpError::Protocol)?;
    validate_rpc(value, id).map(|result| (result, list_changed))
}
fn validate_rpc(value: Value, id: u64) -> McpResult<Value> {
    if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
        || value.get("id").and_then(Value::as_u64) != Some(id)
    {
        return Err(McpError::Protocol);
    }
    if let Some(error) = value.get("error") {
        if value.get("result").is_some() {
            return Err(McpError::Protocol);
        }
        return Err(McpError::RemoteProtocol(
            error
                .get("code")
                .and_then(Value::as_i64)
                .ok_or(McpError::Protocol)?,
        ));
    }
    value.get("result").cloned().ok_or(McpError::Protocol)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::core::*;
    use crate::net::{CredentialProvider, SecretValue};
    use std::{future::Future, pin::Pin, time::Duration};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        time::Instant,
    };

    struct NoKey;
    impl CredentialProvider for NoKey {
        fn resolve(
            &self,
            _: CredentialRef,
        ) -> Pin<Box<dyn Future<Output = crate::error::CoreResult<SecretValue>> + Send + '_>>
        {
            Box::pin(async { panic!("unexpected credential") })
        }
    }
    async fn fixture(
        responses: Vec<(u16, &'static str, String)>,
    ) -> (ServerConfig, tokio::task::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let mut requests = vec![];
            for (status, extra_headers, body) in responses {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buf = [0; 4096];
                loop {
                    let n = socket.read(&mut buf).await.unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&buf[..n]);
                    if let Some(pos) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&request[..pos]);
                        let len: usize = head
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|s| s.parse().ok())
                            })
                            .unwrap_or(0);
                        if request.len() >= pos + 4 + len {
                            break;
                        }
                    }
                }
                requests.push(String::from_utf8(request).unwrap());
                socket.write_all(format!("HTTP/1.1 {status} Fixture\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
            requests
        });
        (
            ServerConfig {
                id: ServerId::new(),
                name: "fixture".into(),
                url: format!("http://{addr}/exact/mcp"),
                allow_http: true,
                secret_headers: vec![],
            },
            task,
        )
    }
    fn reply(id: u64, result: &str) -> String {
        format!(r#"{{"jsonrpc":"2.0","id":{id},"result":{result}}}"#)
    }
    fn init() -> String {
        reply(
            1,
            r#"{"protocolVersion":"2025-11-25","capabilities":{"tools":{"listChanged":true}},"serverInfo":{"name":"fixture","version":"1"}}"#,
        )
    }
    fn list(id: u64, names: &[&str], cursor: Option<&str>) -> String {
        let tools: Vec<_> = names
            .iter()
            .map(|name| serde_json::json!({"name":name,"inputSchema":{"type":"object"}}))
            .collect();
        reply(
            id,
            &serde_json::json!({"tools":tools,"nextCursor":cursor}).to_string(),
        )
    }
    fn linked_repo(server: &ServerConfig) -> (Repository, tempfile::TempDir, ProfileId) {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::open(&dir.path().join("mcp.db")).unwrap();
        let endpoint = EndpointId::new();
        repo.save_endpoint(EndpointConfig {
            id: endpoint,
            name: "model".into(),
            base_url: "https://example.test/v1".into(),
            model: "model".into(),
            context_tokens: 100,
            max_output_tokens: 10,
            temperature: 0.1,
            allow_http: false,
            credential: None,
        })
        .unwrap();
        repo.save_server(server.clone()).unwrap();
        let profile = ProfileId(server.id.0);
        repo.save_profile(Profile {
            mode: crate::model::core::ProfileMode::Agent,
            id: profile,
            name: "profile".into(),
            endpoint_id: endpoint,
            server_ids: vec![server.id],
            instructions: String::new(),
            limits: RunLimits::default(),
        })
        .unwrap();
        (repo, dir, profile)
    }
    fn fixture_repository(server: &ServerConfig) -> Repository {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::open(&dir.path().join("fixture.db")).unwrap();
        // Keep the fixture directory alive until the repository worker shuts down.
        let _path = dir.keep();
        let endpoint = EndpointId::new();
        repo.save_endpoint(EndpointConfig {
            id: endpoint,
            name: "model".into(),
            base_url: "https://example.test/v1".into(),
            model: "model".into(),
            context_tokens: 100,
            max_output_tokens: 10,
            temperature: 0.1,
            allow_http: false,
            credential: None,
        })
        .unwrap();
        repo.save_server(server.clone()).unwrap();
        repo.save_profile(Profile {
            mode: crate::model::core::ProfileMode::Agent,
            id: ProfileId(server.id.0),
            name: "profile".into(),
            endpoint_id: endpoint,
            server_ids: vec![server.id],
            instructions: String::new(),
            limits: RunLimits::default(),
        })
        .unwrap();
        repo
    }
    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(5)
    }
    #[tokio::test]
    async fn initialize_then_discover_all_pages() {
        let (server, task) = fixture(vec![
            (
                200,
                "Content-Type: application/json\r\nMcp-Session-Id: synthetic-session\r\n",
                init(),
            ),
            (202, "", String::new()),
            (
                200,
                "Content-Type: application/json\r\n",
                list(2, &["alpha"], Some("next")),
            ),
            (
                200,
                "Content-Type: application/json\r\n",
                list(3, &["beta"], None),
            ),
        ])
        .await;
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                ProfileId(server.id.0),
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        let result = session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap();
        assert_eq!(result.tools.len(), 2);
        assert!(result.diagnostics.is_empty());
        let requests = task.await.unwrap();
        assert!(requests[0].starts_with("POST /exact/mcp"));
        assert!(requests[0].contains("\"capabilities\":{}"));
        assert!(!requests[0].contains("MCP-Session-Id:"));
        assert!(requests[1]
            .to_ascii_lowercase()
            .contains("mcp-session-id: synthetic-session"));
        assert!(requests[2]
            .to_ascii_lowercase()
            .contains("mcp-protocol-version: 2025-11-25"));
        assert!(requests[3].contains("\"cursor\":\"next\""));
    }
    #[tokio::test]
    async fn json_and_sse_tool_result() {
        let sse = format!(
            ": comment\n\ndata: {}\n\n",
            reply(
                4,
                r#"{"content":[{"type":"text","text":"ok"}],"structuredContent":{"n":1}}"#
            )
        );
        let (server, task) = fixture(vec![
            (200, "Content-Type: application/json\r\n", init()),
            (202, "", String::new()),
            (
                200,
                "Content-Type: application/json\r\n",
                list(2, &["alpha"], None),
            ),
            (
                200,
                "Content-Type: application/json\r\n",
                reply(3, r#"{"content":[{"type":"text","text":"json"}]}"#),
            ),
            (200, "Content-Type: text/event-stream\r\n", sse),
        ])
        .await;
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                ProfileId(server.id.0),
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap();
        let call = ToolCall {
            key: ToolKey {
                profile_id: session.profile_id(),
                server: ToolServer::Mcp(server.id),
                name: "alpha".into(),
            },
            provider_call_id: "call".into(),
            arguments_json: "{}".into(),
        };
        assert_eq!(
            session
                .call_tool(&call, deadline(), Cancellation::new())
                .await
                .unwrap()
                .text,
            vec!["json"]
        );
        let sse_result = session
            .call_tool(&call, deadline(), Cancellation::new())
            .await
            .unwrap();
        assert_eq!(sse_result.text, vec!["ok"]);
        assert_eq!(sse_result.structured_content.unwrap()["n"], 1);
        assert_eq!(task.await.unwrap().len(), 5);
    }
    #[tokio::test]
    async fn remote_error_is_distinct_from_protocol_and_unknown_outcome() {
        let (server, task) = fixture(vec![
            (200, "Content-Type: application/json\r\n", init()),
            (202, "", String::new()),
            (
                200,
                "Content-Type: application/json\r\n",
                list(2, &["alpha"], None),
            ),
            (
                200,
                "Content-Type: application/json\r\n",
                reply(
                    3,
                    r#"{"content":[{"type":"text","text":"business failure"}],"isError":true}"#,
                ),
            ),
            (
                200,
                "Content-Type: application/json\r\n",
                r#"{"jsonrpc":"2.0","id":4,"error":{"code":-32602,"message":"bad input"}}"#.into(),
            ),
        ])
        .await;
        let profile = ProfileId(server.id.0);
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                profile,
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap();
        let call = ToolCall {
            key: ToolKey {
                profile_id: profile,
                server: ToolServer::Mcp(server.id),
                name: "alpha".into(),
            },
            provider_call_id: "call".into(),
            arguments_json: "{}".into(),
        };
        let failure = session
            .call_tool(&call, deadline(), Cancellation::new())
            .await
            .unwrap();
        assert!(failure.is_error);
        assert_eq!(failure.text, vec!["business failure"]);
        assert_eq!(
            session
                .call_tool(&call, deadline(), Cancellation::new())
                .await,
            Err(McpError::RemoteProtocol(-32602))
        );
        task.await.unwrap();
    }
    #[tokio::test]
    async fn protocol_error_not_empty_success() {
        let (server, task) = fixture(vec![
            (200, "Content-Type: application/json\r\n", init()),
            (202, "", String::new()),
            (
                200,
                "Content-Type: application/json\r\n",
                reply(99, r#"{"tools":[]}"#),
            ),
        ])
        .await;
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                ProfileId(server.id.0),
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        assert!(matches!(
            session.list_tools(deadline(), Cancellation::new()).await,
            Err(McpError::Protocol)
        ));
        task.await.unwrap();
    }
    #[tokio::test]
    async fn expired_session_refreshes_discovery_without_replaying_call() {
        let (server, task) = fixture(vec![
            (200, "Content-Type: application/json\r\nMcp-Session-Id: old\r\n", init()), (202, "", String::new()),
            (200, "Content-Type: application/json\r\n", list(2, &["alpha"], None)),
            (404, "", String::new()),
            (200, "Content-Type: application/json\r\nMcp-Session-Id: renewed\r\n", reply(4, r#"{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"fixture","version":"1"}}"#)),
            (202, "", String::new()),
            (200, "Content-Type: application/json\r\n", list(5, &["beta"], None)),
        ]).await;
        let profile = ProfileId(server.id.0);
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                profile,
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap();
        let call = ToolCall {
            key: ToolKey {
                profile_id: profile,
                server: ToolServer::Mcp(server.id),
                name: "alpha".into(),
            },
            provider_call_id: "call".into(),
            arguments_json: "{}".into(),
        };
        assert!(matches!(
            session
                .call_tool(&call, deadline(), Cancellation::new())
                .await,
            Err(McpError::OutcomeUnknown)
        ));
        assert!(matches!(
            session
                .call_tool(&call, deadline(), Cancellation::new())
                .await,
            Err(McpError::Conflict)
        ));
        session
            .initialize(deadline(), &Cancellation::new())
            .await
            .unwrap();
        let refreshed = session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap();
        assert_eq!(refreshed.tools[0].key.name, "beta");
        let requests = task.await.unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|v| v.contains("\"method\":\"tools/call\""))
                .count(),
            1
        );
        assert!(!requests[4]
            .to_ascii_lowercase()
            .contains("mcp-session-id: old"));
    }
    #[tokio::test]
    async fn unsupported_sampling_rejected() {
        let request = r#"{"jsonrpc":"2.0","id":88,"method":"sampling/createMessage","params":{}}"#;
        let sse = format!(
            "data: {request}\n\ndata: {}\n\n",
            reply(2, r#"{"tools":[]}"#)
        );
        let (server, task) = fixture(vec![
            (200, "Content-Type: application/json\r\n", init()),
            (202, "", String::new()),
            (200, "Content-Type: text/event-stream\r\n", sse),
            (202, "", String::new()),
        ])
        .await;
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                ProfileId(server.id.0),
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap();
        let requests = task.await.unwrap();
        assert!(requests[3].contains("\"code\":-32601"));
        assert!(requests[3].contains("\"id\":88"));
    }
    #[tokio::test]
    async fn list_change_revokes_old_schema_trust() {
        let directory = tempfile::tempdir().unwrap();
        let repo = Repository::open(&directory.path().join("mcp.db")).unwrap();
        let profile = ProfileId::new();
        let endpoint = EndpointId::new();
        repo.save_endpoint(EndpointConfig {
            id: endpoint,
            name: "model".into(),
            base_url: "https://example.test/v1".into(),
            model: "model".into(),
            context_tokens: 100,
            max_output_tokens: 10,
            temperature: 0.1,
            allow_http: false,
            credential: None,
        })
        .unwrap();
        let (server, task) = fixture(vec![
            (200, "Content-Type: application/json\r\n", init()), (202, "", String::new()),
            (200, "Content-Type: application/json\r\n", list(2, &["alpha"], None)),
            (200, "Content-Type: text/event-stream\r\n", format!("data: {{\"jsonrpc\":\"2.0\",\"method\":\"notifications/tools/list_changed\"}}\n\ndata: {}\n\n", reply(3, r#"{"content":[{"type":"text","text":"ok"}]}"#))),
        ]).await;
        repo.save_server(server.clone()).unwrap();
        repo.save_profile(Profile {
            mode: crate::model::core::ProfileMode::Agent,
            id: profile,
            name: "profile".into(),
            endpoint_id: endpoint,
            server_ids: vec![server.id],
            instructions: String::new(),
            limits: RunLimits::default(),
        })
        .unwrap();
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                profile,
                server.clone(),
                repo.clone(),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        let definition = session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap()
            .tools
            .remove(0);
        let call = ToolCall {
            key: definition.key.clone(),
            provider_call_id: "call".into(),
            arguments_json: "{}".into(),
        };
        assert_eq!(
            session
                .call_tool(&call, deadline(), Cancellation::new())
                .await
                .unwrap()
                .text,
            vec!["ok"]
        );
        assert!(session.stale);
        assert_eq!(
            session
                .call_tool(&call, deadline(), Cancellation::new())
                .await,
            Err(McpError::Conflict)
        );
        let key = definition.key.clone();
        assert!(repo.with_connection(move |conn| {
            let (profile, server) = (key.profile_id.0.to_string(), format!("mcp:{}", server.id.0));
            use rusqlite::OptionalExtension;
            conn.query_row("SELECT definition_digest FROM observed_tool_definitions WHERE profile_id=?1 AND server_key=?2 AND tool_name=?3", rusqlite::params![profile,server,key.name], |row| row.get::<_, String>(0)).optional().map_err(|_| crate::error::CoreError::Storage)
        }).unwrap().is_none());
        task.await.unwrap();
    }
    #[tokio::test]
    async fn discovery_pagination_cycle_and_unsupported_tool_diagnostics() {
        let (server, task) = fixture(vec![
            (200, "Content-Type: application/json\r\n", init()),
            (202, "", String::new()),
            (
                200,
                "Content-Type: application/json\r\n",
                list(2, &["alpha"], Some("same")),
            ),
            (
                200,
                "Content-Type: application/json\r\n",
                list(3, &["beta"], Some("same")),
            ),
        ])
        .await;
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                ProfileId(server.id.0),
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        assert!(matches!(
            session.list_tools(deadline(), Cancellation::new()).await,
            Err(McpError::Protocol)
        ));
        assert!(session.stale);
        task.await.unwrap();
        let (server, task) = fixture(vec![
            (200, "Content-Type: application/json\r\n", init()), (202, "", String::new()),
            (200, "Content-Type: application/json\r\n", reply(2, r#"{"tools":[{"name":"ok","inputSchema":{"type":"object"}},{"name":"bad","inputSchema":false}]}"#)),
        ]).await;
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                ProfileId(server.id.0),
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        let result = session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap();
        assert_eq!(result.tools.len(), 1);
        assert_eq!(result.diagnostics[0].name, "bad");
        task.await.unwrap();
    }
    #[tokio::test]
    async fn late_publication_worker_cannot_restore_after_list_change() {
        let (server, task) = fixture(vec![
            (200, "Content-Type: application/json\r\n", init()),
            (202, "", String::new()),
            (
                200,
                "Content-Type: application/json\r\n",
                list(2, &["old"], None),
            ),
        ])
        .await;
        let profile = ProfileId(server.id.0);
        let repo = fixture_repository(&server);
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                profile,
                server.clone(),
                repo.clone(),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap();
        let generation = session.generation.clone();
        let (page_tx, page_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
        let discovery = tokio::spawn(async move {
            session.invalidate().await.unwrap();
            let snapshot = session.generation.load(Ordering::Acquire);
            page_tx.send(snapshot).unwrap();
            release_rx.await.unwrap();
            let result = tokio::task::spawn_blocking({
                let repo = session.repo.clone();
                let server = session.server.clone();
                let generation = session.generation.clone();
                move || {
                    permissions::replace_mcp_discovery_if_current(
                        &repo,
                        profile,
                        server.id,
                        &server,
                        vec![(
                            ToolKey {
                                profile_id: profile,
                                server: ToolServer::Mcp(server.id),
                                name: "old".into(),
                            },
                            r#"{"type":"object"}"#.into(),
                        )],
                        &generation,
                        snapshot,
                    )
                }
            })
            .await
            .unwrap();
            (session, result)
        });
        let snapshot = page_rx.await.unwrap();
        assert_eq!(snapshot, generation.load(Ordering::Acquire));
        generation.fetch_add(1, Ordering::AcqRel);
        release_tx.send(()).unwrap();
        let (mut session, result) = discovery.await.unwrap();
        assert!(matches!(result, Err(CoreError::Conflict)), "{result:?}");
        let call = ToolCall {
            key: ToolKey {
                profile_id: profile,
                server: ToolServer::Mcp(server.id),
                name: "old".into(),
            },
            provider_call_id: "call".into(),
            arguments_json: "{}".into(),
        };
        assert_eq!(
            session
                .call_tool(&call, deadline(), Cancellation::new())
                .await,
            Err(McpError::Conflict)
        );
        task.await.unwrap();
    }
    #[tokio::test]
    async fn incremented_generation_blocks_call_before_stale_flag() {
        let (server, task) = fixture(vec![
            (200, "Content-Type: application/json\r\n", init()),
            (202, "", String::new()),
            (
                200,
                "Content-Type: application/json\r\n",
                list(2, &["old"], None),
            ),
        ])
        .await;
        let profile = ProfileId(server.id.0);
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                profile,
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap();
        let call = ToolCall {
            key: ToolKey {
                profile_id: profile,
                server: ToolServer::Mcp(server.id),
                name: "old".into(),
            },
            provider_call_id: "call".into(),
            arguments_json: "{}".into(),
        };
        let generation = session.generation.clone();
        let stale = session.shared_stale.clone();
        let (incremented_tx, incremented_rx) = tokio::sync::oneshot::channel();
        let (resume_tx, resume_rx) = tokio::sync::oneshot::channel::<()>();
        let notification = tokio::spawn(async move {
            generation.fetch_add(1, Ordering::AcqRel);
            incremented_tx.send(()).unwrap();
            resume_rx.await.unwrap();
            stale.store(true, Ordering::Release);
        });
        incremented_rx.await.unwrap();
        assert!(!session.shared_stale.load(Ordering::Acquire));
        assert_eq!(
            session
                .call_tool(&call, deadline(), Cancellation::new())
                .await,
            Err(McpError::Conflict)
        );
        resume_tx.send(()).unwrap();
        notification.await.unwrap();
        assert_eq!(
            task.await
                .unwrap()
                .iter()
                .filter(|r| r.contains("\"method\":\"tools/call\""))
                .count(),
            0
        );
    }
    #[tokio::test]
    async fn final_publication_overlap_cannot_restore_eligible_old_definitions() {
        let (server, task) = fixture(vec![
            (200, "Content-Type: application/json\r\n", init()),
            (202, "", String::new()),
            (
                200,
                "Content-Type: application/json\r\n",
                list(2, &["old"], None),
            ),
        ])
        .await;
        let profile = ProfileId(server.id.0);
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                profile,
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap();
        let call = ToolCall {
            key: ToolKey {
                profile_id: profile,
                server: ToolServer::Mcp(server.id),
                name: "old".into(),
            },
            provider_call_id: "call".into(),
            arguments_json: "{}".into(),
        };
        let original = session.trusted_generation.unwrap();
        let generation = session.generation.clone();
        let stale = session.shared_stale.clone();
        let (incremented_tx, incremented_rx) = tokio::sync::oneshot::channel();
        let (resume_tx, resume_rx) = tokio::sync::oneshot::channel::<()>();
        let notification = tokio::spawn(async move {
            generation.fetch_add(1, Ordering::AcqRel);
            incremented_tx.send(()).unwrap();
            resume_rx.await.unwrap();
            stale.store(true, Ordering::Release);
        });
        incremented_rx.await.unwrap();
        // Discovery may have returned before the notification, or cleared the flag
        // immediately before it. Neither ordering makes the old generation eligible.
        assert_eq!(session.trusted_generation, Some(original));
        assert!(!session.shared_stale.load(Ordering::Acquire));
        assert_eq!(
            session
                .call_tool(&call, deadline(), Cancellation::new())
                .await,
            Err(McpError::Conflict)
        );
        resume_tx.send(()).unwrap();
        notification.await.unwrap();
        assert_eq!(
            task.await
                .unwrap()
                .iter()
                .filter(|r| r.contains("\"method\":\"tools/call\""))
                .count(),
            0
        );
    }
    #[tokio::test]
    async fn notification_during_pagination_cannot_publish_stale_page() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = ServerConfig {
            id: ServerId::new(),
            name: "fixture".into(),
            url: format!("http://{addr}/mcp"),
            allow_http: true,
            secret_headers: vec![],
        };
        let (page_tx, page_rx) = tokio::sync::oneshot::channel::<()>();
        let (resume_tx, resume_rx) = tokio::sync::oneshot::channel::<()>();
        let (change_tx, change_rx) = tokio::sync::oneshot::channel::<()>();
        let server_task = tokio::spawn(async move {
            for (status, body) in [
                (200, init()),
                (202, String::new()),
                (200, list(2, &["old"], None)),
            ] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0; 4096];
                assert!(socket.read(&mut buf).await.unwrap() > 0);
                socket.write_all(format!("HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
            let (mut get, _) = listener.accept().await.unwrap();
            let mut buf = [0; 4096];
            assert!(get.read(&mut buf).await.unwrap() > 0);
            get.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n").await.unwrap();
            let (mut page, _) = listener.accept().await.unwrap();
            assert!(page.read(&mut buf).await.unwrap() > 0);
            page_tx.send(()).unwrap();
            change_rx.await.unwrap();
            let data =
                b"data: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/tools/list_changed\"}\n\n";
            get.write_all(format!("{:x}\r\n", data.len()).as_bytes())
                .await
                .unwrap();
            get.write_all(data).await.unwrap();
            get.write_all(b"\r\n").await.unwrap();
            resume_rx.await.unwrap();
            let body = list(3, &["old"], None);
            page.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        });
        let profile = ProfileId(server.id.0);
        let repo = fixture_repository(&server);
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                profile,
                server.clone(),
                repo.clone(),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap();
        session.start_notifications(deadline()).await.unwrap();
        let discovery = tokio::spawn(async move {
            let result = session.list_tools(deadline(), Cancellation::new()).await;
            (session, result)
        });
        page_rx.await.unwrap();
        change_tx.send(()).unwrap();
        let key = ToolKey {
            profile_id: profile,
            server: ToolServer::Mcp(server.id),
            name: "old".into(),
        };
        for _ in 0..100 {
            if repo.with_connection({ let key = key.clone(); move |conn| { use rusqlite::OptionalExtension; conn.query_row("SELECT definition_digest FROM observed_tool_definitions WHERE profile_id=?1 AND tool_name=?2", rusqlite::params![key.profile_id.0.to_string(), key.name], |row| row.get::<_, String>(0)).optional().map_err(|_| CoreError::Storage) }}).unwrap().is_none() { break; }
            tokio::task::yield_now().await;
        }
        resume_tx.send(()).unwrap();
        let (mut session, result) = discovery.await.unwrap();
        assert!(matches!(result, Err(McpError::Conflict)));
        let call = ToolCall {
            key,
            provider_call_id: "call".into(),
            arguments_json: "{}".into(),
        };
        assert_eq!(
            session
                .call_tool(&call, deadline(), Cancellation::new())
                .await,
            Err(McpError::Conflict)
        );
        server_task.await.unwrap();
    }
    #[tokio::test]
    async fn monitor_list_change_revokes_while_stream_stays_open() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = ServerConfig {
            id: ServerId::new(),
            name: "fixture".into(),
            url: format!("http://{addr}/mcp"),
            allow_http: true,
            secret_headers: vec![],
        };
        let (sent_tx, sent_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
        let server_task = tokio::spawn(async move {
            for (status, body) in [
                (200, init()),
                (202, String::new()),
                (200, list(2, &["alpha"], None)),
            ] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0; 4096];
                assert!(socket.read(&mut buf).await.unwrap() > 0);
                socket.write_all(format!("HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0; 4096];
            assert!(socket.read(&mut buf).await.unwrap() > 0);
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n").await.unwrap();
            let data = b"data: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/tools/list_changed\"}\r\n\r\n";
            socket
                .write_all(format!("{:x}\r\n", data.len()).as_bytes())
                .await
                .unwrap();
            socket.write_all(data).await.unwrap();
            socket.write_all(b"\r\n").await.unwrap();
            sent_tx.send(()).unwrap();
            release_rx.await.unwrap();
            drop(socket);
        });
        let profile = ProfileId(server.id.0);
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                profile,
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap();
        session.start_notifications(deadline()).await.unwrap();
        sent_rx.await.unwrap();
        let call = ToolCall {
            key: ToolKey {
                profile_id: profile,
                server: ToolServer::Mcp(server.id),
                name: "alpha".into(),
            },
            provider_call_id: "call".into(),
            arguments_json: "{}".into(),
        };
        tokio::task::yield_now().await;
        assert_eq!(
            session
                .call_tool(&call, deadline(), Cancellation::new())
                .await,
            Err(McpError::Conflict)
        );
        release_tx.send(()).unwrap();
        session
            .close(deadline(), Cancellation::new())
            .await
            .unwrap();
        server_task.await.unwrap();
    }
    #[tokio::test]
    async fn close_expired_deadline_aborts_and_joins_monitor() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = ServerConfig {
            id: ServerId::new(),
            name: "fixture".into(),
            url: format!("http://{addr}/mcp"),
            allow_http: true,
            secret_headers: vec![],
        };
        let (get_seen_tx, get_seen_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            for (status, body) in [(200, init()), (202, String::new())] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0; 4096];
                assert!(socket.read(&mut buf).await.unwrap() > 0);
                socket.write_all(format!("HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
            let (mut get, _) = listener.accept().await.unwrap();
            let mut buf = [0; 4096];
            assert!(get.read(&mut buf).await.unwrap() > 0);
            get.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n").await.unwrap();
            get_seen_tx.send(()).unwrap();
            let _ = get.read(&mut buf).await;
        });
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                ProfileId(server.id.0),
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        session.start_notifications(deadline()).await.unwrap();
        get_seen_rx.await.unwrap();
        let expired = Instant::now() - Duration::from_millis(1);
        assert_eq!(
            session.close(expired, Cancellation::new()).await,
            Err(McpError::Cancelled)
        );
        assert!(session.monitor.is_none());
        assert!(session.closed);
        task.await.unwrap();
    }
    #[tokio::test]
    async fn monitor_open_does_not_block_tool_call_and_close_joins() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = ServerConfig {
            id: ServerId::new(),
            name: "fixture".into(),
            url: format!("http://{addr}/mcp"),
            allow_http: true,
            secret_headers: vec![],
        };
        let (seen_tx, seen_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let mut seen_tx = Some(seen_tx);
            let mut get_socket = None;
            let mut calls = 0;
            for _ in 0..5 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = vec![0; 4096];
                let n = socket.read(&mut buf).await.unwrap();
                let request = String::from_utf8_lossy(&buf[..n]).to_string();

                if request.starts_with("GET ") {
                    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n").await.unwrap();
                    seen_tx.take().unwrap().send(()).unwrap();
                    get_socket = Some(socket);
                    continue;
                }
                let (status, body) = if request.contains("\"method\":\"initialize\"") {
                    (200, init())
                } else if request.contains("\"method\":\"tools/list\"") {
                    (200, list(2, &["alpha"], None))
                } else if request.contains("\"method\":\"tools/call\"") {
                    calls += 1;
                    (
                        200,
                        reply(3, r#"{"content":[{"type":"text","text":"done"}]}"#),
                    )
                } else if request.starts_with("DELETE ") {
                    (200, String::new())
                } else {
                    (202, String::new())
                };
                socket.write_all(format!("HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
            drop(get_socket);
            calls
        });
        let profile = ProfileId(server.id.0);
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                profile,
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap();
        session.start_notifications(deadline()).await.unwrap();
        seen_rx.await.unwrap();
        let call = ToolCall {
            key: ToolKey {
                profile_id: profile,
                server: ToolServer::Mcp(server.id),
                name: "alpha".into(),
            },
            provider_call_id: "call".into(),
            arguments_json: "{}".into(),
        };
        assert_eq!(
            session
                .call_tool(&call, deadline(), Cancellation::new())
                .await
                .unwrap()
                .text,
            vec!["done"]
        );
        session
            .close(deadline(), Cancellation::new())
            .await
            .unwrap();
        assert_eq!(task.await.unwrap(), 1);
    }
    #[tokio::test]
    async fn business_http_failure_is_unknown_and_never_replayed() {
        for (status, session_header) in [(500, ""), (404, "Mcp-Session-Id: current\r\n")] {
            let (server, task) = fixture(vec![
                (
                    200,
                    "Content-Type: application/json\r\nMcp-Session-Id: current\r\n",
                    init(),
                ),
                (202, "", String::new()),
                (
                    200,
                    "Content-Type: application/json\r\n",
                    list(2, &["alpha"], None),
                ),
                (status, session_header, String::new()),
            ])
            .await;
            let profile = ProfileId(server.id.0);
            let mut session = McpClient::new(NoKey)
                .unwrap()
                .connect(
                    profile,
                    server.clone(),
                    fixture_repository(&server),
                    deadline(),
                    Cancellation::new(),
                )
                .await
                .unwrap();
            session
                .list_tools(deadline(), Cancellation::new())
                .await
                .unwrap();
            let call = ToolCall {
                key: ToolKey {
                    profile_id: profile,
                    server: ToolServer::Mcp(server.id),
                    name: "alpha".into(),
                },
                provider_call_id: "call".into(),
                arguments_json: "{}".into(),
            };
            assert_eq!(
                session
                    .call_tool(&call, deadline(), Cancellation::new())
                    .await,
                Err(McpError::OutcomeUnknown)
            );
            if status == 404 {
                assert_eq!(
                    session
                        .call_tool(&call, deadline(), Cancellation::new())
                        .await,
                    Err(McpError::Conflict)
                );
            }
            let requests = task.await.unwrap();
            assert_eq!(
                requests
                    .iter()
                    .filter(|r| r.contains("\"method\":\"tools/call\""))
                    .count(),
                1
            );
        }
    }
    #[tokio::test]
    async fn malformed_content_is_unknown_after_single_post() {
        for result in [
            r#"{"content":[{}]}"#,
            r#"{"content":[{"type":"text"}]}"#,
            r#"{"content":[],"isError":"true"}"#,
        ] {
            let (server, task) = fixture(vec![
                (200, "Content-Type: application/json\r\n", init()),
                (202, "", String::new()),
                (
                    200,
                    "Content-Type: application/json\r\n",
                    list(2, &["alpha"], None),
                ),
                (200, "Content-Type: application/json\r\n", reply(3, result)),
            ])
            .await;
            let profile = ProfileId(server.id.0);
            let mut session = McpClient::new(NoKey)
                .unwrap()
                .connect(
                    profile,
                    server.clone(),
                    fixture_repository(&server),
                    deadline(),
                    Cancellation::new(),
                )
                .await
                .unwrap();
            session
                .list_tools(deadline(), Cancellation::new())
                .await
                .unwrap();
            let call = ToolCall {
                key: ToolKey {
                    profile_id: profile,
                    server: ToolServer::Mcp(server.id),
                    name: "alpha".into(),
                },
                provider_call_id: "call".into(),
                arguments_json: "{}".into(),
            };
            assert_eq!(
                session
                    .call_tool(&call, deadline(), Cancellation::new())
                    .await,
                Err(McpError::OutcomeUnknown)
            );
            assert_eq!(
                task.await
                    .unwrap()
                    .iter()
                    .filter(|r| r.contains("\"method\":\"tools/call\""))
                    .count(),
                1
            );
        }
    }
    #[tokio::test]
    async fn malformed_or_oversized_post_reply_is_unknown_without_replay() {
        for body in [
            "not-json".to_owned(),
            "x".repeat(MAX_RPC_BYTES + 1),
            reply(77, r#"{"content":[]}"#),
        ] {
            let (server, task) = fixture(vec![
                (200, "Content-Type: application/json\r\n", init()),
                (202, "", String::new()),
                (
                    200,
                    "Content-Type: application/json\r\n",
                    list(2, &["alpha"], None),
                ),
                (200, "Content-Type: application/json\r\n", body),
            ])
            .await;
            let profile = ProfileId(server.id.0);
            let mut session = McpClient::new(NoKey)
                .unwrap()
                .connect(
                    profile,
                    server.clone(),
                    fixture_repository(&server),
                    deadline(),
                    Cancellation::new(),
                )
                .await
                .unwrap();
            session
                .list_tools(deadline(), Cancellation::new())
                .await
                .unwrap();
            let call = ToolCall {
                key: ToolKey {
                    profile_id: profile,
                    server: ToolServer::Mcp(server.id),
                    name: "alpha".into(),
                },
                provider_call_id: "call".into(),
                arguments_json: "{}".into(),
            };
            assert_eq!(
                session
                    .call_tool(&call, deadline(), Cancellation::new())
                    .await,
                Err(McpError::OutcomeUnknown)
            );
            assert_eq!(
                task.await
                    .unwrap()
                    .iter()
                    .filter(|r| r.contains("\"method\":\"tools/call\""))
                    .count(),
                1
            );
        }
    }
    #[tokio::test]
    async fn known_result_with_list_change_returns_result_and_revokes_next_call() {
        let notification = "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/tools/list_changed\"}";
        let outcome = reply(3, r#"{"content":[{"type":"text","text":"done"}]}"#);
        let (server, task) = fixture(vec![
            (200, "Content-Type: application/json\r\n", init()),
            (202, "", String::new()),
            (
                200,
                "Content-Type: application/json\r\n",
                list(2, &["alpha"], None),
            ),
            (
                200,
                "Content-Type: text/event-stream\r\n",
                format!("data: {notification}\r\n\r\ndata: {outcome}\r\n\r\n"),
            ),
        ])
        .await;
        let profile = ProfileId(server.id.0);
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                profile,
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap();
        let call = ToolCall {
            key: ToolKey {
                profile_id: profile,
                server: ToolServer::Mcp(server.id),
                name: "alpha".into(),
            },
            provider_call_id: "call".into(),
            arguments_json: "{}".into(),
        };
        assert_eq!(
            session
                .call_tool(&call, deadline(), Cancellation::new())
                .await
                .unwrap()
                .text,
            vec!["done"]
        );
        assert_eq!(
            session
                .call_tool(&call, deadline(), Cancellation::new())
                .await,
            Err(McpError::Conflict)
        );
        assert_eq!(
            task.await
                .unwrap()
                .iter()
                .filter(|r| r.contains("\"method\":\"tools/call\""))
                .count(),
            1
        );
    }
    #[tokio::test]
    async fn crlf_multiline_sse_result_with_comments() {
        let data = "{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":";
        let body = format!(": comment\r\ndata: {data}\r\ndata: {{\"tools\":[{{\"name\":\"alpha\",\"inputSchema\":{{\"type\":\"object\"}}}}]}}}}\r\n\r\n");
        let (server, task) = fixture(vec![
            (200, "Content-Type: application/json\r\n", init()),
            (202, "", String::new()),
            (200, "Content-Type: text/event-stream\r\n", body),
        ])
        .await;
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                ProfileId(server.id.0),
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        assert_eq!(
            session
                .list_tools(deadline(), Cancellation::new())
                .await
                .unwrap()
                .tools[0]
                .key
                .name,
            "alpha"
        );
        task.await.unwrap();
    }
    #[tokio::test]
    async fn optional_get_notification_stream_rejects_sampling() {
        let (server, task) = fixture(vec![
            (200, "Content-Type: application/json\r\n", init()),
            (202, "", String::new()),
            (
                200,
                "Content-Type: text/event-stream\r\n",
                "data: {\"jsonrpc\":\"2.0\",\"id\":42,\"method\":\"sampling/createMessage\"}\n\n"
                    .into(),
            ),
            (202, "", String::new()),
        ])
        .await;
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                ProfileId(server.id.0),
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        session
            .listen_notifications(deadline(), Cancellation::new())
            .await
            .unwrap();
        let requests = task.await.unwrap();
        assert!(requests[2].starts_with("GET /exact/mcp"));
        assert!(requests[3].contains("\"code\":-32601"));
    }
    #[tokio::test]
    async fn route_change_during_discovery_does_not_publish_old_observation() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = ServerConfig {
            id: ServerId::new(),
            name: "fixture".into(),
            url: format!("http://{address}/mcp"),
            allow_http: true,
            secret_headers: vec![],
        };
        let (repo, _dir, profile) = linked_repo(&server);
        let (page_seen, release) = (
            tokio::sync::oneshot::channel::<()>(),
            tokio::sync::oneshot::channel::<()>(),
        );
        let server_task = tokio::spawn(async move {
            let mut page_sender = Some(page_seen.0);
            let mut release_receiver = Some(release.1);
            for (index, body) in [init(), String::new(), list(2, &["alpha"], None)]
                .into_iter()
                .enumerate()
            {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0; 4096];
                assert!(socket.read(&mut buf).await.unwrap() > 0);
                if index == 2 {
                    page_sender.take().unwrap().send(()).unwrap();
                    release_receiver.take().unwrap().await.unwrap();
                }
                socket.write_all(format!("HTTP/1.1 {} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", if index == 1 { 202 } else { 200 }, body.len()).as_bytes()).await.unwrap();
            }
        });
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                profile,
                server.clone(),
                repo.clone(),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        let discovery =
            tokio::spawn(async move { session.list_tools(deadline(), Cancellation::new()).await });
        page_seen.1.await.unwrap();
        let mut changed = server.clone();
        changed.url = "http://127.0.0.1:1/other".into();
        repo.save_server(changed).unwrap();
        release.0.send(()).unwrap();
        assert!(matches!(discovery.await.unwrap(), Err(McpError::Conflict)));
        let key = ToolKey {
            profile_id: profile,
            server: ToolServer::Mcp(server.id),
            name: "alpha".into(),
        };
        let observed = repo.with_connection(move |conn| { use rusqlite::OptionalExtension; conn.query_row("SELECT definition_digest FROM observed_tool_definitions WHERE profile_id=?1 AND tool_name=?2", rusqlite::params![key.profile_id.0.to_string(), key.name], |row| row.get::<_, String>(0)).optional().map_err(|_| CoreError::Storage) }).unwrap();
        assert!(observed.is_none());
        server_task.await.unwrap();
    }
    #[tokio::test]
    async fn credential_ref_rotation_same_url_blocks_old_session_post() {
        struct Key;
        impl CredentialProvider for Key {
            fn resolve(
                &self,
                _: CredentialRef,
            ) -> Pin<Box<dyn Future<Output = crate::error::CoreResult<SecretValue>> + Send + '_>>
            {
                Box::pin(async { Ok(SecretValue::new("fixture-token".into())) })
            }
        }
        let (mut server, task) = fixture(vec![
            (200, "Content-Type: application/json\r\n", init()),
            (202, "", String::new()),
            (
                200,
                "Content-Type: application/json\r\n",
                list(2, &["alpha"], None),
            ),
        ])
        .await;
        server.secret_headers = vec![("Authorization".into(), CredentialRef::new())];
        let (repo, _dir, profile) = linked_repo(&server);
        let mut session = McpClient::new(Key)
            .unwrap()
            .connect(
                profile,
                server.clone(),
                repo.clone(),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap();
        let mut rotated = server.clone();
        rotated.secret_headers = vec![("authorization".into(), CredentialRef::new())];
        repo.save_server(rotated).unwrap();
        let call = ToolCall {
            key: ToolKey {
                profile_id: profile,
                server: ToolServer::Mcp(server.id),
                name: "alpha".into(),
            },
            provider_call_id: "call".into(),
            arguments_json: "{}".into(),
        };
        assert_eq!(
            session
                .call_tool(&call, deadline(), Cancellation::new())
                .await,
            Err(McpError::Conflict)
        );
        assert_eq!(
            task.await
                .unwrap()
                .iter()
                .filter(|r| r.contains("\"method\":\"tools/call\""))
                .count(),
            0
        );
    }
    #[tokio::test]
    async fn route_change_after_discovery_prevents_business_post() {
        let (server, task) = fixture(vec![
            (200, "Content-Type: application/json\r\n", init()),
            (202, "", String::new()),
            (
                200,
                "Content-Type: application/json\r\n",
                list(2, &["alpha"], None),
            ),
        ])
        .await;
        let (repo, _dir, profile) = linked_repo(&server);
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                profile,
                server.clone(),
                repo.clone(),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap();
        let call = ToolCall {
            key: ToolKey {
                profile_id: profile,
                server: ToolServer::Mcp(server.id),
                name: "alpha".into(),
            },
            provider_call_id: "call".into(),
            arguments_json: "{}".into(),
        };
        let mut changed = server.clone();
        changed.url = "http://127.0.0.1:1/new-route".into();
        repo.save_server(changed).unwrap();
        assert_eq!(
            session
                .call_tool(&call, deadline(), Cancellation::new())
                .await,
            Err(McpError::Conflict)
        );
        let requests = task.await.unwrap();
        assert_eq!(requests.len(), 3);
    }
    #[tokio::test]
    async fn partial_page_failure_revokes_previous_catalog() {
        let directory = tempfile::tempdir().unwrap();
        let repo = Repository::open(&directory.path().join("mcp.db")).unwrap();
        let profile = ProfileId::new();
        let endpoint = EndpointId::new();
        repo.save_endpoint(EndpointConfig {
            id: endpoint,
            name: "model".into(),
            base_url: "https://example.test/v1".into(),
            model: "model".into(),
            context_tokens: 100,
            max_output_tokens: 10,
            temperature: 0.1,
            allow_http: false,
            credential: None,
        })
        .unwrap();
        let (server, task) = fixture(vec![
            (200, "Content-Type: application/json\r\n", init()),
            (202, "", String::new()),
            (
                200,
                "Content-Type: application/json\r\n",
                list(2, &["alpha"], None),
            ),
            (
                200,
                "Content-Type: application/json\r\n",
                list(3, &["beta"], Some("next")),
            ),
            (
                200,
                "Content-Type: application/json\r\n",
                reply(999, r#"{"tools":[]}"#),
            ),
        ])
        .await;
        repo.save_server(server.clone()).unwrap();
        repo.save_profile(Profile {
            mode: crate::model::core::ProfileMode::Agent,
            id: profile,
            name: "profile".into(),
            endpoint_id: endpoint,
            server_ids: vec![server.id],
            instructions: String::new(),
            limits: RunLimits::default(),
        })
        .unwrap();
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                profile,
                server,
                repo.clone(),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        let old = session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap()
            .tools
            .remove(0);
        assert!(matches!(
            session.list_tools(deadline(), Cancellation::new()).await,
            Err(McpError::Protocol)
        ));
        assert!(session.stale);
        let observed = repo.with_connection(move |conn| {
            use rusqlite::OptionalExtension;
            conn.query_row("SELECT definition_digest FROM observed_tool_definitions WHERE profile_id=?1 AND tool_name=?2", rusqlite::params![old.key.profile_id.0.to_string(), old.key.name], |row| row.get::<_, String>(0)).optional().map_err(|_| crate::error::CoreError::Storage)
        }).unwrap();
        assert!(observed.is_none());
        task.await.unwrap();
    }
    #[tokio::test]
    async fn unsupported_schema_does_not_hide_compatible_tools() {
        let (server, task) = fixture(vec![
            (200, "Content-Type: application/json\r\n", init()), (202, "", String::new()),
            (200, "Content-Type: application/json\r\n", reply(2, r#"{"tools":[{"name":"ok","inputSchema":{"type":"object"}},{"name":"unsupported","inputSchema":{"$ref":"https://example.test/schema"}}]}"#)),
        ]).await;
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                ProfileId(server.id.0),
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        let result = session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap();
        assert_eq!(result.tools.len(), 1);
        assert_eq!(result.diagnostics[0].name, "unsupported");
        task.await.unwrap();
    }
    #[tokio::test]
    async fn cancellation_and_close_do_not_replay_business_post() {
        let (server, task) = fixture(vec![
            (
                200,
                "Content-Type: application/json\r\nMcp-Session-Id: close-me\r\n",
                init(),
            ),
            (202, "", String::new()),
            (
                200,
                "Content-Type: application/json\r\n",
                list(2, &["alpha"], None),
            ),
            (
                200,
                "Content-Type: application/json\r\n",
                reply(3, r#"{"content":[]}"#),
            ),
            (200, "", String::new()),
        ])
        .await;
        let profile = ProfileId(server.id.0);
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                profile,
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap();
        let call = ToolCall {
            key: ToolKey {
                profile_id: profile,
                server: ToolServer::Mcp(server.id),
                name: "alpha".into(),
            },
            provider_call_id: "call".into(),
            arguments_json: "{}".into(),
        };
        let cancel = Cancellation::new();
        cancel.cancel();
        assert_eq!(
            session.call_tool(&call, deadline(), cancel).await,
            Err(McpError::Cancelled)
        );
        assert!(
            !session
                .call_tool(&call, deadline(), Cancellation::new())
                .await
                .unwrap()
                .is_error
        );
        session
            .close(deadline(), Cancellation::new())
            .await
            .unwrap();
        assert_eq!(
            session
                .call_tool(&call, deadline(), Cancellation::new())
                .await,
            Err(McpError::Conflict)
        );
        let requests = task.await.unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|v| v.contains("\"method\":\"tools/call\""))
                .count(),
            1
        );
        assert!(requests[4].starts_with("DELETE /exact/mcp"));
    }
    #[tokio::test]
    async fn incompatible_version_and_unavailable_server_are_isolated() {
        let (bad, bad_task) = fixture(vec![(200, "Content-Type: application/json\r\n", reply(1, r#"{"protocolVersion":"2026-07-28","capabilities":{},"serverInfo":{"name":"future","version":"1"}}"#))]).await;
        let (good, good_task) = fixture(vec![
            (200, "Content-Type: application/json\r\n", init()),
            (202, "", String::new()),
        ])
        .await;
        let client = McpClient::new(NoKey).unwrap();
        assert!(matches!(
            client
                .connect(
                    ProfileId::new(),
                    bad.clone(),
                    linked_repo(&bad).0,
                    deadline(),
                    Cancellation::new()
                )
                .await,
            Err(McpError::UnsupportedVersion)
        ));
        assert!(McpClient::new(NoKey)
            .unwrap()
            .connect(
                ProfileId::new(),
                good.clone(),
                linked_repo(&good).0,
                deadline(),
                Cancellation::new()
            )
            .await
            .is_ok());
        bad_task.await.unwrap();
        good_task.await.unwrap();
    }
    #[tokio::test]
    async fn response_limit_equal_and_one_over() {
        // Construct exact wire lengths independently of the client's byte accounting.
        for extra in [0, 1] {
            let empty = reply(2, r#"{"tools":[],"padding":""}"#);
            let body = reply(
                2,
                &format!(
                    r#"{{"tools":[],"padding":"{}"}}"#,
                    "x".repeat(MAX_RPC_BYTES + extra - empty.len())
                ),
            );
            assert_eq!(body.len(), MAX_RPC_BYTES + extra);
            let (server, task) = fixture(vec![
                (200, "Content-Type: application/json\r\n", init()),
                (202, "", String::new()),
                (200, "Content-Type: application/json\r\n", body),
            ])
            .await;
            let mut session = McpClient::new(NoKey)
                .unwrap()
                .connect(
                    ProfileId(server.id.0),
                    server.clone(),
                    fixture_repository(&server),
                    deadline(),
                    Cancellation::new(),
                )
                .await
                .unwrap();
            let result = session.list_tools(deadline(), Cancellation::new()).await;
            if extra == 0 {
                assert!(result.is_ok(), "{result:?}");
            } else {
                assert!(matches!(result, Err(McpError::Limit)));
            }
            task.await.unwrap();
        }
    }
    #[tokio::test]
    async fn unknown_transport_actionable_error() {
        let (server, task) = fixture(vec![(405, "", String::new())]).await;
        assert!(matches!(
            McpClient::new(NoKey)
                .unwrap()
                .connect(
                    ProfileId(server.id.0),
                    server.clone(),
                    fixture_repository(&server),
                    deadline(),
                    Cancellation::new()
                )
                .await,
            Err(McpError::UnsupportedTransport)
        ));
        task.await.unwrap();
    }
    use crate::failure::{self, FailureStep, FailureTarget};

    async fn connect_noted(
        server: &ServerConfig,
    ) -> (McpResult<McpSession<NoKey>>, Option<failure::FailureDetail>) {
        failure::scope(async {
            McpClient::new(NoKey)
                .unwrap()
                .connect(
                    ProfileId(server.id.0),
                    server.clone(),
                    fixture_repository(server),
                    deadline(),
                    Cancellation::new(),
                )
                .await
        })
        .await
    }
    #[tokio::test]
    async fn initialize_401_notes_status_reason_and_step() {
        let (server, task) = fixture(vec![(
            401,
            "Content-Type: application/json\r\n",
            r#"{"error":"unknown API key"}"#.into(),
        )])
        .await;
        let (result, detail) = connect_noted(&server).await;
        assert!(matches!(result, Err(McpError::Authentication)));
        let detail = detail.expect("detail");
        assert_eq!(detail.step, FailureStep::Initialize);
        assert_eq!(detail.status, Some(401));
        assert_eq!(detail.reason.as_deref(), Some("unknown API key"));
        assert_eq!(
            detail.target,
            FailureTarget::McpServer {
                name: "fixture".into()
            }
        );
        task.await.unwrap();
    }
    #[tokio::test]
    async fn initialize_404_is_unsupported_transport_with_detail() {
        let (server, task) = fixture(vec![(404, "", String::new())]).await;
        let (result, detail) = connect_noted(&server).await;
        assert!(matches!(result, Err(McpError::UnsupportedTransport)));
        let detail = detail.expect("detail");
        assert_eq!(detail.step, FailureStep::Initialize);
        assert_eq!(detail.status, Some(404));
        task.await.unwrap();
    }
    #[tokio::test(start_paused = true)]
    async fn initialize_stalled_error_body_still_maps_status() {
        // Headers promise a body that never arrives; the bounded read must not hang.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = ServerConfig {
            id: ServerId::new(),
            name: "fixture".into(),
            url: format!("http://{addr}/mcp"),
            allow_http: true,
            secret_headers: vec![],
        };
        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0; 4096];
            assert!(socket.read(&mut buf).await.unwrap() > 0);
            socket
                .write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 50\r\n\r\n")
                .await
                .unwrap();
            let _ = done_rx.await;
        });
        let (result, detail) = connect_noted(&server).await;
        assert!(matches!(result, Err(McpError::Authentication)));
        assert_eq!(detail.expect("detail").status, Some(401));
        let _ = done_tx.send(());
        task.await.unwrap();
    }
    #[tokio::test]
    async fn list_and_call_failures_carry_their_steps() {
        let (server, task) = fixture(vec![
            (200, "Content-Type: application/json\r\n", init()),
            (202, "", String::new()),
            (500, "", r#"{"message":"list broke"}"#.into()),
            (
                200,
                "Content-Type: application/json\r\n",
                list(3, &["alpha"], None),
            ),
            (502, "", String::new()),
        ])
        .await;
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                ProfileId(server.id.0),
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        let (result, detail) =
            failure::scope(async { session.list_tools(deadline(), Cancellation::new()).await })
                .await;
        assert!(matches!(result, Err(McpError::Protocol)));
        let detail = detail.expect("detail");
        assert_eq!(detail.step, FailureStep::ListTools);
        assert_eq!(detail.status, Some(500));
        assert_eq!(detail.reason.as_deref(), Some("list broke"));
        session
            .list_tools(deadline(), Cancellation::new())
            .await
            .unwrap();
        let call = ToolCall {
            key: ToolKey {
                profile_id: session.profile_id(),
                server: ToolServer::Mcp(server.id),
                name: "alpha".into(),
            },
            provider_call_id: "call".into(),
            arguments_json: "{}".into(),
        };
        let (result, detail) = failure::scope(async {
            session
                .call_tool(&call, deadline(), Cancellation::new())
                .await
        })
        .await;
        assert!(matches!(result, Err(McpError::OutcomeUnknown)));
        let detail = detail.expect("detail");
        assert_eq!(detail.step, FailureStep::CallTool);
        assert_eq!(detail.status, Some(502));
        task.await.unwrap();
    }
    /// Serves initialize (with a session id) and its ack, then a stalled 404 for the next request.
    async fn session_then_stalled_404() -> (
        ServerConfig,
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = ServerConfig {
            id: ServerId::new(),
            name: "fixture".into(),
            url: format!("http://{addr}/mcp"),
            allow_http: true,
            secret_headers: vec![],
        };
        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            for (status, headers, body) in [
                (
                    200,
                    "Content-Type: application/json\r\nMcp-Session-Id: sess-1\r\n",
                    init(),
                ),
                (202, "", String::new()),
            ] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0; 4096];
                assert!(socket.read(&mut buf).await.unwrap() > 0);
                socket.write_all(format!("HTTP/1.1 {status} OK\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0; 4096];
            assert!(socket.read(&mut buf).await.unwrap() > 0);
            socket
                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 50\r\n\r\n")
                .await
                .unwrap();
            let _ = done_rx.await;
        });
        (server, done_tx, task)
    }
    #[tokio::test(start_paused = true)]
    async fn list_404_invalidates_session_even_if_body_read_hits_deadline() {
        let (server, done_tx, task) = session_then_stalled_404().await;
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                ProfileId(server.id.0),
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        assert!(session.session_id.is_some());
        let short = Instant::now() + Duration::from_secs(1);
        let result = session.list_tools(short, Cancellation::new()).await;
        assert!(matches!(result, Err(McpError::Cancelled)));
        assert!(session.session_id.is_none());
        let call = ToolCall {
            key: ToolKey {
                profile_id: session.profile_id(),
                server: ToolServer::Mcp(server.id),
                name: "alpha".into(),
            },
            provider_call_id: "call".into(),
            arguments_json: "{}".into(),
        };
        assert_eq!(
            session
                .call_tool(&call, deadline(), Cancellation::new())
                .await,
            Err(McpError::Conflict)
        );
        let _ = done_tx.send(());
        task.await.unwrap();
    }
    #[tokio::test]
    async fn close_failure_does_not_overwrite_outer_note() {
        let (server, done_tx, task) = session_then_stalled_404().await;
        let mut session = McpClient::new(NoKey)
            .unwrap()
            .connect(
                ProfileId(server.id.0),
                server.clone(),
                fixture_repository(&server),
                deadline(),
                Cancellation::new(),
            )
            .await
            .unwrap();
        // Free the port so the DELETE is refused.
        let _ = done_tx.send(());
        task.abort();
        let _ = task.await;
        let original = failure::FailureDetail {
            target: FailureTarget::Inference { name: "llm".into() },
            step: FailureStep::ChatRequest,
            url: None,
            status: Some(401),
            reason: Some("bad key".into()),
        };
        let (result, detail) = failure::scope(async {
            failure::note(original.clone());
            session.close(deadline(), Cancellation::new()).await
        })
        .await;
        assert!(result.is_err());
        assert_eq!(detail, Some(original));
    }
}
