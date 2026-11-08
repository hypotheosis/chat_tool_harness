mod diagnostics;
pub mod dto;
mod mapping;
use diagnostics::{Diagnostic, DiagnosticTasks};

use crate::agent::{
    Coordinator, HostFuture, PlatformRunHost, RuntimeConfig, RuntimeServices, SystemClock,
};
use crate::error::{CoreError, CoreResult};
use crate::model::{core, PlatformReply};
use crate::net::{Cancellation, CredentialProvider, SecretValue};
use crate::storage::Repository;
use dto::*;
use mapping::uuid;
use std::{future::Future, pin::Pin, sync::Arc};

pub enum CredentialReply {
    Value(String),
    Missing,
    Failed(String),
}
impl std::fmt::Debug for CredentialReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CredentialReply([REDACTED])")
    }
}
// Credential notes must never carry external text (platform messages, lookup
// errors): `failure::promote` treats the CREDENTIAL_MISSING_REASON reason on a
// ResolveCredential note as the marker for an absent credential. net.rs
// re-targets these notes at the endpoint or server holding the credential.
fn credential_value(reply: CredentialReply) -> CoreResult<SecretValue> {
    match reply {
        CredentialReply::Value(value) => Ok(SecretValue::new(value)),
        CredentialReply::Missing => {
            crate::failure::note(crate::failure::credential_missing());
            Err(CoreError::CredentialMissing)
        }
        CredentialReply::Failed(_) => {
            crate::failure::note(crate::failure::FailureDetail {
                target: crate::failure::FailureTarget::Credential,
                step: crate::failure::FailureStep::ResolveCredential,
                url: None,
                status: None,
                reason: Some("credential lookup failed".into()),
            });
            Err(CoreError::Authentication)
        }
    }
}
fn platform_result(reply: PlatformReply) -> CoreResult<()> {
    match reply {
        PlatformReply::Ok => Ok(()),
        PlatformReply::Failed(_) => Err(CoreError::UnavailableTransport),
    }
}
type CredentialFuture = Pin<Box<dyn Future<Output = CredentialReply> + Send>>;
type PlatformFuture = Pin<Box<dyn Future<Output = PlatformReply> + Send>>;
#[derive(Clone)]
struct Credentials(Arc<dyn Fn(String) -> CredentialFuture + Send + Sync>);
impl CredentialProvider for Credentials {
    fn resolve(
        &self,
        reference: core::CredentialRef,
    ) -> Pin<Box<dyn Future<Output = CoreResult<SecretValue>> + Send + '_>> {
        Box::pin(async move { credential_value((self.0)(reference.0.to_string()).await) })
    }
}
struct Host(Arc<dyn Fn(crate::model::RunHostRequest) -> PlatformFuture + Send + Sync>);
impl PlatformRunHost for Host {
    fn set_active(&self, active: crate::model::RunHostRequest) -> HostFuture {
        let future = (self.0)(active);
        Box::pin(async move { platform_result(future.await) })
    }
}
use crate::files::artifacts::{ArtifactStore, ArtifactSummary, RETENTION_BYTES};
impl From<ArtifactSummary> for AgentArtifact {
    fn from(v: ArtifactSummary) -> Self {
        Self {
            id: v.id.0.to_string(),
            conversation_id: v.conversation_id.0.to_string(),
            name: v.name,
            bytes: v.bytes,
        }
    }
}

pub struct Application {
    #[cfg(test)]
    registry_hook: std::sync::Mutex<Option<crate::mcp::RegistryHook>>,
    #[cfg(test)]
    diagnostic_finished: Arc<tokio::sync::Notify>,
    repository: Repository,
    artifacts: ArtifactStore,
    coordinator: Coordinator,
    credentials: Credentials,
    diagnostics: DiagnosticTasks,
    gate: tokio::sync::RwLock<bool>,
    pub(crate) closed: Cancellation,
}
impl Application {
    pub async fn open(
        data_dir: String,
        credentials: impl Fn(String) -> CredentialFuture + Send + Sync + 'static,
        host: impl Fn(crate::model::RunHostRequest) -> PlatformFuture + Send + Sync + 'static,
    ) -> CoreResult<Self> {
        if data_dir.trim().is_empty() {
            return Err(CoreError::InvalidInput);
        }
        let credentials = Credentials(Arc::new(credentials));
        let runtime_credentials = credentials.clone();
        let (repository, coordinator, artifacts) = blocking(move || {
            let path = std::path::Path::new(&data_dir);
            std::fs::create_dir_all(path).map_err(|_| CoreError::Storage)?;
            let repository = Repository::open(&path.join("agent.sqlite3"))?;
            let artifacts = ArtifactStore::open(repository.clone(), &path.join("artifacts"))?;
            let services = RuntimeServices::with_transports(
                runtime_credentials,
                Arc::new(Host(Arc::new(host))),
                Arc::new(artifacts.clone()),
                Arc::new(SystemClock),
            )?;
            let coordinator = Coordinator::open(
                RuntimeConfig {
                    repository: repository.clone(),
                },
                services,
            )?;
            Ok((repository, coordinator, artifacts))
        })
        .await?;
        let closed = Cancellation::new();
        Ok(Self {
            diagnostics: DiagnosticTasks::new(closed.clone()),
            #[cfg(test)]
            registry_hook: std::sync::Mutex::new(None),
            #[cfg(test)]
            diagnostic_finished: Arc::new(tokio::sync::Notify::new()),
            artifacts,
            credentials,
            repository,
            coordinator,
            gate: tokio::sync::RwLock::new(false),
            closed,
        })
    }
    /// [`Application::command`] with the failure detail noted where the
    /// command failed (connection tests, profile refresh, credential lookup).
    pub async fn command_with_detail(
        &self,
        command: AppCommand,
    ) -> Result<AgentCommandReply, AgentFailure> {
        let (result, detail) = crate::failure::scope(self.command(command)).await;
        result.map_err(|code| AgentFailure::new(code, detail))
    }
    pub async fn command(&self, command: AppCommand) -> CoreResult<AgentCommandReply> {
        let closed = self.gate.read().await;
        if *closed {
            return Err(CoreError::Conflict);
        }
        let repo = self.repository.clone();
        let store = self.artifacts.clone();
        let mut history = Vec::new();
        let mut export_bundle = None;
        let mut artifact = None;
        let mut discovered_rules = Vec::new();
        let id = match command {
            AppCommand::SearchConversations {
                query,
                offset,
                limit,
            } => {
                history = blocking(move || repo.search_conversations(&query, offset, limit))
                    .await?
                    .into_iter()
                    .map(|v| AgentConversationSummary {
                        id: v.id.0.to_string(),
                        title: v.title,
                        has_unknown_outcomes: v.has_unknown_outcomes,
                    })
                    .collect();
                None
            }
            AppCommand::RenameConversation {
                conversation_id,
                title,
            } => {
                blocking(move || {
                    repo.rename_conversation(core::ConversationId(uuid(&conversation_id)?), &title)
                })
                .await?;
                None
            }
            AppCommand::DeleteConversation {
                conversation_id,
                confirmed_unknown,
            } => {
                self.coordinator
                    .delete_conversation(
                        store,
                        core::ConversationId(uuid(&conversation_id)?),
                        confirmed_unknown,
                    )
                    .await?;
                None
            }
            AppCommand::ExportTranscript {
                conversation_id,
                include_tool_payloads,
            } => {
                let bundle = tokio::select! {
                    _ = self.closed.cancelled() => return Err(CoreError::Cancelled),
                    result = crate::history::export_transcript(&repo,&store,core::ConversationId(uuid(&conversation_id)?),crate::history::ExportOptions{include_tool_payloads},&self.credentials) => result?,
                };
                export_bundle = Some(AgentExportBundle {
                    transcript: bundle.transcript.into(),
                });
                None
            }
            AppCommand::DiscoverModels { endpoint_id } => {
                return self.diagnostic(Diagnostic::Models(endpoint_id)).await
            }
            AppCommand::ProbeEndpoint { endpoint_id } => {
                return self.diagnostic(Diagnostic::Probe(endpoint_id)).await
            }
            AppCommand::DiscoverServer {
                profile_id,
                server_id,
            } => {
                return self
                    .diagnostic(Diagnostic::Server(
                        core::ProfileId(uuid(&profile_id)?),
                        core::ServerId(uuid(&server_id)?),
                    ))
                    .await
            }
            AppCommand::CreateArtifact {
                conversation_id,
                name,
                content,
            } => Some(
                blocking(move || {
                    store
                        .create(
                            core::ConversationId(uuid(&conversation_id)?),
                            &name,
                            &content,
                        )
                        .map(|a| a.id.0.to_string())
                })
                .await?,
            ),
            AppCommand::ReadArtifact {
                conversation_id,
                artifact_id,
            } => {
                artifact = Some(
                    blocking(move || {
                        let (summary, text, private_path) = store.read(
                            core::ConversationId(uuid(&conversation_id)?),
                            core::ArtifactId(uuid(&artifact_id)?),
                        )?;
                        Ok(AgentArtifactContent {
                            summary: summary.into(),
                            text,
                            private_path,
                        })
                    })
                    .await?,
                );
                None
            }
            AppCommand::DeleteArtifact {
                conversation_id,
                artifact_id,
            } => {
                blocking(move || {
                    store.delete(
                        core::ConversationId(uuid(&conversation_id)?),
                        core::ArtifactId(uuid(&artifact_id)?),
                    )
                })
                .await?;
                None
            }
            AppCommand::NewId => Some(uuid::Uuid::new_v4().to_string()),
            AppCommand::SaveEndpoint { endpoint } => Some(
                blocking(move || {
                    repo.save_endpoint(endpoint.try_into()?)
                        .map(|v| v.0.to_string())
                })
                .await?,
            ),
            AppCommand::SaveServer { server } => Some(
                blocking(move || {
                    repo.save_server(server.try_into()?)
                        .map(|v| v.0.to_string())
                })
                .await?,
            ),
            AppCommand::SaveProfile { profile } => Some(
                blocking(move || {
                    repo.save_profile(profile.try_into()?)
                        .map(|v| v.0.to_string())
                })
                .await?,
            ),
            AppCommand::CreateConversation { profile_id } => Some(
                blocking(move || {
                    repo.create_conversation(core::ProfileId(uuid(&profile_id)?))
                        .map(|v| v.0.to_string())
                })
                .await?,
            ),
            AppCommand::ApplyProfile {
                conversation_id,
                profile_id,
            } => {
                blocking(move || {
                    repo.apply_profile(
                        core::ConversationId(uuid(&conversation_id)?),
                        core::ProfileId(uuid(&profile_id)?),
                    )
                })
                .await?;
                None
            }
            AppCommand::SetRule {
                key,
                schema_digest,
                destination_digest,
                rule,
            } => {
                let key: core::ToolKey = key.try_into()?;
                let observed_repo = repo.clone();
                let observed_key = key.clone();
                let observed =
                    blocking(move || observed_definition(&observed_repo, observed_key)).await;
                match (observed, &key.server) {
                    (Ok(_), _) => {}
                    (Err(CoreError::Conflict), core::ToolServer::Mcp(_)) => {
                        return self
                            .diagnostic(Diagnostic::Rule {
                                key,
                                schema: schema_digest,
                                destination: destination_digest,
                                rule,
                            })
                            .await
                    }
                    (Err(error), _) => return Err(error),
                }
                #[cfg(test)]
                let hook = self.registry_hook.lock().unwrap().clone();
                discovered_rules = diagnostics::set_observed_rule(
                    repo,
                    key,
                    schema_digest,
                    destination_digest,
                    rule,
                    #[cfg(test)]
                    hook,
                )
                .await?;
                None
            }
            AppCommand::StartTurn {
                conversation_id,
                text,
            } => Some(
                self.coordinator
                    .start_turn(core::ConversationId(uuid(&conversation_id)?), text)
                    .await?
                    .0
                    .to_string(),
            ),
            AppCommand::PlatformPause { run_id, generation } => {
                self.coordinator
                    .platform_pause(core::RunId(uuid(&run_id)?), uuid(&generation)?)
                    .await?;
                None
            }
            AppCommand::PlatformStop { run_id, generation } => {
                self.coordinator
                    .platform_stop(core::RunId(uuid(&run_id)?), uuid(&generation)?)
                    .await?;
                None
            }
            AppCommand::CancelRun { run_id } => {
                self.coordinator.cancel(core::RunId(uuid(&run_id)?)).await?;
                None
            }
            AppCommand::ResumeRun { run_id } => {
                self.coordinator.resume(core::RunId(uuid(&run_id)?)).await?;
                None
            }
            AppCommand::Approve { binding } => {
                let binding: core::ApprovalBinding = binding.try_into()?;
                let check = binding.clone();
                // A repeated approval needs no rediscovery (its session may be gone).
                if blocking(move || crate::permissions::already_approved(&repo, &check)).await? {
                    return Ok(AgentCommandReply::empty());
                }
                if matches!(binding.key.server, core::ToolServer::Mcp(_)) {
                    return self.diagnostic(Diagnostic::Approve(binding)).await;
                }
                self.coordinator.approve(binding).await?;
                None
            }
            AppCommand::Deny { attempt_id } => {
                self.coordinator
                    .deny(core::AttemptId(uuid(&attempt_id)?))
                    .await?;
                None
            }
            AppCommand::DecideApprovals {
                run_id,
                approve,
                deny,
            } => {
                let approve = approve
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<CoreResult<Vec<core::ApprovalBinding>>>()?;
                let deny = deny
                    .iter()
                    .map(|id| uuid(id).map(core::AttemptId))
                    .collect::<CoreResult<Vec<_>>>()?;
                self.decide_approvals(core::RunId(uuid(&run_id)?), approve, deny)
                    .await?;
                None
            }
            AppCommand::SetServerTrust { server_id, trusted } => {
                let server = core::ServerId(uuid(&server_id)?);
                blocking(move || crate::permissions::set_server_trust(&repo, server, trusted))
                    .await?;
                None
            }
            AppCommand::SummarizeContext {
                conversation_id,
                selection,
            } => Some(
                self.coordinator
                    .summarize_context(
                        core::ConversationId(uuid(&conversation_id)?),
                        selection.try_into()?,
                    )
                    .await?
                    .0
                    .to_string(),
            ),
            AppCommand::ApplyContextSelection { run_id, selection } => {
                self.coordinator
                    .apply_context_selection(core::RunId(uuid(&run_id)?), selection.try_into()?)
                    .await?;
                None
            }
            AppCommand::AcknowledgeContextDisclosure {
                conversation_id,
                origin,
            } => {
                self.coordinator
                    .acknowledge_context_disclosure(
                        core::ConversationId(uuid(&conversation_id)?),
                        crate::agent::context::DisclosureAcknowledgment { origin },
                    )
                    .await?;
                None
            }
        };
        Ok(AgentCommandReply {
            history,
            export_bundle,
            id,
            artifact,
            discovered_rules,
            models: Vec::new(),
            tool_support: None,
            discovered_tools: Vec::new(),
            skipped_tools: Vec::new(),
        })
    }
    /// Store every decision (one discovery per MCP server being approved, one
    /// transaction for all decisions), then resume the run once. The
    /// diagnostic lease is released before the resume.
    async fn decide_approvals(
        &self,
        run: core::RunId,
        approve: Vec<core::ApprovalBinding>,
        deny: Vec<core::AttemptId>,
    ) -> CoreResult<()> {
        let repo = self.repository.clone();
        let (check_approve, check_deny) = (approve.clone(), deny.clone());
        let repeated =
            blocking(move || crate::agent::already_decided(&repo, run, check_approve, check_deny))
                .await?;
        if !repeated {
            if approve
                .iter()
                .any(|b| matches!(b.key.server, core::ToolServer::Mcp(_)))
            {
                self.diagnostic(Diagnostic::Decide { run, approve, deny })
                    .await?;
            } else {
                self.coordinator.decide(run, approve, deny).await?;
            }
        }
        // Every decision is stored; resume only a run that is stopped. A run
        // that is already going (or finished) keeps the decisions as they are.
        let repo = self.repository.clone();
        let current = blocking(move || crate::agent::run_state(&repo, run)).await?;
        if !matches!(
            current,
            core::RunState::WaitingApproval | core::RunState::Paused | core::RunState::Interrupted
        ) {
            return Ok(());
        }
        self.coordinator.resume(run).await
    }
    pub async fn snapshot(&self) -> CoreResult<AgentAppSnapshot> {
        let closed = self.gate.read().await;
        if *closed {
            return Err(CoreError::Conflict);
        }
        let repo = self.repository.clone();
        let (sequence, endpoints, servers, profiles, conversations, rules) =
            blocking(move || configuration_snapshot(&repo)).await?;
        let runs = self
            .coordinator
            .snapshot()
            .await?
            .runs
            .into_iter()
            .map(|v| AgentRun {
                id: v.id.0.to_string(),
                conversation_id: v.conversation.0.to_string(),
                profile: v.profile.into(),
                state: v.state,
                inference_count: v.inference_count,
                attempt_count: v.attempt_count,
                active_ms: v.active_ms,
                partial_text: v.partial_text,
                failure: v.failure.as_ref().map(|f| f.display()),
                failure_code: v.failure.as_ref().map(|f| f.code.clone()),
                failure_detail: v.failure.and_then(|f| f.detail).map(Into::into),
                attempts: v
                    .attempts
                    .into_iter()
                    .map(|v| AgentAttempt {
                        id: v.id.0.to_string(),
                        state: v.state,
                        call: AgentToolCall {
                            key: v.call.key.into(),
                            provider_call_id: v.call.provider_call_id,
                            arguments_json: v.call.arguments_json,
                            source_envelope: None,
                        },
                        binding: v.binding.map(Into::into),
                    })
                    .collect(),
                context: v.context.into(),
                input_message_id: v.input_message_id,
            })
            .collect();
        let store = self.artifacts.clone();
        let artifacts = blocking(move || store.all())
            .await?
            .into_iter()
            .map(Into::into)
            .collect();
        Ok(AgentAppSnapshot {
            artifacts,
            retention_limit_bytes: RETENTION_BYTES,
            sequence,
            endpoints,
            servers,
            profiles,
            conversations,
            runs,
            rules,
        })
    }
    pub fn subscribe(&self, after: u64) -> crate::agent::EventSubscription {
        self.coordinator.subscribe(after)
    }
    pub async fn close(&self) -> CoreResult<()> {
        // Diagnostics hold a read gate during I/O; cancel them before joining that gate.
        self.closed.cancel();
        let mut closed = self.gate.write().await;
        *closed = true;
        let diagnostics = self.diagnostics.drain().await;
        let coordinator = self.coordinator.shutdown().await;
        diagnostics?;
        coordinator
    }
}
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> CoreResult<T> + Send + 'static,
) -> CoreResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|_| CoreError::Storage)?
}
fn mcp_rules(
    repo: &Repository,
    profile_id: core::ProfileId,
    server_id: core::ServerId,
) -> CoreResult<Vec<AgentToolRule>> {
    let (_, _, _, _, _, rules) = configuration_snapshot(repo)?;
    Ok(rules
        .into_iter()
        .filter(|r| {
            r.key.profile_id == profile_id.0.to_string()
                && r.key.server_id.as_deref() == Some(server_id.0.to_string().as_str())
        })
        .collect())
}
fn saved_config<T: serde::de::DeserializeOwned + Send + 'static>(
    repo: &Repository,
    table: &'static str,
    id: uuid::Uuid,
) -> CoreResult<T> {
    repo.with_connection(move |conn| {
        use rusqlite::OptionalExtension;
        let json: String = conn
            .query_row(
                &format!("SELECT payload FROM {table} WHERE id=?1"),
                [id.to_string()],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| CoreError::Storage)?
            .ok_or(CoreError::Conflict)?;
        serde_json::from_str(&json).map_err(|_| CoreError::Storage)
    })
}
fn saved_endpoint(repo: &Repository, id: &str) -> CoreResult<core::EndpointConfig> {
    let id = core::EndpointId(uuid(id)?);
    saved_config(repo, "endpoints", id.0)
}
fn diagnostic_mcp_error(error: crate::mcp::McpError) -> CoreError {
    use crate::mcp::McpError;
    match error {
        McpError::Authentication => CoreError::Authentication,
        McpError::InvalidInput => CoreError::InvalidInput,
        McpError::Limit => CoreError::Limit,
        McpError::Storage => CoreError::Storage,
        McpError::Conflict => CoreError::Conflict,
        McpError::Cancelled => CoreError::Cancelled,
        McpError::UnsupportedTransport | McpError::UnsupportedVersion => {
            CoreError::DiscoveryUnavailable
        }
        McpError::Protocol | McpError::RemoteProtocol(_) | McpError::ExpiredSession => {
            CoreError::Protocol
        }
        McpError::OutcomeUnknown | McpError::Unavailable => CoreError::UnavailableTransport,
    }
}
fn server_key(key: &core::ToolKey) -> String {
    match key.server {
        core::ToolServer::Mcp(v) => format!("mcp:{}", v.0),
        core::ToolServer::LocalFiles => "local-files".into(),
    }
}
fn observed_definition(repo: &Repository, key: core::ToolKey) -> CoreResult<core::ToolDefinition> {
    repo.with_connection(move |conn| {
        use rusqlite::OptionalExtension;
        conn.query_row("SELECT schema_json,schema_digest,destination_digest FROM observed_tool_definitions WHERE profile_id=?1 AND server_key=?2 AND tool_name=?3",rusqlite::params![key.profile_id.0.to_string(),server_key(&key),key.name],|r|Ok(core::ToolDefinition{key:key.clone(),schema_json:r.get(0)?,schema_digest:r.get(1)?,destination_digest:r.get(2)?})).optional().map_err(|_|CoreError::Storage)?.ok_or(CoreError::Conflict)
    })
}
type ConfigSnapshot = (
    u64,
    Vec<AgentEndpointConfig>,
    Vec<AgentServerConfig>,
    Vec<AgentProfile>,
    Vec<AgentConversation>,
    Vec<AgentToolRule>,
);
fn configuration_snapshot(repo: &Repository) -> CoreResult<ConfigSnapshot> {
    let (sequence,endpoints,servers,profiles,ids,rules)=repo.with_connection(|conn| {
        let sequence:i64=conn.query_row("SELECT coalesce(max(id),0) FROM events",[],|r|r.get(0)).map_err(|_|CoreError::Storage)?;
        fn values<T:serde::de::DeserializeOwned>(conn:&rusqlite::Connection,sql:&str)->CoreResult<Vec<T>> {
            let mut stmt=conn.prepare(sql).map_err(|_|CoreError::Storage)?;
            let rows=stmt.query_map([],|r|r.get::<_,String>(0)).map_err(|_|CoreError::Storage)?;
            rows.map(|r|serde_json::from_str(&r.map_err(|_|CoreError::Storage)?).map_err(|_|CoreError::Storage)).collect()
        }
        let endpoints=values::<core::EndpointConfig>(conn,"SELECT payload FROM endpoints ORDER BY rowid")?.into_iter().map(Into::into).collect();
        let servers=values::<core::ServerConfig>(conn,"SELECT payload FROM servers ORDER BY rowid")?.into_iter().map(|config| {
            // A server whose saved settings no longer form a valid destination is not trusted.
            let trusted = match crate::permissions::server_trusted_on(conn, config.id) {
                Ok(trusted) => trusted,
                Err(CoreError::InvalidInput) => false,
                Err(error) => return Err(error),
            };
            Ok(AgentServerConfig { trusted, ..config.into() })
        }).collect::<CoreResult<Vec<_>>>()?;
        let profiles=values::<core::Profile>(conn,"SELECT payload FROM profiles ORDER BY rowid")?.into_iter().map(Into::into).collect();
        let mut stmt=conn.prepare("SELECT id FROM conversations ORDER BY rowid").map_err(|_|CoreError::Storage)?;
        let ids=stmt.query_map([],|r|r.get::<_,String>(0)).map_err(|_|CoreError::Storage)?.map(|r|r.map_err(|_|CoreError::Storage)).collect::<CoreResult<Vec<_>>>()?;
        let mut stmt=conn.prepare("SELECT o.profile_id,o.server_key,o.tool_name,o.schema_json,o.schema_digest,o.destination_digest,CASE WHEN r.definition_digest=o.definition_digest THEN coalesce(r.rule,'Ask') ELSE 'Ask' END FROM observed_tool_definitions o LEFT JOIN tool_permission_rules r ON r.profile_id=o.profile_id AND r.server_key=o.server_key AND r.tool_name=o.tool_name ORDER BY o.rowid").map_err(|_|CoreError::Storage)?;
        let rules=stmt.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?))).map_err(|_|CoreError::Storage)?.map(|row| {
            let (profile,server,name,schema_json,schema_digest,destination_digest,rule)=row.map_err(|_|CoreError::Storage)?;
            Ok(AgentToolRule{key:AgentToolKey{profile_id:profile,server_id:server.strip_prefix("mcp:").map(str::to_string),name},schema_json,schema_digest,destination_digest,rule:match rule.as_str(){"Ask"=>PermissionRule::Ask,"Allow"=>PermissionRule::Allow,"Block"=>PermissionRule::Block,_=>return Err(CoreError::Storage)}})
        }).collect::<CoreResult<Vec<_>>>()?;
        Ok((u64::try_from(sequence).map_err(|_|CoreError::Storage)?,endpoints,servers,profiles,ids,rules))
    })?;
    let conversations = ids
        .into_iter()
        .map(|id| {
            let snapshot = repo.conversation(core::ConversationId(uuid(&id)?))?;
            // A misconfigured endpoint has no origin, so nothing can be acknowledged for it.
            let acknowledged =
                match crate::agent::context::endpoint_origin(&snapshot.profile.endpoint) {
                    Ok(origin) => crate::agent::origin_acknowledged(repo, &origin)?,
                    Err(_) => false,
                };
            Ok(AgentConversation::new(snapshot, acknowledged))
        })
        .collect::<CoreResult<_>>()?;
    Ok((sequence, endpoints, servers, profiles, conversations, rules))
}
#[cfg(test)]
mod diagnostic_tests;
#[cfg(test)]
mod tests;
