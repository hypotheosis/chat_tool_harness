use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub Uuid);
        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}
uuid_id!(ConversationId);
uuid_id!(MessageId);
uuid_id!(ProfileId);
uuid_id!(EndpointId);
uuid_id!(ServerId);
uuid_id!(RunId);
uuid_id!(AttemptId);
uuid_id!(AttachmentId);
uuid_id!(ArtifactId);
uuid_id!(CredentialRef);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolServer {
    Mcp(ServerId),
    LocalFiles,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToolKey {
    pub profile_id: ProfileId,
    pub server: ToolServer,
    pub name: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDefinition {
    pub key: ToolKey,
    pub(crate) schema_json: String,
    pub(crate) schema_digest: String,
    pub(crate) destination_digest: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub key: ToolKey,
    pub provider_call_id: String,
    pub arguments_json: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalBinding {
    pub attempt_id: AttemptId,
    pub key: ToolKey,
    pub provider_call_id: String,
    pub definition_digest: String,
    pub arguments_digest: String,
    pub arguments_json: String,
    pub schema_json: String,
    pub destination_digest: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionRule {
    Ask,
    Allow,
    Block,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Authorization {
    Allow,
    Ask(Box<ApprovalBinding>),
    Block,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EndpointConfig {
    pub id: EndpointId,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub context_tokens: u32,
    pub max_output_tokens: u32,
    pub temperature: f32,
    pub allow_http: bool,
    pub credential: Option<CredentialRef>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerConfig {
    pub id: ServerId,
    pub name: String,
    pub url: String,
    pub allow_http: bool,
    pub secret_headers: Vec<(String, CredentialRef)>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunLimits {
    pub max_inference_requests: u32,
    pub max_tool_attempts: u32,
    pub active_timeout_ms: u64,
}
impl Default for RunLimits {
    fn default() -> Self {
        Self {
            max_inference_requests: 20,
            max_tool_attempts: 40,
            active_timeout_ms: 900_000,
        }
    }
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProfileMode {
    #[default]
    Agent,
    ChatOnly,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub mode: ProfileMode,
    pub id: ProfileId,
    pub name: String,
    pub endpoint_id: EndpointId,
    pub server_ids: Vec<ServerId>,
    pub instructions: String,
    pub limits: RunLimits,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileSnapshot {
    #[serde(default)]
    pub mode: ProfileMode,
    pub profile_id: ProfileId,
    pub revision: i64,
    pub name: String,
    pub endpoint: EndpointConfig,
    pub servers: Vec<ServerConfig>,
    pub instructions: String,
    pub limits: RunLimits,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunState {
    Running,
    WaitingApproval,
    Paused,
    Completed,
    Cancelled,
    Failed,
    Interrupted,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttemptState {
    Proposed,
    Denied,
    Approved,
    Dispatching,
    Succeeded,
    Failed,
    OutcomeUnknown,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttemptRecord {
    pub id: AttemptId,
    pub state: AttemptState,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: RunId,
    pub state: RunState,
    pub attempts: Vec<AttemptRecord>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversationSnapshot {
    pub id: ConversationId,
    pub profile: ProfileSnapshot,
    pub runs: Vec<RunRecord>,
    pub messages: Vec<crate::agent::context::HistoryMessage>,
    pub summaries: Vec<crate::agent::context::DerivedContext>,
}
