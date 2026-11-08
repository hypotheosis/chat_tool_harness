use crate::error::{CoreError, CoreResult};
use crate::model::core::{EndpointConfig, ToolCall, ToolKey};
use crate::net::{next_chunk, Cancellation, CredentialProvider, NetClient, MAX_RESPONSE_BYTES};
use crate::permissions::MAX_TOOL_JSON_BYTES;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use tokio::sync::mpsc;
use tokio::time::Instant;

pub const MAX_FRAME_BYTES: usize = 256 * 1024;
pub const MAX_TEXT_BYTES: usize = 1024 * 1024;
pub const MAX_TOOLS: usize = 64;
pub const EVENT_CHANNEL_CAPACITY: usize = 32;

fn tool_alias(key: &ToolKey) -> String {
    let server = match key.server {
        crate::model::core::ToolServer::LocalFiles => "local".to_owned(),
        crate::model::core::ToolServer::Mcp(id) => format!("mcp:{}", id.0),
    };
    let mut digest = Sha256::new();
    digest.update(key.profile_id.0.as_bytes());
    digest.update([0]);
    digest.update(server.as_bytes());
    digest.update([0]);
    digest.update(key.name.as_bytes());
    let bytes = digest.finalize();
    let mut alias = String::with_capacity(37);
    alias.push_str("tool_");
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in &bytes[..16] {
        alias.push(HEX[(byte >> 4) as usize] as char);
        alias.push(HEX[(byte & 0x0f) as usize] as char);
    }
    alias
}

#[derive(Debug, Clone, Copy)]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}
impl ChatRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub tool_call_id: Option<String>,
}
impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }
    pub fn assistant_with_calls(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
            tool_calls,
            tool_call_id: None,
        }
    }
    pub fn tool_result(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Tool,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: Some(call_id.into()),
        }
    }
}
#[derive(Debug, Clone)]
pub struct AdvertisedTool {
    pub key: ToolKey,
    pub description: String,
    pub schema_json: String,
}
#[derive(Debug, Clone)]
pub struct InferenceRequest {
    pub endpoint: EndpointConfig,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<AdvertisedTool>,
    pub deadline: Instant,
}
impl InferenceRequest {
    pub fn validate(&self) -> CoreResult<()> {
        crate::net::endpoint_url(&self.endpoint, "chat/completions")?;
        let e = &self.endpoint;
        if e.model.trim().is_empty()
            || e.model.len() > 256
            || e.context_tokens == 0
            || e.context_tokens > 1_000_000
            || e.max_output_tokens == 0
            || e.max_output_tokens > e.context_tokens
            || !e.temperature.is_finite()
            || !(0.0..=2.0).contains(&e.temperature)
            || self.tools.len() > MAX_TOOLS
            || self.messages.len() > 256
            || self
                .messages
                .iter()
                .any(|m| m.content.len() > MAX_TEXT_BYTES)
        {
            return Err(CoreError::InvalidInput);
        }
        let mut outstanding = HashSet::new();
        for message in &self.messages {
            match message.role {
                ChatRole::Tool => {
                    if !message.tool_calls.is_empty()
                        || !message
                            .tool_call_id
                            .as_ref()
                            .is_some_and(|id| outstanding.remove(id))
                    {
                        return Err(CoreError::InvalidInput);
                    }
                }
                ChatRole::Assistant => {
                    if message.tool_call_id.is_some() {
                        return Err(CoreError::InvalidInput);
                    }
                    for call in &message.tool_calls {
                        if call.provider_call_id.is_empty()
                            || call.provider_call_id.len() > 256
                            || !outstanding.insert(call.provider_call_id.clone())
                            || !crate::permissions::parse_json(&call.arguments_json)
                                .is_ok_and(|v| v.is_object())
                        {
                            return Err(CoreError::InvalidInput);
                        }
                    }
                }
                ChatRole::System | ChatRole::User => {
                    if message.tool_call_id.is_some() || !message.tool_calls.is_empty() {
                        return Err(CoreError::InvalidInput);
                    }
                }
            }
        }
        let mut advertised = HashSet::new();
        for t in &self.tools {
            if !advertised.insert(tool_alias(&t.key))
                || t.key.name.is_empty()
                || t.description.len() > 4096
                || t.schema_json.len() > MAX_TOOL_JSON_BYTES
                || !serde_json::from_str::<Value>(&t.schema_json).is_ok_and(|v| v.is_object())
            {
                return Err(CoreError::InvalidInput);
            }
        }
        Ok(())
    }
    pub(crate) fn payload(&self) -> CoreResult<Vec<u8>> {
        let messages: Vec<_> = self
            .messages
            .iter()
            .map(|m| {
                let mut value = json!({"role":m.role.as_str(),"content":m.content});
                if let Some(id) = &m.tool_call_id {
                    value["tool_call_id"] = json!(id);
                }
                if !m.tool_calls.is_empty() {
                    value["tool_calls"] = json!(m.tool_calls.iter().map(|call| json!({
                    "id":call.provider_call_id,
                    "type":"function",
                    "function":{"name":tool_alias(&call.key),"arguments":call.arguments_json}
                })).collect::<Vec<_>>());
                }
                value
            })
            .collect();
        let tools: Vec<_> = self.tools.iter().map(|t| {
            let schema: Value = serde_json::from_str(&t.schema_json).map_err(|_| CoreError::InvalidInput)?;
            Ok(json!({"type":"function","function":{"name":tool_alias(&t.key),"description":t.description,"parameters":schema}}))
        }).collect::<CoreResult<_>>()?;
        let mut body = json!({"model":self.endpoint.model,"messages":messages,"stream":true,"stream_options":{"include_usage":true},"temperature":self.endpoint.temperature,"max_tokens":self.endpoint.max_output_tokens});
        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }
        serde_json::to_vec(&body).map_err(|_| CoreError::InvalidInput)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferenceTurn {
    pub text: String,
    pub calls: Vec<ToolCall>,
    pub usage: Option<Usage>,
    pub finish_reason: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceEvent {
    Text(String),
    WarningChatOnly,
}
/// Mandatory diagnostic for endpoint capability UI; never infer support from event delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSupport {
    Supported,
    Unsupported,
    Inconclusive,
}
pub type EventSender = mpsc::Sender<InferenceEvent>;

pub struct InferenceClient<P> {
    net: NetClient<P>,
}
impl<P: CredentialProvider> InferenceClient<P> {
    pub fn new(provider: P) -> CoreResult<Self> {
        Ok(Self {
            net: NetClient::new(provider)?,
        })
    }
    pub async fn discover_models(&self, endpoint: &EndpointConfig) -> CoreResult<Vec<String>> {
        self.discover_models_with(
            endpoint,
            Instant::now() + std::time::Duration::from_secs(150),
            Cancellation::new(),
        )
        .await
    }
    pub async fn discover_models_with(
        &self,
        endpoint: &EndpointConfig,
        deadline: Instant,
        cancel: Cancellation,
    ) -> CoreResult<Vec<String>> {
        crate::failure::set_step(crate::failure::FailureStep::DiscoverModels);
        let mut response = match self
            .net
            .get(endpoint, "models", deadline, cancel.clone())
            .await
        {
            Err(CoreError::Protocol) => return Err(CoreError::DiscoveryUnavailable),
            other => other?,
        };
        let mut data = Vec::new();
        while let Some(chunk) = next_chunk(
            &mut response,
            deadline,
            &cancel,
            MAX_RESPONSE_BYTES - data.len(),
        )
        .await?
        {
            if data.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err(CoreError::Limit);
            }
            data.extend_from_slice(&chunk);
        }
        let parsed: Value = serde_json::from_slice(&data).map_err(|_| CoreError::Protocol)?;
        let entries = parsed
            .get("data")
            .and_then(Value::as_array)
            .ok_or(CoreError::Protocol)?;
        if entries.len() > 1024 {
            return Err(CoreError::Limit);
        }
        entries
            .iter()
            .map(|v| {
                v.get("id")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty() && s.len() <= 256)
                    .map(str::to_owned)
                    .ok_or(CoreError::Protocol)
            })
            .collect()
    }
    pub async fn stream_turn(
        &self,
        request: InferenceRequest,
        events: EventSender,
        cancel: Cancellation,
    ) -> CoreResult<InferenceTurn> {
        self.stream_turn_with_choice(request, events, cancel, false)
            .await
    }
    async fn stream_turn_with_choice(
        &self,
        request: InferenceRequest,
        events: EventSender,
        cancel: Cancellation,
        required_tool: bool,
    ) -> CoreResult<InferenceTurn> {
        request.validate()?;
        let mut payload = request.payload()?;
        if required_tool {
            let mut body: Value =
                serde_json::from_slice(&payload).map_err(|_| CoreError::InvalidInput)?;
            body["tool_choice"] = json!("required");
            payload = serde_json::to_vec(&body).map_err(|_| CoreError::InvalidInput)?;
        }
        crate::failure::set_step(crate::failure::FailureStep::ChatRequest);
        let mut response = self
            .net
            .post(
                &request.endpoint,
                "chat/completions",
                payload,
                request.deadline,
                &cancel,
            )
            .await?;
        let mut decoder = StreamDecoder::new(&request.tools)?;
        loop {
            let chunk = next_chunk(
                &mut response,
                request.deadline,
                &cancel,
                MAX_RESPONSE_BYTES - decoder.total,
            )
            .await?;
            let Some(chunk) = chunk else {
                break;
            };
            for text in decoder.push(&chunk)? {
                tokio::select! {
                    _ = cancel.cancelled() => return Err(CoreError::Cancelled),
                    result = tokio::time::timeout_at(request.deadline, events.send(InferenceEvent::Text(text))) => {
                        result.map_err(|_| CoreError::Cancelled)?.map_err(|_| CoreError::Cancelled)?;
                    }
                }
            }
        }
        decoder.finish()
    }
    pub async fn probe_tools(
        &self,
        mut request: InferenceRequest,
        events: EventSender,
        cancel: Cancellation,
    ) -> CoreResult<ToolSupport> {
        request.tools = vec![AdvertisedTool {
            key: ToolKey {
                profile_id: crate::model::core::ProfileId::new(),
                server: crate::model::core::ToolServer::LocalFiles,
                name: "probe".into(),
            },
            description: "Call this diagnostic function with an empty object".into(),
            schema_json: "{\"type\":\"object\"}".into(),
        }];
        match self
            .stream_turn_with_choice(request, events, cancel, true)
            .await
        {
            Ok(turn) if !turn.calls.is_empty() => Ok(ToolSupport::Supported),
            Ok(_) => Ok(ToolSupport::Inconclusive),
            Err(CoreError::ToolUnsupported) => Ok(ToolSupport::Unsupported),
            Err(e) => Err(e),
        }
    }
}

#[derive(Default)]
struct PendingCall {
    id: String,
    name: String,
    args: String,
    typed: bool,
}
pub struct StreamDecoder {
    aliases: HashMap<String, ToolKey>,
    frame: Vec<u8>,
    event_data: Option<Vec<u8>>,
    total: usize,
    text: String,
    pending: BTreeMap<u64, PendingCall>,
    ids: HashSet<String>,
    usage: Option<Usage>,
    finish_reason: Option<String>,
    done: bool,
}
impl StreamDecoder {
    pub fn new(tools: &[AdvertisedTool]) -> CoreResult<Self> {
        if tools.len() > MAX_TOOLS {
            return Err(CoreError::Limit);
        }
        let aliases: HashMap<_, _> = tools
            .iter()
            .map(|t| (tool_alias(&t.key), t.key.clone()))
            .collect();
        if aliases.len() != tools.len() {
            return Err(CoreError::InvalidInput);
        }
        Ok(Self {
            aliases,
            frame: Vec::new(),
            event_data: None,
            total: 0,
            text: String::new(),
            pending: BTreeMap::new(),
            ids: HashSet::new(),
            usage: None,
            finish_reason: None,
            done: false,
        })
    }
    pub fn push(&mut self, bytes: &[u8]) -> CoreResult<Vec<String>> {
        self.total = self
            .total
            .checked_add(bytes.len())
            .filter(|n| *n <= MAX_RESPONSE_BYTES)
            .ok_or(CoreError::Limit)?;
        let mut out = Vec::new();
        for byte in bytes {
            if self.done {
                return Err(CoreError::Protocol);
            }
            if *byte == b'\n' {
                let line = std::mem::take(&mut self.frame);
                let line = line.strip_suffix(b"\r").unwrap_or(&line);
                if line.is_empty() {
                    if let Some(data) = self.event_data.take() {
                        if data == b"[DONE]" {
                            self.done = true;
                            continue;
                        }
                        let value: Value =
                            serde_json::from_slice(&data).map_err(|_| CoreError::Protocol)?;
                        if let Some(error) = value.get("error") {
                            return Err(match error.get("code").and_then(Value::as_str) {
                                Some("context_length_exceeded" | "context_window_exceeded") => {
                                    CoreError::ContextLength
                                }
                                _ => CoreError::Protocol,
                            });
                        }
                        if let Some(usage) = value.get("usage").filter(|v| !v.is_null()) {
                            self.usage = Some(Usage {
                                prompt_tokens: usage
                                    .get("prompt_tokens")
                                    .and_then(Value::as_u64)
                                    .ok_or(CoreError::Protocol)?,
                                completion_tokens: usage
                                    .get("completion_tokens")
                                    .and_then(Value::as_u64)
                                    .ok_or(CoreError::Protocol)?,
                            });
                        }
                        let choices = value
                            .get("choices")
                            .and_then(Value::as_array)
                            .ok_or(CoreError::Protocol)?;
                        if choices.len() > 1 {
                            return Err(CoreError::Protocol);
                        }
                        if let Some(choice) = choices.first() {
                            let delta = choice
                                .get("delta")
                                .and_then(Value::as_object)
                                .ok_or(CoreError::Protocol)?;
                            if let Some(content) = delta.get("content").filter(|v| !v.is_null()) {
                                let text = content.as_str().ok_or(CoreError::Protocol)?;
                                if self.text.len().saturating_add(text.len()) > MAX_TEXT_BYTES {
                                    return Err(CoreError::Limit);
                                }
                                self.text.push_str(text);
                                out.push(text.to_owned());
                            }
                            if let Some(calls) = delta.get("tool_calls") {
                                for call in calls.as_array().ok_or(CoreError::Protocol)? {
                                    let index = call
                                        .get("index")
                                        .and_then(Value::as_u64)
                                        .ok_or(CoreError::Protocol)?;
                                    if index >= MAX_TOOLS as u64 {
                                        return Err(CoreError::Limit);
                                    }
                                    let pending = self.pending.entry(index).or_default();
                                    if let Some(id) = call.get("id") {
                                        let id = id
                                            .as_str()
                                            .filter(|s| !s.is_empty() && s.len() <= 256)
                                            .ok_or(CoreError::Protocol)?;
                                        if !pending.id.is_empty() || !self.ids.insert(id.to_owned())
                                        {
                                            return Err(CoreError::Protocol);
                                        }
                                        pending.id = id.to_owned();
                                    }
                                    if let Some(kind) = call.get("type") {
                                        if kind != "function" || pending.typed {
                                            return Err(CoreError::Protocol);
                                        }
                                        pending.typed = true;
                                    }
                                    if let Some(function) = call.get("function") {
                                        if let Some(name) = function.get("name") {
                                            let name = name.as_str().ok_or(CoreError::Protocol)?;
                                            if !pending.name.is_empty() {
                                                return Err(CoreError::Protocol);
                                            }
                                            pending.name = name.to_owned();
                                        }
                                        if let Some(args) = function.get("arguments") {
                                            let args = args.as_str().ok_or(CoreError::Protocol)?;
                                            if pending.args.len().saturating_add(args.len())
                                                > MAX_TOOL_JSON_BYTES
                                            {
                                                return Err(CoreError::Limit);
                                            }
                                            pending.args.push_str(args);
                                        }
                                    }
                                }
                            }
                            if let Some(reason) =
                                choice.get("finish_reason").filter(|v| !v.is_null())
                            {
                                let reason = reason.as_str().ok_or(CoreError::Protocol)?;
                                if self.finish_reason.is_some() {
                                    return Err(CoreError::Protocol);
                                }
                                self.finish_reason = Some(reason.to_owned());
                            }
                        }
                    }
                } else if let Some(data) = line.strip_prefix(b"data:") {
                    let data = data.strip_prefix(b" ").unwrap_or(data);
                    let event = self.event_data.get_or_insert_with(Vec::new);
                    if !event.is_empty() {
                        event.push(b'\n');
                    }
                    if event.len().saturating_add(data.len()) > MAX_FRAME_BYTES {
                        return Err(CoreError::Limit);
                    }
                    event.extend_from_slice(data);
                } else if !(line.starts_with(b":")
                    || line.starts_with(b"event:")
                    || line.starts_with(b"id:"))
                {
                    return Err(CoreError::Protocol);
                }
            } else {
                if self.frame.len() >= MAX_FRAME_BYTES {
                    return Err(CoreError::Limit);
                }
                self.frame.push(*byte);
            }
        }
        Ok(out)
    }
    pub fn finish(self) -> CoreResult<InferenceTurn> {
        if !self.done || !self.frame.is_empty() || self.event_data.is_some() {
            return Err(CoreError::Protocol);
        }
        let reason = self.finish_reason.ok_or(CoreError::Protocol)?;
        if !matches!(reason.as_str(), "stop" | "tool_calls" | "length") {
            return Err(CoreError::Protocol);
        }
        if reason == "length" && !self.pending.is_empty() {
            return Err(CoreError::Protocol);
        }
        if (reason == "tool_calls") != !self.pending.is_empty() {
            return Err(CoreError::Protocol);
        }
        let mut calls = Vec::new();
        for (_, pending) in self.pending {
            if !pending.typed || pending.id.is_empty() {
                return Err(CoreError::Protocol);
            }
            let key = self
                .aliases
                .get(&pending.name)
                .ok_or(CoreError::Protocol)?
                .clone();
            let parsed =
                crate::permissions::parse_json(&pending.args).map_err(|_| CoreError::Protocol)?;
            if !parsed.is_object() {
                return Err(CoreError::Protocol);
            }
            calls.push(ToolCall {
                key,
                provider_call_id: pending.id,
                arguments_json: pending.args,
            });
        }
        Ok(InferenceTurn {
            text: self.text,
            calls,
            usage: self.usage,
            finish_reason: reason,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streamed_context_rejection_is_typed_without_error_body_disclosure() {
        for code in ["context_length_exceeded", "context_window_exceeded"] {
            let mut decoder = StreamDecoder::new(&[]).unwrap();
            let frame = format!(
                "data: {{\"error\":{{\"code\":\"{code}\",\"message\":\"private payload\"}}}}\n\n"
            );
            let error = decoder.push(frame.as_bytes()).unwrap_err();
            assert_eq!(error, CoreError::ContextLength);
            assert!(!error.to_string().contains("private payload"));
        }
    }
    use crate::model::core::{EndpointId, ProfileId, ToolServer};
    fn endpoint() -> EndpointConfig {
        EndpointConfig {
            id: EndpointId::new(),
            name: "fixture".into(),
            base_url: "https://fixture.test/v1".into(),
            model: "manual-id".into(),
            context_tokens: 1024,
            max_output_tokens: 128,
            temperature: 0.2,
            allow_http: false,
            credential: None,
        }
    }
    fn advertised(name: &str) -> AdvertisedTool {
        AdvertisedTool {
            key: ToolKey {
                profile_id: ProfileId::new(),
                server: ToolServer::LocalFiles,
                name: name.into(),
            },
            description: "Read a path".into(),
            schema_json: r#"{"type":"object"}"#.into(),
        }
    }
    fn request(tools: Vec<AdvertisedTool>) -> InferenceRequest {
        InferenceRequest {
            endpoint: endpoint(),
            messages: vec![ChatMessage::user("hello")],
            tools,
            deadline: Instant::now() + std::time::Duration::from_secs(300),
        }
    }
    #[test]
    fn inference_generic_stream_future_is_send() {
        fn assert_send<T: Send>(_: T) {}
        fn check<P: CredentialProvider>(
            client: &InferenceClient<P>,
            req: InferenceRequest,
            events: EventSender,
        ) {
            assert_send(client.stream_turn(req, events, Cancellation::new()));
        }
        struct NoKey;
        impl CredentialProvider for NoKey {
            fn resolve(
                &self,
                _: crate::model::core::CredentialRef,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<Output = CoreResult<crate::net::SecretValue>>
                        + Send
                        + '_,
                >,
            > {
                Box::pin(async { panic!("unused") })
            }
        }
        let client = InferenceClient::new(NoKey).unwrap();
        let (events, _) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        check(&client, request(vec![]), events);
    }
    #[test]
    fn inference_done_requires_blank_line_even_after_complete_calls() {
        let tool = advertised("read");
        let call = json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"id","type":"function","function":{"name":tool_alias(&tool.key),"arguments":"{}"}}]},"finish_reason":"tool_calls"}]});
        let complete_call = format!("data: {call}\n\n");
        for tail in [
            "data: [DONE]\n",
            "data: [DONE]\r\n",
            "data: [DONE]\n: still pending\n",
        ] {
            let mut decoder = StreamDecoder::new(std::slice::from_ref(&tool)).unwrap();
            decoder.push(complete_call.as_bytes()).unwrap();
            decoder.push(tail.as_bytes()).unwrap();
            assert!(matches!(decoder.finish(), Err(CoreError::Protocol)));
        }
        let mut decoder = StreamDecoder::new(std::slice::from_ref(&tool)).unwrap();
        decoder.push(complete_call.as_bytes()).unwrap();
        decoder.push(b"data: [DONE]\n").unwrap();
        decoder.push(b"\n").unwrap();
        assert_eq!(decoder.finish().unwrap().calls.len(), 1);
        let mut decoder = StreamDecoder::new(std::slice::from_ref(&tool)).unwrap();
        decoder.push(complete_call.as_bytes()).unwrap();
        decoder.push(b"data: [DONE]\r").unwrap();
        decoder.push(b"\n\r").unwrap();
        decoder.push(b"\n").unwrap();
        assert_eq!(decoder.finish().unwrap().calls.len(), 1);
    }
    #[test]
    fn inference_done_rejects_trailing_nonempty_event_and_accepts_crlf_metadata() {
        let completed = "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\r\n\r\n: keepalive\r\nevent: message\r\nid: 7\r\ndata: [DONE]\r\n\r\n";
        let mut valid = StreamDecoder::new(&[]).unwrap();
        for bytes in completed.as_bytes().chunks(3) {
            valid.push(bytes).unwrap();
        }
        assert_eq!(valid.finish().unwrap().finish_reason, "stop");
        for tail in ["data: {}\n\n", "event: late\n\n", "  "] {
            let mut decoder = StreamDecoder::new(&[]).unwrap();
            decoder.push(completed.as_bytes()).unwrap();
            let pushed = decoder.push(tail.as_bytes());
            assert!(pushed.is_err() || matches!(decoder.finish(), Err(CoreError::Protocol)));
        }
    }
    #[test]
    fn inference_fragmented_utf8_and_sse_reassemble() {
        let mut decoder = StreamDecoder::new(&[]).unwrap();
        let payload = "data: {\"choices\":[{\"delta\":{\"content\":\"café 🦀\"},\"finish_reason\":null}]}\n\ndata: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
        let bytes = payload.as_bytes();
        let crab = bytes.windows(4).position(|x| x == "🦀".as_bytes()).unwrap();
        for part in [
            &bytes[..crab + 1],
            &bytes[crab + 1..crab + 3],
            &bytes[crab + 3..],
        ] {
            decoder.push(part).unwrap();
        }
        assert_eq!(decoder.finish().unwrap().text, "café 🦀");
    }
    #[test]
    fn inference_tool_json_not_dispatched_before_complete() {
        let tool = advertised("read");
        let mut d = StreamDecoder::new(std::slice::from_ref(&tool)).unwrap();
        let incomplete = format!(
            "data: {}\n\n",
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_a","type":"function","function":{"name":tool_alias(&tool.key),"arguments":"{\"path\":"}}]},"finish_reason":null}]})
        );
        d.push(incomplete.as_bytes()).unwrap();
        assert!(d.finish().is_err());
        let complete = format!("data: {}\n\ndata: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"tool_calls\"}}]}}\n\ndata: [DONE]\n\n", json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_a","type":"function","function":{"name":tool_alias(&tool.key),"arguments":r#"{"path":"x"}"#}}]},"finish_reason":null}]}));
        let mut d = StreamDecoder::new(&[tool]).unwrap();
        d.push(complete.as_bytes()).unwrap();
        let turn = d.finish().unwrap();
        assert_eq!(turn.calls[0].arguments_json, r#"{"path":"x"}"#);
        assert_eq!(turn.calls[0].provider_call_id, "call_a");
    }
    #[test]
    fn inference_multiple_tool_calls_keep_ids() {
        let a = advertised("same");
        let b = advertised("same");
        let mut d = StreamDecoder::new(&[a.clone(), b.clone()]).unwrap();
        let response = format!("data: {}\n\ndata: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"tool_calls\"}}]}}\n\ndata: [DONE]\n\n", json!({"choices":[{"delta":{"tool_calls":[{"index":1,"id":"B","type":"function","function":{"name":tool_alias(&b.key),"arguments":"{}"}},{"index":0,"id":"A","type":"function","function":{"name":tool_alias(&a.key),"arguments":"{}"}}]},"finish_reason":null}]}));
        d.push(response.as_bytes()).unwrap();
        let turn = d.finish().unwrap();
        assert_eq!(
            turn.calls
                .iter()
                .map(|c| c.provider_call_id.as_str())
                .collect::<Vec<_>>(),
            ["A", "B"]
        );
        assert_eq!(turn.calls[0].key, a.key);
        assert_eq!(turn.calls[1].key, b.key);
    }
    #[test]
    fn inference_unknown_alias_and_malformed_completion_rejected() {
        for data in [
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"x\",\"type\":\"function\",\"function\":{\"name\":\"foreign\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"x\",\"type\":\"function\",\"function\":{\"name\":\"tool_0\",\"arguments\":\"{\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n\n",
        ] { let mut d = StreamDecoder::new(&[advertised("read")]).unwrap(); let _ = d.push(data.as_bytes()); assert!(d.finish().is_err()); }
    }
    #[tokio::test(start_paused = true)]
    async fn inference_backpressured_events_obey_caller_deadline() {
        use crate::model::core::CredentialRef;
        use crate::net::SecretValue;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (sent_tx, sent_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0; 4096];
            let _ = socket.read(&mut buf).await.unwrap();
            let body = "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"},\"finish_reason\":null}]}\n\ndata: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            sent_tx.send(()).unwrap();
        });
        struct NoKey;
        impl CredentialProvider for NoKey {
            fn resolve(
                &self,
                _: CredentialRef,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = CoreResult<SecretValue>> + Send + '_>,
            > {
                Box::pin(async { panic!("unexpected") })
            }
        }
        let mut r = request(vec![]);
        r.endpoint.base_url = format!("http://{addr}/v1");
        r.endpoint.allow_http = true;
        r.deadline = Instant::now() + std::time::Duration::from_secs(10);
        let (events, _receiver) = mpsc::channel(1);
        events
            .try_send(InferenceEvent::Text("occupied".into()))
            .unwrap();
        let task = tokio::spawn(async move {
            InferenceClient::new(NoKey)
                .unwrap()
                .stream_turn(r, events, Cancellation::new())
                .await
        });
        sent_rx.await.unwrap();
        tokio::task::yield_now().await;
        tokio::time::advance(std::time::Duration::from_secs(10)).await;
        assert_eq!(task.await.unwrap(), Err(CoreError::Cancelled));
        server.await.unwrap();
    }
    #[tokio::test]
    async fn inference_probe_no_call_is_inconclusive_with_required_choice() {
        use crate::model::core::CredentialRef;
        use crate::net::SecretValue;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        struct NoKey;
        impl CredentialProvider for NoKey {
            fn resolve(
                &self,
                _: CredentialRef,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = CoreResult<SecretValue>> + Send + '_>,
            > {
                Box::pin(async { panic!("unexpected") })
            }
        }
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![0; 4096];
            let n = socket.read(&mut bytes).await.unwrap();
            let request = String::from_utf8_lossy(&bytes[..n]);
            assert!(request.contains("\"tool_choice\":\"required\""));
            let body = "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        let mut req = request(vec![]);
        req.endpoint.base_url = format!("http://{addr}/v1");
        req.endpoint.allow_http = true;
        let (events, mut receiver) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        assert_eq!(
            InferenceClient::new(NoKey)
                .unwrap()
                .probe_tools(req, events, Cancellation::new())
                .await
                .unwrap(),
            ToolSupport::Inconclusive
        );
        assert_eq!(
            receiver.try_recv(),
            Ok(InferenceEvent::Text("hello".into()))
        );
        assert!(receiver.try_recv().is_err());
        server.await.unwrap();
    }
    #[tokio::test]
    async fn inference_probe_full_or_closed_channel_does_not_lose_unsupported_diagnostic() {
        use crate::model::core::CredentialRef;
        use crate::net::SecretValue;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        struct NoKey;
        impl CredentialProvider for NoKey {
            fn resolve(
                &self,
                _: CredentialRef,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = CoreResult<SecretValue>> + Send + '_>,
            > {
                Box::pin(async { panic!("unexpected") })
            }
        }
        for close in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = [0; 4096];
                let _ = socket.read(&mut bytes).await.unwrap();
                let body = r#"{"error":{"code":"unsupported_parameter","param":"tools"}}"#;
                socket.write_all(format!("HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            });
            let mut req = request(vec![]);
            req.endpoint.base_url = format!("http://{addr}/v1");
            req.endpoint.allow_http = true;
            let (events, receiver) = mpsc::channel(1);
            if close {
                drop(receiver);
            } else {
                events
                    .try_send(InferenceEvent::Text("occupied".into()))
                    .unwrap();
            }
            assert_eq!(
                InferenceClient::new(NoKey)
                    .unwrap()
                    .probe_tools(req, events, Cancellation::new())
                    .await
                    .unwrap(),
                ToolSupport::Unsupported
            );
            server.await.unwrap();
        }
    }
    #[tokio::test]
    async fn inference_probe_does_not_turn_auth_or_rate_limit_into_chat_only() {
        use crate::model::core::CredentialRef;
        use crate::net::SecretValue;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        struct NoKey;
        impl CredentialProvider for NoKey {
            fn resolve(
                &self,
                _: CredentialRef,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = CoreResult<SecretValue>> + Send + '_>,
            > {
                Box::pin(async { panic!("unexpected") })
            }
        }
        for (status, expected) in [
            (401, CoreError::Authentication),
            (429, CoreError::RateLimited),
            (400, CoreError::ToolUnsupported),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0; 4096];
                let _ = socket.read(&mut buf).await.unwrap();
                let body = r#"{"error":{"code":"unsupported_parameter","param":"tools"}}"#;
                socket.write_all(format!("HTTP/1.1 {status} Error\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            });
            let mut r = request(vec![]);
            r.endpoint.base_url = format!("http://{addr}/v1");
            r.endpoint.allow_http = true;
            let (sender, mut receiver) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
            let result = InferenceClient::new(NoKey)
                .unwrap()
                .probe_tools(r, sender, Cancellation::new())
                .await;
            if expected == CoreError::ToolUnsupported {
                assert_eq!(result, Ok(ToolSupport::Unsupported));
                assert!(receiver.try_recv().is_err());
            } else {
                assert_eq!(result, Err(expected));
                assert!(receiver.try_recv().is_err());
            }
            server.await.unwrap();
        }
    }
    #[tokio::test]
    async fn inference_loopback_streams_partial_then_validated_turn() {
        use crate::model::core::CredentialRef;
        use crate::net::SecretValue;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let tool = advertised("read");
        let alias = tool_alias(&tool.key);
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![0; 4096];
            let n = socket.read(&mut bytes).await.unwrap();
            let incoming = String::from_utf8_lossy(&bytes[..n]);
            assert!(incoming.starts_with("POST /custom/v1/chat/completions HTTP/1.1"));
            assert!(incoming.contains("manual-id"));
            assert!(incoming.contains(&alias));
            let body = format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"hello\"}},\"finish_reason\":null}}]}}\n\ndata: {}\n\ndata: [DONE]\n\n", json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"id1","type":"function","function":{"name":alias,"arguments":"{}"}}]},"finish_reason":"tool_calls"}]}));
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n",body.len()).as_bytes()).await.unwrap();
        });
        struct NoKey;
        impl CredentialProvider for NoKey {
            fn resolve(
                &self,
                _: CredentialRef,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = CoreResult<SecretValue>> + Send + '_>,
            > {
                Box::pin(async { panic!("not needed") })
            }
        }
        let mut r = request(vec![tool]);
        r.endpoint.base_url = format!("http://{addr}/custom/v1");
        r.endpoint.allow_http = true;
        let (events, mut receiver) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let turn = InferenceClient::new(NoKey)
            .unwrap()
            .stream_turn(r, events, Cancellation::new())
            .await
            .unwrap();
        assert_eq!(
            receiver.recv().await,
            Some(InferenceEvent::Text("hello".into()))
        );
        assert_eq!(turn.calls.len(), 1);
        assert_eq!(turn.calls[0].provider_call_id, "id1");
        server.await.unwrap();
    }
    #[test]
    fn inference_alias_fits_provider_name_limit_and_charset() {
        let tool = advertised("read/\u{1f980}");
        let alias = tool_alias(&tool.key);
        assert_eq!(alias.len(), 37);
        assert!(alias.starts_with("tool_"));
        assert!(alias[5..]
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
        assert_eq!(alias, tool_alias(&tool.key));
        let value: Value =
            serde_json::from_slice(&request(vec![tool.clone()]).payload().unwrap()).unwrap();
        assert_eq!(value["tools"][0]["function"]["name"], alias);
    }
    #[test]
    fn inference_duplicate_advertised_key_is_rejected_before_routing() {
        let tool = advertised("read");
        let request = request(vec![tool.clone(), tool.clone()]);
        assert_eq!(request.validate(), Err(CoreError::InvalidInput));
        assert!(matches!(
            StreamDecoder::new(&request.tools),
            Err(CoreError::InvalidInput)
        ));
    }
    #[test]
    fn inference_tool_alias_stable_across_reorder_and_removal() {
        let a = advertised("same");
        let b = advertised("same");
        let first = request(vec![a.clone(), b.clone()]).payload().unwrap();
        let reordered = request(vec![b.clone(), a.clone()]).payload().unwrap();
        let removed = request(vec![b.clone()]).payload().unwrap();
        let alias_for = |bytes: &[u8], key: &ToolKey| -> String {
            let value: Value = serde_json::from_slice(bytes).unwrap();
            let descriptions = value["tools"].as_array().unwrap();
            let index = if *key == a.key { 0 } else { 1 };
            descriptions
                .iter()
                .map(|v| v["function"]["name"].as_str().unwrap())
                .nth(index)
                .unwrap()
                .to_owned()
        };
        let a_alias = alias_for(&first, &a.key);
        let b_alias = alias_for(&first, &b.key);
        assert_ne!(a_alias, b_alias);
        let swapped: Value = serde_json::from_slice(&reordered).unwrap();
        let solo: Value = serde_json::from_slice(&removed).unwrap();
        assert_eq!(swapped["tools"][1]["function"]["name"], a_alias);
        assert_eq!(swapped["tools"][0]["function"]["name"], b_alias);
        assert_eq!(solo["tools"][0]["function"]["name"], b_alias);
    }
    #[test]
    fn inference_duplicate_argument_keys_never_return_calls() {
        let tool = advertised("read");
        for arguments in [r#"{"x":1,"x":2}"#, r#"{"nested":{"x":1,"x":2}}"#] {
            let frame = format!(
                "data: {}\n\ndata: [DONE]\n\n",
                json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"id","type":"function","function":{"name":"tool_0","arguments":arguments}}]},"finish_reason":"tool_calls"}]})
            );
            let mut decoder = StreamDecoder::new(std::slice::from_ref(&tool)).unwrap();
            decoder.push(frame.as_bytes()).unwrap();
            assert!(matches!(decoder.finish(), Err(CoreError::Protocol)));
        }
    }
    #[test]
    fn inference_history_rejects_orphan_tool_result_and_invalid_call() {
        let mut r = request(vec![]);
        r.messages
            .push(ChatMessage::tool_result("missing", "result"));
        assert_eq!(r.validate(), Err(CoreError::InvalidInput));
        r.messages.pop();
        let tool = advertised("read");
        r.messages.push(ChatMessage::assistant_with_calls(
            "",
            vec![ToolCall {
                key: tool.key,
                provider_call_id: "id".into(),
                arguments_json: r#"{"x":1,"x":2}"#.into(),
            }],
        ));
        assert_eq!(r.validate(), Err(CoreError::InvalidInput));
    }
    #[test]
    fn inference_assistant_call_tool_result_history_serializes_without_current_tool() {
        let tool = advertised("read");
        let call = ToolCall {
            key: tool.key.clone(),
            provider_call_id: "call-27".into(),
            arguments_json: r#"{"path":"a"}"#.into(),
        };
        let mut r = request(vec![]);
        r.messages = vec![
            ChatMessage::user("read it"),
            ChatMessage::assistant_with_calls("", vec![call.clone()]),
            ChatMessage::tool_result("call-27", "contents"),
            ChatMessage::user("what next?"),
        ];
        let payload: Value = serde_json::from_slice(&r.payload().unwrap()).unwrap();
        assert!(payload.get("tools").is_none());
        assert_eq!(payload["messages"][1]["role"], "assistant");
        assert_eq!(payload["messages"][1]["tool_calls"][0]["id"], "call-27");
        assert_eq!(payload["messages"][1]["tool_calls"][0]["type"], "function");
        assert_eq!(
            payload["messages"][1]["tool_calls"][0]["function"]["name"],
            tool_alias(&tool.key)
        );
        assert_eq!(
            payload["messages"][1]["tool_calls"][0]["function"]["arguments"],
            r#"{"path":"a"}"#
        );
        assert_eq!(payload["messages"][2]["role"], "tool");
        assert_eq!(payload["messages"][2]["tool_call_id"], "call-27");
        assert_eq!(payload["messages"][2]["content"], "contents");
    }
    #[test]
    fn inference_manual_model_without_discovery_and_settings_validation() {
        assert!(request(vec![]).validate().is_ok());
        let mut r = request(vec![]);
        r.endpoint.temperature = f32::NAN;
        assert!(r.validate().is_err());
        r.endpoint.temperature = 0.2;
        r.endpoint.context_tokens = 0;
        assert!(r.validate().is_err());
        r.endpoint.context_tokens = 1024;
        r.endpoint.max_output_tokens = 1025;
        assert!(r.validate().is_err());
        r.endpoint.context_tokens = u32::MAX;
        assert!(r.validate().is_err());
    }

    // Failure-step tagging.
    struct NoKeyNoted;
    impl CredentialProvider for NoKeyNoted {
        fn resolve(
            &self,
            _: crate::model::core::CredentialRef,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = CoreResult<crate::net::SecretValue>> + Send + '_>,
        > {
            Box::pin(async { Err(CoreError::Authentication) })
        }
    }
    async fn one_shot(status: u16, body: &'static str) -> (String, tokio::task::JoinHandle<()>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0; 4096];
            let _ = socket.read(&mut buf).await.unwrap();
            let head = format!(
                "HTTP/1.1 {status} Error\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(head.as_bytes()).await.unwrap();
            socket.write_all(body.as_bytes()).await.unwrap();
        });
        (format!("http://{addr}/v1"), task)
    }
    #[tokio::test]
    async fn discovery_failure_is_tagged_with_its_step() {
        let (base, server) = one_shot(401, r#"{"error":{"message":"bad key"}}"#).await;
        let mut endpoint = endpoint();
        endpoint.base_url = base;
        endpoint.allow_http = true;
        let (result, detail) = crate::failure::scope(async {
            InferenceClient::new(NoKeyNoted)
                .unwrap()
                .discover_models_with(
                    &endpoint,
                    Instant::now() + std::time::Duration::from_secs(5),
                    Cancellation::new(),
                )
                .await
        })
        .await;
        assert_eq!(result, Err(CoreError::Authentication));
        let detail = detail.expect("detail");
        assert_eq!(detail.step, crate::failure::FailureStep::DiscoverModels);
        assert_eq!(detail.status, Some(401));
        assert_eq!(detail.reason.as_deref(), Some("bad key"));
        server.await.unwrap();
    }
    #[tokio::test]
    async fn chat_failure_is_tagged_with_its_step() {
        let (base, server) = one_shot(500, "{}").await;
        let mut r = request(vec![]);
        r.endpoint.base_url = base;
        r.endpoint.allow_http = true;
        let (sender, _receiver) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let (result, detail) = crate::failure::scope(async {
            InferenceClient::new(NoKeyNoted)
                .unwrap()
                .stream_turn(r, sender, Cancellation::new())
                .await
        })
        .await;
        assert!(result.is_err());
        let detail = detail.expect("detail");
        assert_eq!(detail.step, crate::failure::FailureStep::ChatRequest);
        assert_eq!(detail.status, Some(500));
        server.await.unwrap();
    }
}
