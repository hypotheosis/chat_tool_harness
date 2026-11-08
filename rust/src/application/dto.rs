// Typed application DTOs. Identifiers are validated at the application boundary.
pub use crate::agent::context::{ContextChoiceReason, ContextPurpose};
pub use crate::inference::ToolSupport;
pub use crate::model::core::{AttemptState, PermissionRule, ProfileMode, RunState};
#[derive(Debug, Clone)]
pub struct AgentEndpointConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub context_tokens: u32,
    pub max_output_tokens: u32,
    pub temperature: f32,
    pub allow_http: bool,
    pub credential: Option<String>,
}
#[derive(Debug, Clone)]
pub struct AgentServerConfig {
    pub id: String,
    pub name: String,
    pub url: String,
    pub allow_http: bool,
    pub secret_headers: Vec<AgentSecretHeader>,
}
#[derive(Debug, Clone)]
pub struct AgentSecretHeader {
    pub name: String,
    pub credential: String,
}
#[derive(Debug, Clone)]
pub struct AgentRunLimits {
    pub max_inference_requests: u32,
    pub max_tool_attempts: u32,
    pub active_timeout_ms: u64,
}
#[derive(Debug, Clone)]
pub struct AgentProfile {
    pub mode: ProfileMode,
    pub id: String,
    pub name: String,
    pub endpoint_id: String,
    pub server_ids: Vec<String>,
    pub instructions: String,
    pub limits: AgentRunLimits,
}
#[derive(Debug, Clone)]
pub struct AgentProfileSnapshot {
    pub inference_origin: Option<String>,
    pub mode: ProfileMode,
    pub profile_id: String,
    pub revision: i64,
    pub name: String,
    pub endpoint: AgentEndpointConfig,
    pub servers: Vec<AgentServerConfig>,
    pub instructions: String,
    pub limits: AgentRunLimits,
}
#[derive(Debug, Clone)]
pub struct AgentToolKey {
    pub profile_id: String,
    pub server_id: Option<String>,
    pub name: String,
}
#[derive(Debug, Clone)]
pub struct AgentApprovalBinding {
    pub attempt_id: String,
    pub key: AgentToolKey,
    pub provider_call_id: String,
    pub definition_digest: String,
    pub arguments_digest: String,
    pub arguments_json: String,
    pub schema_json: String,
    pub destination_digest: String,
}
#[derive(Debug, Clone)]
pub struct AgentToolCall {
    pub key: AgentToolKey,
    pub provider_call_id: String,
    pub arguments_json: String,
    pub source_envelope: Option<String>,
}
#[derive(Debug, Clone)]
pub struct AgentMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub calls: Vec<AgentToolCall>,
    pub call_id: Option<String>,
    pub source_envelope: Option<String>,
}
#[derive(Debug, Clone)]
pub struct AgentContextSelection {
    pub message_ids: Vec<String>,
    pub summary_ids: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct AgentPageReference {
    pub attachment_id: String,
    pub physical_page: u32,
}
#[derive(Debug, Clone)]
pub struct AgentDerivedContext {
    pub id: String,
    pub run_id: String,
    pub text: String,
    pub selection: AgentContextSelection,
    pub page_references: Vec<AgentPageReference>,
    pub partial_coverage: bool,
    pub unresolved_attempts: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct AgentProviderUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}
#[derive(Debug, Clone)]
pub struct AgentContextUsage {
    pub estimation: crate::agent::context::TokenEstimation,
    pub estimated_input_tokens: u64,
    pub output_reserve: u32,
    pub window: u32,
    pub raw_bytes: u64,
    pub raw_bytes_is_lower_bound: bool,
    pub provider: Option<AgentProviderUsage>,
    pub reason: Option<ContextChoiceReason>,
}
#[derive(Debug, Clone)]
pub struct AgentRunContext {
    pub purpose: ContextPurpose,
    pub selection: Option<AgentContextSelection>,
    pub selection_through_ordinal: Option<i64>,
    pub usage: Option<AgentContextUsage>,
    pub derived_entry: Option<String>,
    pub summary: Option<AgentDerivedContext>,
}
#[derive(Debug, Clone)]
pub struct AgentAttempt {
    pub id: String,
    pub state: AttemptState,
    pub call: AgentToolCall,
    pub binding: Option<AgentApprovalBinding>,
}
#[derive(Debug, Clone)]
pub struct AgentRun {
    pub id: String,
    pub conversation_id: String,
    pub profile: AgentProfileSnapshot,
    pub state: RunState,
    pub inference_count: u32,
    pub attempt_count: u32,
    pub active_ms: u64,
    pub partial_text: String,
    /// One-line display text: the failure reason, or the code's text.
    pub failure: Option<String>,
    pub failure_code: Option<crate::error::CoreError>,
    pub failure_detail: Option<AgentFailureDetail>,
    pub attempts: Vec<AgentAttempt>,
    pub context: AgentRunContext,
    /// The id of the user message that started a turn; none for summaries.
    pub input_message_id: Option<String>,
}
#[derive(Debug, Clone)]
pub struct AgentConversation {
    pub id: String,
    pub profile: AgentProfileSnapshot,
    /// Whether the app has acknowledged disclosure for `profile.inference_origin`.
    pub origin_acknowledged: bool,
    pub messages: Vec<AgentMessage>,
    pub summaries: Vec<AgentDerivedContext>,
}
#[derive(Debug, Clone)]
pub struct AgentToolRule {
    pub key: AgentToolKey,
    pub schema_json: String,
    pub schema_digest: String,
    pub destination_digest: String,
    pub rule: PermissionRule,
}
#[derive(Debug, Clone)]
pub struct AgentAppSnapshot {
    pub artifacts: Vec<AgentArtifact>,
    pub retention_limit_bytes: u64,
    pub sequence: u64,
    pub endpoints: Vec<AgentEndpointConfig>,
    pub servers: Vec<AgentServerConfig>,
    pub profiles: Vec<AgentProfile>,
    pub conversations: Vec<AgentConversation>,
    pub runs: Vec<AgentRun>,
    pub rules: Vec<AgentToolRule>,
}
#[derive(Debug, Clone)]
pub struct AgentAppEvent {
    pub sequence: u64,
    pub conversation_id: String,
}
#[derive(Debug, Clone)]
pub struct AgentCommandReply {
    pub history: Vec<AgentConversationSummary>,
    pub export_bundle: Option<AgentExportBundle>,
    pub models: Vec<String>,
    pub tool_support: Option<ToolSupport>,
    pub discovered_tools: Vec<String>,
    pub discovered_rules: Vec<AgentToolRule>,
    pub id: Option<String>,
    pub artifact: Option<AgentArtifactContent>,
}
#[derive(Debug, Clone)]
pub struct AgentArtifact {
    pub id: String,
    pub conversation_id: String,
    pub name: String,
    pub bytes: u64,
}
#[derive(Debug, Clone)]
pub struct AgentArtifactContent {
    pub summary: AgentArtifact,
    pub text: String,
    pub private_path: String,
}
#[derive(Debug, Clone)]
pub struct AgentConversationSummary {
    pub id: String,
    pub title: String,
    pub has_unknown_outcomes: bool,
}
#[derive(Debug, Clone)]
pub struct AgentExportBundle {
    pub transcript: AgentArtifact,
}
#[derive(Debug, Clone)]
pub enum AppCommand {
    SearchConversations {
        query: String,
        offset: u32,
        limit: u32,
    },
    RenameConversation {
        conversation_id: String,
        title: String,
    },
    DeleteConversation {
        conversation_id: String,
        confirmed_unknown: bool,
    },
    ExportTranscript {
        conversation_id: String,
        include_tool_payloads: bool,
    },
    DiscoverModels {
        endpoint_id: String,
    },
    ProbeEndpoint {
        endpoint_id: String,
    },
    DiscoverServer {
        profile_id: String,
        server_id: String,
    },
    CreateArtifact {
        conversation_id: String,
        name: String,
        content: String,
    },
    ReadArtifact {
        conversation_id: String,
        artifact_id: String,
    },
    DeleteArtifact {
        conversation_id: String,
        artifact_id: String,
    },
    NewId,
    SaveEndpoint {
        endpoint: AgentEndpointConfig,
    },
    SaveServer {
        server: AgentServerConfig,
    },
    SaveProfile {
        profile: AgentProfile,
    },
    CreateConversation {
        profile_id: String,
    },
    ApplyProfile {
        conversation_id: String,
        profile_id: String,
    },
    SetRule {
        key: AgentToolKey,
        schema_digest: String,
        destination_digest: String,
        rule: PermissionRule,
    },
    StartTurn {
        conversation_id: String,
        text: String,
    },
    PlatformPause {
        run_id: String,
        generation: String,
    },
    PlatformStop {
        run_id: String,
        generation: String,
    },
    CancelRun {
        run_id: String,
    },
    ResumeRun {
        run_id: String,
    },
    Approve {
        binding: AgentApprovalBinding,
    },
    Deny {
        attempt_id: String,
    },
    SummarizeContext {
        conversation_id: String,
        selection: AgentContextSelection,
    },
    ApplyContextSelection {
        run_id: String,
        selection: AgentContextSelection,
    },
    AcknowledgeContextDisclosure {
        conversation_id: String,
        origin: String,
    },
}
/// What failed and where, sanitized for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFailureDetail {
    /// "inference" | "mcpServer" | "profile" | "credential"
    pub target_kind: String,
    pub target_name: Option<String>,
    /// The failed step's name in lowerCamelCase, for example "discoverModels".
    pub step: String,
    pub url: Option<String>,
    pub status: Option<u16>,
    pub reason: Option<String>,
}
/// A command failure: its kind plus the detail noted where it happened.
/// The detail is boxed to keep `Result<_, AgentFailure>` small.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFailure {
    pub code: crate::error::CoreError,
    pub detail: Option<Box<AgentFailureDetail>>,
}
