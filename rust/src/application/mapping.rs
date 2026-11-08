use super::dto::*;
use crate::{
    agent::context as ctx,
    error::{CoreError, CoreResult},
    model::core,
};

pub fn uuid(value: &str) -> CoreResult<uuid::Uuid> {
    uuid::Uuid::parse_str(value).map_err(|_| CoreError::InvalidInput)
}
impl TryFrom<AgentEndpointConfig> for core::EndpointConfig {
    type Error = CoreError;
    fn try_from(v: AgentEndpointConfig) -> CoreResult<Self> {
        Ok(Self {
            id: core::EndpointId(uuid(&v.id)?),
            name: v.name,
            base_url: v.base_url,
            model: v.model,
            context_tokens: v.context_tokens,
            max_output_tokens: v.max_output_tokens,
            temperature: v.temperature,
            allow_http: v.allow_http,
            credential: v
                .credential
                .map(|v| uuid(&v).map(core::CredentialRef))
                .transpose()?,
        })
    }
}
impl From<core::EndpointConfig> for AgentEndpointConfig {
    fn from(v: core::EndpointConfig) -> Self {
        Self {
            id: v.id.0.to_string(),
            name: v.name,
            base_url: v.base_url,
            model: v.model,
            context_tokens: v.context_tokens,
            max_output_tokens: v.max_output_tokens,
            temperature: v.temperature,
            allow_http: v.allow_http,
            credential: v.credential.map(|v| v.0.to_string()),
        }
    }
}
impl TryFrom<AgentServerConfig> for core::ServerConfig {
    type Error = CoreError;
    fn try_from(v: AgentServerConfig) -> CoreResult<Self> {
        Ok(Self {
            id: core::ServerId(uuid(&v.id)?),
            name: v.name,
            url: v.url,
            allow_http: v.allow_http,
            secret_headers: v
                .secret_headers
                .into_iter()
                .map(|v| Ok((v.name, core::CredentialRef(uuid(&v.credential)?))))
                .collect::<CoreResult<_>>()?,
        })
    }
}
impl From<core::ServerConfig> for AgentServerConfig {
    fn from(v: core::ServerConfig) -> Self {
        Self {
            id: v.id.0.to_string(),
            name: v.name,
            url: v.url,
            allow_http: v.allow_http,
            secret_headers: v
                .secret_headers
                .into_iter()
                .map(|(name, id)| AgentSecretHeader {
                    name,
                    credential: id.0.to_string(),
                })
                .collect(),
        }
    }
}
impl From<AgentRunLimits> for core::RunLimits {
    fn from(v: AgentRunLimits) -> Self {
        Self {
            max_inference_requests: v.max_inference_requests,
            max_tool_attempts: v.max_tool_attempts,
            active_timeout_ms: v.active_timeout_ms,
        }
    }
}
impl From<core::RunLimits> for AgentRunLimits {
    fn from(v: core::RunLimits) -> Self {
        Self {
            max_inference_requests: v.max_inference_requests,
            max_tool_attempts: v.max_tool_attempts,
            active_timeout_ms: v.active_timeout_ms,
        }
    }
}
impl TryFrom<AgentProfile> for core::Profile {
    type Error = CoreError;
    fn try_from(v: AgentProfile) -> CoreResult<Self> {
        Ok(Self {
            id: core::ProfileId(uuid(&v.id)?),
            name: v.name,
            endpoint_id: core::EndpointId(uuid(&v.endpoint_id)?),
            server_ids: v
                .server_ids
                .into_iter()
                .map(|v| uuid(&v).map(core::ServerId))
                .collect::<CoreResult<_>>()?,
            mode: v.mode,
            instructions: v.instructions,
            limits: v.limits.into(),
        })
    }
}
impl From<core::Profile> for AgentProfile {
    fn from(v: core::Profile) -> Self {
        Self {
            id: v.id.0.to_string(),
            name: v.name,
            endpoint_id: v.endpoint_id.0.to_string(),
            server_ids: v.server_ids.into_iter().map(|v| v.0.to_string()).collect(),
            mode: v.mode,
            instructions: v.instructions,
            limits: v.limits.into(),
        }
    }
}
impl From<core::ProfileSnapshot> for AgentProfileSnapshot {
    fn from(v: core::ProfileSnapshot) -> Self {
        Self {
            inference_destination: crate::agent::context::disclosure_destination(&v.endpoint).ok(),
            profile_id: v.profile_id.0.to_string(),
            revision: v.revision,
            name: v.name,
            endpoint: v.endpoint.into(),
            servers: v.servers.into_iter().map(Into::into).collect(),
            mode: v.mode,
            instructions: v.instructions,
            limits: v.limits.into(),
        }
    }
}
impl TryFrom<AgentToolKey> for core::ToolKey {
    type Error = CoreError;
    fn try_from(v: AgentToolKey) -> CoreResult<Self> {
        Ok(Self {
            profile_id: core::ProfileId(uuid(&v.profile_id)?),
            server: v
                .server_id
                .map(|v| uuid(&v).map(core::ServerId).map(core::ToolServer::Mcp))
                .transpose()?
                .unwrap_or(core::ToolServer::LocalFiles),
            name: v.name,
        })
    }
}
impl From<core::ToolKey> for AgentToolKey {
    fn from(v: core::ToolKey) -> Self {
        Self {
            profile_id: v.profile_id.0.to_string(),
            server_id: match v.server {
                core::ToolServer::Mcp(v) => Some(v.0.to_string()),
                core::ToolServer::LocalFiles => None,
            },
            name: v.name,
        }
    }
}
impl TryFrom<AgentApprovalBinding> for core::ApprovalBinding {
    type Error = CoreError;
    fn try_from(v: AgentApprovalBinding) -> CoreResult<Self> {
        Ok(Self {
            attempt_id: core::AttemptId(uuid(&v.attempt_id)?),
            key: v.key.try_into()?,
            provider_call_id: v.provider_call_id,
            definition_digest: v.definition_digest,
            arguments_digest: v.arguments_digest,
            arguments_json: v.arguments_json,
            schema_json: v.schema_json,
            destination_digest: v.destination_digest,
        })
    }
}
impl From<core::ApprovalBinding> for AgentApprovalBinding {
    fn from(v: core::ApprovalBinding) -> Self {
        Self {
            attempt_id: v.attempt_id.0.to_string(),
            key: v.key.into(),
            provider_call_id: v.provider_call_id,
            definition_digest: v.definition_digest,
            arguments_digest: v.arguments_digest,
            arguments_json: v.arguments_json,
            schema_json: v.schema_json,
            destination_digest: v.destination_digest,
        }
    }
}
impl TryFrom<AgentContextSelection> for ctx::ContextSelection {
    type Error = CoreError;
    fn try_from(v: AgentContextSelection) -> CoreResult<Self> {
        Ok(Self {
            message_ids: v
                .message_ids
                .iter()
                .map(|v| uuid(v).map(core::MessageId))
                .collect::<CoreResult<_>>()?,
            summary_ids: v
                .summary_ids
                .iter()
                .map(|v| uuid(v).map(core::MessageId))
                .collect::<CoreResult<_>>()?,
        })
    }
}
impl From<ctx::ContextSelection> for AgentContextSelection {
    fn from(v: ctx::ContextSelection) -> Self {
        Self {
            message_ids: v.message_ids.iter().map(|v| v.0.to_string()).collect(),
            summary_ids: v.summary_ids.iter().map(|v| v.0.to_string()).collect(),
        }
    }
}
impl From<ctx::DerivedContext> for AgentDerivedContext {
    fn from(v: ctx::DerivedContext) -> Self {
        Self {
            id: v.id.0.to_string(),
            run_id: v.run_id.0.to_string(),
            text: v.text,
            selection: v.selection.into(),
            page_references: v
                .page_references
                .into_iter()
                .map(|v| AgentPageReference {
                    attachment_id: v.attachment_id.0.to_string(),
                    physical_page: v.physical_page,
                })
                .collect(),
            partial_coverage: v.partial_coverage,
            unresolved_attempts: v
                .unresolved_attempts
                .iter()
                .map(|v| v.0.to_string())
                .collect(),
        }
    }
}
impl From<ctx::ContextUsage> for AgentContextUsage {
    fn from(v: ctx::ContextUsage) -> Self {
        Self {
            estimation: v.estimation,
            estimated_input_tokens: v.estimated_input_tokens,
            output_reserve: v.output_reserve,
            window: v.window,
            raw_bytes: v.raw_bytes,
            raw_bytes_is_lower_bound: v.raw_bytes_is_lower_bound,
            provider: v.provider.map(|v| AgentProviderUsage {
                prompt_tokens: v.prompt_tokens,
                completion_tokens: v.completion_tokens,
            }),
            reason: v.reason,
        }
    }
}
impl From<ctx::RunContext> for AgentRunContext {
    fn from(v: ctx::RunContext) -> Self {
        Self {
            purpose: v.purpose,
            selection: v.selection.map(Into::into),
            selection_through_ordinal: v.selection_through_ordinal,
            usage: v.usage.map(Into::into),
            derived_entry: v.derived_entry.map(|v| v.0.to_string()),
            summary: v.summary.map(Into::into),
        }
    }
}
impl From<ctx::StoredCall> for AgentToolCall {
    fn from(v: ctx::StoredCall) -> Self {
        Self {
            key: v.key.into(),
            provider_call_id: v.provider_call_id,
            arguments_json: v.arguments_json,
            source_envelope: v.source_envelope.map(|v| v.0.to_string()),
        }
    }
}
impl From<ctx::HistoryMessage> for AgentMessage {
    fn from(v: ctx::HistoryMessage) -> Self {
        Self {
            id: v.id.0.to_string(),
            role: v.message.role,
            content: v.message.content,
            calls: v.message.calls.into_iter().map(Into::into).collect(),
            call_id: v.message.call_id,
            source_envelope: v.message.source_envelope.map(|v| v.0.to_string()),
        }
    }
}
impl From<core::ConversationSnapshot> for AgentConversation {
    fn from(v: core::ConversationSnapshot) -> Self {
        Self {
            id: v.id.0.to_string(),
            profile: v.profile.into(),
            messages: v.messages.into_iter().map(Into::into).collect(),
            summaries: v.summaries.into_iter().map(Into::into).collect(),
        }
    }
}
