use super::*;
use crate::agent::DiagnosticLease;
use tokio::sync::{oneshot, watch};

pub(super) enum Diagnostic {
    Models(String),
    Probe(String),
    Server(core::ProfileId, core::ServerId),
    Approve(core::ApprovalBinding),
    Rule {
        key: core::ToolKey,
        schema: String,
        destination: String,
        rule: core::PermissionRule,
    },
}

// Completion receipts supervise already-admitted work, not a queue of pending diagnostics.
// Keeping receipts in the owner also makes a dropped shutdown waiter safe to retry.
pub(super) struct DiagnosticTasks {
    owner: Cancellation,
    tasks: std::sync::Mutex<Vec<watch::Receiver<bool>>>,
}
impl DiagnosticTasks {
    pub(super) fn new(owner: Cancellation) -> Self {
        Self {
            owner,
            tasks: std::sync::Mutex::new(Vec::new()),
        }
    }
    pub(super) fn track(&self, done: watch::Receiver<bool>) {
        let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        tasks.retain(|task| !*task.borrow());
        tasks.push(done);
    }
    pub(super) async fn drain(&self) -> CoreResult<()> {
        let tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let mut result = Ok(());
        for mut done in tasks {
            while !*done.borrow_and_update() {
                if done.changed().await.is_err() {
                    result = Err(CoreError::Storage);
                    break;
                }
            }
        }
        result
    }
}
impl Drop for DiagnosticTasks {
    fn drop(&mut self) {
        self.owner.cancel();
    }
}
struct CancelOnDrop(Cancellation);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

pub(super) struct Operation {
    repo: Repository,
    credentials: Credentials,
    cancel: Cancellation,
    deadline: tokio::time::Instant,
    #[cfg(test)]
    hook: Option<crate::mcp::RegistryHook>,
}
impl Operation {
    async fn run(&self, command: Diagnostic) -> CoreResult<AgentCommandReply> {
        let mut reply = AgentCommandReply {
            history: Vec::new(),
            export_bundle: None,
            id: None,
            artifact: None,
            models: Vec::new(),
            tool_support: None,
            discovered_tools: Vec::new(),
            skipped_tools: Vec::new(),
            discovered_rules: Vec::new(),
        };
        match command {
            Diagnostic::Models(id) => {
                let repo = self.repo.clone();
                let endpoint = blocking(move || saved_endpoint(&repo, &id)).await?;
                reply.models = crate::inference::InferenceClient::new(self.credentials.clone())?
                    .discover_models_with(&endpoint, self.deadline, self.cancel.clone())
                    .await?;
            }
            Diagnostic::Probe(id) => {
                let repo = self.repo.clone();
                let endpoint = blocking(move || saved_endpoint(&repo, &id)).await?;
                let request = crate::inference::InferenceRequest { endpoint, messages: vec![crate::inference::ChatMessage::user("Diagnostic: call the provided empty function. No real action will execute.")], tools: vec![], deadline: self.deadline };
                let (tx, mut rx) = tokio::sync::mpsc::channel(32);
                let client = crate::inference::InferenceClient::new(self.credentials.clone())?;
                let probe = client.probe_tools(request, tx, self.cancel.clone());
                let drain = async { while rx.recv().await.is_some() {} };
                let (result, ()) = tokio::join!(probe, drain);
                reply.tool_support = Some(result?);
            }
            Diagnostic::Server(profile, server) => {
                let (mut session, discovery) = self.discover(profile, server).await?;
                reply.discovered_tools = discovery.tools.into_iter().map(|t| t.key.name).collect();
                reply.skipped_tools = discovery
                    .diagnostics
                    .into_iter()
                    .map(|d| AgentSkippedTool {
                        name: d.name,
                        reason: d.reason,
                    })
                    .collect();
                let repo = self.repo.clone();
                let rules = if self.cancel.is_cancelled() {
                    Err(CoreError::Cancelled)
                } else {
                    blocking(move || mcp_rules(&repo, profile, server)).await
                };
                let closed = self.close(&mut session).await;
                reply.discovered_rules = rules?;
                closed?;
            }
            Diagnostic::Approve(binding) => {
                let server = match binding.key.server {
                    core::ToolServer::Mcp(id) => id,
                    _ => return Err(CoreError::InvalidInput),
                };
                let (mut session, _) = self.discover(binding.key.profile_id, server).await?;
                let repo = self.repo.clone();
                let approved = if self.cancel.is_cancelled() {
                    Err(CoreError::Cancelled)
                } else {
                    blocking(move || {
                        crate::permissions::approve(&repo, binding.attempt_id, &binding)
                    })
                    .await
                };
                let closed = self.close(&mut session).await;
                approved?;
                closed?;
            }
            Diagnostic::Rule {
                key,
                schema,
                destination,
                rule,
            } => {
                let server = match key.server {
                    core::ToolServer::Mcp(id) => id,
                    _ => return Err(CoreError::InvalidInput),
                };
                let (mut session, _) = self.discover(key.profile_id, server).await?;
                let rules = if self.cancel.is_cancelled() {
                    Err(CoreError::Cancelled)
                } else {
                    set_observed_rule(
                        self.repo.clone(),
                        key,
                        schema,
                        destination,
                        rule,
                        #[cfg(test)]
                        self.hook.clone(),
                    )
                    .await
                };
                let closed = self.close(&mut session).await;
                reply.discovered_rules = rules?;
                closed?;
            }
        }
        Ok(reply)
    }
    async fn discover(
        &self,
        profile: core::ProfileId,
        server: core::ServerId,
    ) -> CoreResult<(
        crate::mcp::McpSession<Credentials>,
        crate::mcp::DiscoveryResult,
    )> {
        let repo = self.repo.clone();
        let (config, repo) = blocking(move || {
            let p = saved_config::<core::Profile>(&repo, "profiles", profile.0)?;
            if !p.server_ids.contains(&server) {
                return Err(CoreError::Configuration);
            }
            let config = saved_config::<core::ServerConfig>(&repo, "servers", server.0)?;
            Ok((config, repo))
        })
        .await?;
        let mut session = crate::mcp::McpClient::new(self.credentials.clone())
            .map_err(diagnostic_mcp_error)?
            .connect(profile, config, repo, self.deadline, self.cancel.clone())
            .await
            .map_err(diagnostic_mcp_error)?;
        #[cfg(test)]
        {
            session.registry_hook = self.hook.clone();
        }
        // Never race/drop these awaits on cancellation: they join spawned registry work.
        match session.list_tools(self.deadline, self.cancel.clone()).await {
            Ok(result) => Ok((session, result)),
            Err(error) => {
                let _ = self.close(&mut session).await;
                Err(diagnostic_mcp_error(error))
            }
        }
    }
    async fn close(&self, session: &mut crate::mcp::McpSession<Credentials>) -> CoreResult<()> {
        // Caller cancellation must not cancel required registry invalidation/joining.
        // Remote DELETE is best effort within a separate bounded cleanup window.
        session
            .close(
                tokio::time::Instant::now() + std::time::Duration::from_secs(5),
                Cancellation::new(),
            )
            .await
            .map_err(diagnostic_mcp_error)
    }
}

pub(super) async fn set_observed_rule(
    repo: Repository,
    key: core::ToolKey,
    schema: String,
    destination: String,
    rule: core::PermissionRule,
    #[cfg(test)] hook: Option<crate::mcp::RegistryHook>,
) -> CoreResult<Vec<AgentToolRule>> {
    let read_repo = repo.clone();
    let read_key = key.clone();
    blocking(move || {
        #[cfg(test)]
        if let Some(hook) = hook {
            hook(crate::mcp::RegistryMutation::SetRule);
        }
        let definition = observed_definition(&repo, key)?;
        if definition.schema_digest != schema || definition.destination_digest != destination {
            return Err(CoreError::Conflict);
        }
        crate::permissions::set_rule(&repo, &definition, rule)
    })
    .await?;
    if let core::ToolServer::Mcp(server) = read_key.server {
        blocking(move || mcp_rules(&read_repo, read_key.profile_id, server)).await
    } else {
        Ok(Vec::new())
    }
}

impl Application {
    pub(super) async fn diagnostic(&self, command: Diagnostic) -> CoreResult<AgentCommandReply> {
        let lease: DiagnosticLease = self.coordinator.diagnostic_lease().await?;
        let cancel = self.closed.child_token();
        let _caller = CancelOnDrop(cancel.clone());
        let operation = Operation {
            repo: self.repository.clone(),
            credentials: self.credentials.clone(),
            cancel,
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(150),
            #[cfg(test)]
            hook: self.registry_hook.lock().unwrap().clone(),
        };
        let (send, receive) = oneshot::channel();
        let (done, completion) = watch::channel(false);
        self.diagnostics.track(completion);
        #[cfg(test)]
        let finished = self.diagnostic_finished.clone();
        // This worker owns resources/lease, never Application/Coordinator ownership.
        tokio::spawn(async move {
            let (result, detail) = crate::failure::scope(operation.run(command)).await;
            let detail = detail.filter(|_| result.is_err());
            let result = result
                .map_err(|code| crate::failure::promote(code, detail.as_ref()))
                .map_err(|code| (code, detail));
            drop(lease);
            let _ = done.send(true);
            #[cfg(test)]
            finished.notify_one();
            let _ = send.send(result);
        });
        // The worker runs in its own task; re-note its detail in the caller's scope.
        receive
            .await
            .map_err(|_| CoreError::Storage)?
            .map_err(|(code, detail)| {
                if let Some(detail) = detail {
                    crate::failure::note(detail);
                }
                code
            })
    }
}
