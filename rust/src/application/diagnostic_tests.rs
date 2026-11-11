use super::*;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, Notify};

struct RegistryHold {
    entered: Notify,
    released: std::sync::Mutex<bool>,
    wake: std::sync::Condvar,
    used: std::sync::atomic::AtomicUsize,
}
impl RegistryHold {
    fn install(app: &Application, target: crate::mcp::RegistryMutation) -> ReleaseRegistry {
        let hold = Arc::new(Self {
            entered: Notify::new(),
            released: std::sync::Mutex::new(false),
            wake: std::sync::Condvar::new(),
            used: std::sync::atomic::AtomicUsize::new(0),
        });
        let hook = hold.clone();
        *app.registry_hook.lock().unwrap() = Some(Arc::new(move |phase| {
            if phase != target {
                return;
            }
            let occurrence = hook.used.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            // list_tools first invalidates on entry; hold the second invalidation in close.
            if occurrence
                != if target == crate::mcp::RegistryMutation::Invalidate {
                    1
                } else {
                    0
                }
            {
                return;
            }
            hook.entered.notify_one();
            let mut released = hook.released.lock().unwrap();
            while !*released {
                released = hook.wake.wait(released).unwrap();
            }
        }));
        ReleaseRegistry(hold)
    }
}
struct ReleaseRegistry(Arc<RegistryHold>);
impl ReleaseRegistry {
    fn release(&self) {
        *self.0.released.lock().unwrap() = true;
        self.0.wake.notify_all();
    }
}
impl Drop for ReleaseRegistry {
    fn drop(&mut self) {
        self.release();
    }
}

async fn dropped_registry_case(phase: crate::mcp::RegistryMutation, fallback: bool) {
    let mut wire = Wire::open().await;
    let f = AppFixture::open(&wire.url).await;
    let command = if fallback {
        let app = f.app.clone();
        let command = f.discover();
        let initial = tokio::spawn(async move { app.command(command).await });
        wire.initialize(false).await;
        wire.list().await.rpc(tools(), false);
        let rule = initial
            .await
            .unwrap()
            .unwrap()
            .discovered_rules
            .into_iter()
            .find(|r| r.key.name == "write")
            .unwrap();
        AppCommand::SetRule {
            key: rule.key,
            schema_digest: rule.schema_digest,
            destination_digest: rule.destination_digest,
            rule: PermissionRule::Allow,
        }
    } else {
        f.discover()
    };
    // A previous awaited operation may have left a completion notification.
    std::future::poll_fn(|cx| {
        let mut notice = std::pin::pin!(f.app.diagnostic_finished.notified());
        let _ = notice.as_mut().poll(cx);
        std::task::Poll::Ready(())
    })
    .await;
    let hold = RegistryHold::install(&f.app, phase);
    let app = f.app.clone();
    let caller = tokio::spawn(async move { app.command(command).await });
    wire.initialize(false).await;
    wire.list().await.rpc(tools(), false);
    hold.0.entered.notified().await;
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    f.reject_while_held(&mut wire, f.start()).await;
    f.reject_while_held(&mut wire, f.discover()).await;
    hold.release();
    f.app.diagnostic_finished.notified().await;
    *f.app.registry_hook.lock().unwrap() = None;
    let run = f.app.command(f.start()).await.unwrap().id.unwrap();
    wire.initialize(false).await;
    wire.list().await.rpc(
        json!({"tools":[{"name":"new_write","inputSchema":{"type":"object"}}]}),
        false,
    );
    let inference = wire.next().await;
    assert!(inference.method.starts_with("POST /chat/completions "));
    // Observe the actual new registry after every diagnostic job/cleanup has joined.
    let snapshot = f.app.snapshot().await.unwrap();
    assert!(snapshot.rules.iter().any(|r| r.key.name == "new_write"));
    assert!(!snapshot.rules.iter().any(|r| r.key.name == "write"));
    f.app
        .command(AppCommand::CancelRun { run_id: run })
        .await
        .unwrap();
    drop(inference);
    f.app.close().await.unwrap();
}

#[tokio::test]
async fn r3_drop_publication_retains_lease_until_registry_cleanup() {
    dropped_registry_case(crate::mcp::RegistryMutation::Publish, false).await;
}
#[tokio::test]
async fn r3_drop_invalidation_retains_lease_until_registry_cleanup() {
    dropped_registry_case(crate::mcp::RegistryMutation::Invalidate, false).await;
}
#[tokio::test]
async fn r3_drop_fallback_publication_retains_lease() {
    dropped_registry_case(crate::mcp::RegistryMutation::Publish, true).await;
}
#[tokio::test]
async fn r3_drop_fallback_invalidation_retains_lease() {
    dropped_registry_case(crate::mcp::RegistryMutation::Invalidate, true).await;
}
#[tokio::test]
async fn r3_drop_fallback_rule_mutation_retains_lease() {
    dropped_registry_case(crate::mcp::RegistryMutation::SetRule, true).await;
}

#[tokio::test]
async fn r3_shutdown_drains_abandoned_registry_work() {
    for phase in [
        crate::mcp::RegistryMutation::Publish,
        crate::mcp::RegistryMutation::Invalidate,
    ] {
        let mut wire = Wire::open().await;
        let f = AppFixture::open(&wire.url).await;
        let hold = RegistryHold::install(&f.app, phase);
        let app = f.app.clone();
        let command = f.discover();
        let caller = tokio::spawn(async move { app.command(command).await });
        wire.initialize(false).await;
        wire.list().await.rpc(tools(), false);
        hold.0.entered.notified().await;
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        let mut close = Box::pin(f.app.close());
        let pending =
            std::future::poll_fn(|cx| std::task::Poll::Ready(close.as_mut().poll(cx).is_pending()))
                .await;
        assert!(
            pending,
            "shutdown returned before its abandoned registry worker/cleanup completed"
        );
        hold.release();
        close.await.unwrap();
        assert!(mcp_rules(
            &f.app.repository,
            core::ProfileId(uuid(&f.profile).unwrap()),
            core::ServerId(uuid(&f.server).unwrap())
        )
        .unwrap()
        .is_empty());
    }
}

struct Request {
    method: String,
    body: Value,
    reply: oneshot::Sender<String>,
}
impl Request {
    fn respond(self, status: u16, content_type: &str, headers: &str, body: String) {
        self.reply.send(format!("HTTP/1.1 {status} Fixture\r\nContent-Type: {content_type}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())).unwrap();
    }
    fn rpc(self, result: Value, session: bool) {
        let body = json!({"jsonrpc":"2.0", "id": self.body["id"], "result": result}).to_string();
        self.respond(
            200,
            "application/json",
            if session {
                "Mcp-Session-Id: diagnostic\r\n"
            } else {
                ""
            },
            body,
        );
    }
}
struct Wire {
    url: String,
    requests: mpsc::UnboundedReceiver<Request>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Wire {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Wire {
    async fn open() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (send, requests) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            let mut workers = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (mut socket, _) = accepted.unwrap(); let send = send.clone();
                        workers.spawn(async move {
                            let mut bytes = Vec::new();
                            let end = loop {
                                let mut buf = [0u8; 4096]; let n = socket.read(&mut buf).await.unwrap();
                                if n == 0 { return; } bytes.extend_from_slice(&buf[..n]);
                                if let Some(p) = bytes.windows(4).position(|w| w == b"\r\n\r\n") { break p + 4; }
                                assert!(bytes.len() < 65536);
                            };
                            let header = String::from_utf8(bytes[..end].to_vec()).unwrap();
                            let len = header.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|s| s.trim().parse::<usize>().unwrap())).unwrap_or(0);
                            while bytes.len() < end + len {
                                let mut buf = [0u8; 4096]; let n = socket.read(&mut buf).await.unwrap(); if n == 0 { return; } bytes.extend_from_slice(&buf[..n]);
                            }
                            let body = if len == 0 { Value::Null } else { serde_json::from_slice(&bytes[end..end+len]).unwrap() };
                            let (reply, receive) = oneshot::channel();
                            if send.send(Request { method: header.lines().next().unwrap().into(), body, reply }).is_err() { return; }
                            if let Ok(response) = receive.await { let _ = socket.write_all(response.as_bytes()).await; }
                        });
                    }
                    _ = workers.join_next(), if !workers.is_empty() => {}
                }
            }
        });
        Self {
            url,
            requests,
            task,
        }
    }
    async fn next(&mut self) -> Request {
        tokio::time::timeout(std::time::Duration::from_secs(10), self.requests.recv())
            .await
            .expect("fixture progress")
            .unwrap()
    }
    async fn initialize(&mut self, session: bool) {
        let request = self.next().await;
        assert_eq!(request.body["method"], "initialize");
        request.rpc(json!({"protocolVersion":"2025-11-25", "capabilities":{"tools":{}}, "serverInfo":{"name":"fixture","version":"1"}}), session);
        let request = self.next().await;
        assert_eq!(request.body["method"], "notifications/initialized");
        request.respond(202, "application/json", "", String::new());
    }
    async fn list(&mut self) -> Request {
        let request = self.next().await;
        assert_eq!(request.body["method"], "tools/list");
        request
    }
}
fn tools() -> Value {
    json!({"tools":[{"name":"write","inputSchema":{"type":"object","properties":{"marker":{"type":"string"}},"required":["marker"],"additionalProperties":false}}, {"name":"other","inputSchema":{"type":"object"}}]})
}
struct AppFixture {
    app: Arc<Application>,
    conversation: String,
    profile: String,
    server: String,
    endpoint: String,
    stopped: Arc<Notify>,
    _dir: tempfile::TempDir,
}
impl AppFixture {
    async fn open(url: &str) -> Self {
        Self::open_with(url, None).await
    }
    /// Opens with every credential resolving to `credential` (or missing).
    async fn open_with(url: &str, credential: Option<&'static str>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let stopped = Arc::new(Notify::new());
        let notice = stopped.clone();
        let app = Arc::new(
            Application::open(
                dir.path().to_string_lossy().into_owned(),
                move |_| {
                    Box::pin(async move {
                        match credential {
                            Some(value) => CredentialReply::Value(value.into()),
                            None => CredentialReply::Missing,
                        }
                    })
                },
                move |active| {
                    let notice = notice.clone();
                    Box::pin(async move {
                        if !active.active {
                            notice.notify_one();
                        }
                        PlatformReply::Ok
                    })
                },
            )
            .await
            .unwrap(),
        );
        let endpoint = core::EndpointId::new();
        let server = core::ServerId::new();
        let profile = core::ProfileId::new();
        app.repository
            .save_endpoint(core::EndpointConfig {
                id: endpoint,
                name: "e".into(),
                base_url: url.into(),
                model: "m".into(),
                context_tokens: 65536,
                max_output_tokens: 128,
                temperature: 0.0,
                allow_http: true,
                credential: None,
            })
            .unwrap();
        app.repository
            .save_server(core::ServerConfig {
                id: server,
                name: "s".into(),
                url: format!("{url}/mcp"),
                allow_http: true,
                secret_headers: vec![],
            })
            .unwrap();
        app.repository
            .save_profile(core::Profile {
                id: profile,
                name: "p".into(),
                endpoint_id: endpoint,
                server_ids: vec![server],
                mode: core::ProfileMode::Agent,
                instructions: String::new(),
                limits: core::RunLimits::default(),
            })
            .unwrap();
        let conversation = app
            .repository
            .create_conversation(profile)
            .unwrap()
            .0
            .to_string();
        Self {
            app,
            conversation,
            profile: profile.0.to_string(),
            server: server.0.to_string(),
            endpoint: endpoint.0.to_string(),
            stopped,
            _dir: dir,
        }
    }
    fn discover(&self) -> AppCommand {
        AppCommand::DiscoverServer {
            profile_id: self.profile.clone(),
            server_id: self.server.clone(),
        }
    }
    fn start(&self) -> AppCommand {
        AppCommand::StartTurn {
            conversation_id: self.conversation.clone(),
            text: "hello".into(),
        }
    }
    async fn reject_while_held(&self, wire: &mut Wire, command: AppCommand) {
        // An unexpected request proves admission leaked into real transport; no sleeps/poll counts.
        tokio::select! {
            result = self.app.command(command) => assert!(matches!(result, Err(CoreError::Conflict)), "expected explicit busy rejection: {result:?}"),
            request = wire.next() => panic!("busy admission opened a session/request: {}", request.method),
        }
    }
    async fn start_then_cancel(&self) {
        let run = self.app.command(self.start()).await.unwrap().id.unwrap();
        self.app
            .command(AppCommand::CancelRun { run_id: run })
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn r3_active_agent_excludes_diagnostics_without_revoking_observations() {
    let mut wire = Wire::open().await;
    let f = AppFixture::open(&wire.url).await;
    let run = f.app.command(f.start()).await.unwrap().id.unwrap();
    wire.initialize(false).await;
    wire.list().await.rpc(tools(), false);
    let inference = wire.next().await;
    assert!(inference.method.starts_with("POST /chat/completions "));
    let alias = inference.body["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| !t["function"]["parameters"]["properties"]["marker"].is_null())
        .unwrap()["function"]["name"]
        .as_str()
        .unwrap()
        .to_owned();
    f.reject_while_held(&mut wire, f.discover()).await;
    f.reject_while_held(
        &mut wire,
        AppCommand::DiscoverModels {
            endpoint_id: f.endpoint.clone(),
        },
    )
    .await;
    f.reject_while_held(
        &mut wire,
        AppCommand::ProbeEndpoint {
            endpoint_id: f.endpoint.clone(),
        },
    )
    .await;
    let snapshot = f.app.snapshot().await.unwrap();
    let other = snapshot
        .rules
        .into_iter()
        .find(|r| r.key.name == "other")
        .unwrap();
    f.app
        .command(AppCommand::SetRule {
            key: other.key,
            schema_digest: other.schema_digest,
            destination_digest: other.destination_digest,
            rule: PermissionRule::Block,
        })
        .await
        .unwrap();
    let chunk = json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call","type":"function","function":{"name":alias,"arguments":"{\"marker\":\"value\"}"}}]},"finish_reason":"tool_calls"}]});
    inference.respond(
        200,
        "text/event-stream",
        "",
        format!("data: {chunk}\n\ndata: [DONE]\n\n"),
    );
    f.stopped.notified().await;
    f.app.coordinator.shutdown().await.unwrap();
    let snapshot = f.app.coordinator.snapshot().await.unwrap();
    let result = snapshot
        .runs
        .iter()
        .find(|r| r.id.0.to_string() == run)
        .unwrap();
    assert_eq!(result.state, core::RunState::WaitingApproval);
    assert_eq!(result.attempts[0].state, core::AttemptState::Proposed);
    assert!(
        wire.requests.try_recv().is_err(),
        "no diagnostic or business RPC"
    );
    f.app.close().await.unwrap();
}

#[tokio::test]
async fn r3_diagnostic_lease_excludes_run_admission_through_cleanup_and_set_rule_fallback() {
    for fallback in [false, true] {
        let mut wire = Wire::open().await;
        let f = AppFixture::open(&wire.url).await;
        let command = if fallback {
            let app = f.app.clone();
            let command = f.discover();
            let discovery = tokio::spawn(async move { app.command(command).await });
            wire.initialize(false).await;
            wire.list().await.rpc(tools(), false);
            let rule = discovery
                .await
                .unwrap()
                .unwrap()
                .discovered_rules
                .into_iter()
                .find(|r| r.key.name == "write")
                .unwrap();
            AppCommand::SetRule {
                key: rule.key,
                schema_digest: rule.schema_digest,
                destination_digest: rule.destination_digest,
                rule: PermissionRule::Allow,
            }
        } else {
            f.discover()
        };
        let app = f.app.clone();
        let diagnostic = tokio::spawn(async move { app.command(command).await });
        wire.initialize(true).await;
        let list = wire.list().await;
        f.reject_while_held(&mut wire, f.start()).await;
        f.reject_while_held(
            &mut wire,
            AppCommand::SummarizeContext {
                conversation_id: f.conversation.clone(),
                selection: AgentContextSelection {
                    message_ids: vec![],
                    summary_ids: vec![],
                },
            },
        )
        .await;
        assert!(f.app.snapshot().await.unwrap().runs.is_empty());
        list.rpc(tools(), false);
        let close = wire.next().await;
        assert!(close.method.starts_with("DELETE /mcp "));
        f.reject_while_held(&mut wire, f.start()).await;
        close.respond(200, "application/json", "", String::new());
        diagnostic.await.unwrap().unwrap();
        f.start_then_cancel().await;
        f.app.close().await.unwrap();
    }
}

#[tokio::test]
async fn r3_discovery_error_keeps_lease_through_cleanup_then_releases_it() {
    let mut wire = Wire::open().await;
    let f = AppFixture::open(&wire.url).await;
    let app = f.app.clone();
    let command = f.discover();
    let diagnostic = tokio::spawn(async move { app.command(command).await });
    wire.initialize(true).await;
    let request = wire.list().await;
    request.respond(200, "application/json", "", "invalid-rpc".into());
    let close = wire.next().await;
    assert!(close.method.starts_with("DELETE /mcp "));
    f.reject_while_held(&mut wire, f.start()).await;
    close.respond(200, "application/json", "", String::new());
    assert!(matches!(
        diagnostic.await.unwrap(),
        Err(CoreError::Protocol)
    ));
    f.start_then_cancel().await;
    f.app.close().await.unwrap();
}

#[tokio::test]
async fn discover_server_failure_carries_detail_across_command_boundary() {
    let mut wire = Wire::open().await;
    let f = AppFixture::open(&wire.url).await;
    let app = f.app.clone();
    let command = f.discover();
    let diagnostic = tokio::spawn(async move { app.command_with_detail(command).await });
    let request = wire.next().await;
    assert_eq!(request.body["method"], "initialize");
    request.respond(
        401,
        "application/json",
        "",
        json!({"error":"unknown API key"}).to_string(),
    );
    let failure = diagnostic.await.unwrap().unwrap_err();
    assert_eq!(failure.code, CoreError::Authentication);
    let detail = failure.detail.expect("failure detail");
    assert_eq!(detail.target_kind, "mcpServer");
    assert_eq!(detail.target_name.as_deref(), Some("s"));
    assert_eq!(detail.step, "initialize");
    assert_eq!(detail.status, Some(401));
    assert_eq!(detail.reason.as_deref(), Some("unknown API key"));
    f.app.close().await.unwrap();
}

#[tokio::test]
async fn discover_server_missing_credential_is_credential_missing() {
    let wire = Wire::open().await;
    let f = AppFixture::open(&wire.url).await;
    f.app
        .repository
        .save_server(core::ServerConfig {
            id: core::ServerId(uuid(&f.server).unwrap()),
            name: "s".into(),
            url: format!("{}/mcp", wire.url),
            allow_http: true,
            secret_headers: vec![("Authorization".into(), core::CredentialRef::new())],
        })
        .unwrap();
    let failure = f.app.command_with_detail(f.discover()).await.unwrap_err();
    assert_eq!(failure.code, CoreError::CredentialMissing);
    // The note names the server that holds the header credential.
    let detail = failure.detail.expect("failure detail");
    assert_eq!(detail.target_kind, "mcpServer");
    assert_eq!(detail.target_name.as_deref(), Some("s"));
    assert_eq!(detail.step, "resolveCredential");
    // The plain command path reports the same kind.
    assert_eq!(
        f.app.command(f.discover()).await.unwrap_err(),
        CoreError::CredentialMissing
    );
    f.app.close().await.unwrap();
}

#[tokio::test]
async fn discover_models_transport_failure_carries_detail() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    drop(listener);
    let f = AppFixture::open(&url).await;
    let failure = f
        .app
        .command_with_detail(AppCommand::DiscoverModels {
            endpoint_id: f.endpoint.clone(),
        })
        .await
        .unwrap_err();
    assert_eq!(failure.code, CoreError::UnavailableTransport);
    let detail = failure.detail.expect("failure detail");
    assert_eq!(detail.target_kind, "inference");
    assert_eq!(detail.target_name.as_deref(), Some("e"));
    assert_eq!(detail.step, "discoverModels");
    assert_eq!(detail.reason.as_deref(), Some("connection refused"));
    f.app.close().await.unwrap();
}

#[tokio::test]
async fn failed_run_snapshot_carries_failure_code_and_detail() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let f = AppFixture::open(&url).await;
    let stopped = f.stopped.notified();
    let run = f.app.command(f.start()).await.unwrap().id.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), stopped)
        .await
        .expect("run settles");
    let snapshot = f.app.snapshot().await.unwrap();
    let run = snapshot.runs.iter().find(|r| r.id == run).expect("run");
    let code = run.failure_code.clone().expect("failure code");
    let detail = run.failure_detail.clone().expect("failure detail");
    assert_eq!(code, CoreError::UnavailableTransport);
    assert_eq!(detail.reason.as_deref(), Some("connection refused"));
    assert_eq!(run.failure, detail.reason);
    f.app.close().await.unwrap();
}

#[tokio::test]
async fn run_snapshot_names_its_input_message() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let f = AppFixture::open(&url).await;
    let stopped = f.stopped.notified();
    let run = f.app.command(f.start()).await.unwrap().id.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), stopped)
        .await
        .expect("run settles");
    let snapshot = f.app.snapshot().await.unwrap();
    let run = snapshot.runs.iter().find(|r| r.id == run).expect("run");
    let conversation = snapshot
        .conversations
        .iter()
        .find(|c| c.id == run.conversation_id)
        .expect("conversation");
    let input = conversation
        .messages
        .iter()
        .find(|m| m.role == "user")
        .expect("user message");
    assert_eq!(run.input_message_id.as_deref(), Some(input.id.as_str()));
    f.app.close().await.unwrap();
}

#[tokio::test]
async fn mcp_error_reason_redacts_short_header_secret() {
    let mut wire = Wire::open().await;
    let f = AppFixture::open_with(&wire.url, Some("k3y9")).await;
    f.app
        .repository
        .save_server(core::ServerConfig {
            id: core::ServerId(uuid(&f.server).unwrap()),
            name: "s".into(),
            url: format!("{}/mcp", wire.url),
            allow_http: true,
            secret_headers: vec![("X-Api-Key".into(), core::CredentialRef::new())],
        })
        .unwrap();
    let app = f.app.clone();
    let command = f.discover();
    let diagnostic = tokio::spawn(async move { app.command_with_detail(command).await });
    let request = wire.next().await;
    assert_eq!(request.body["method"], "initialize");
    request.respond(
        401,
        "application/json",
        "",
        json!({"error":"key k3y9 is not valid"}).to_string(),
    );
    let failure = diagnostic.await.unwrap().unwrap_err();
    assert_eq!(failure.code, CoreError::Authentication);
    let reason = failure.detail.expect("detail").reason.expect("reason");
    assert!(!reason.contains("k3y9"), "{reason}");
    assert_eq!(reason, "key ••• is not valid");
    f.app.close().await.unwrap();
}

/// The `write` tool's alias as offered to the model in an inference request.
fn write_alias(inference: &Request) -> String {
    inference.body["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| !t["function"]["parameters"]["properties"]["marker"].is_null())
        .unwrap()["function"]["name"]
        .as_str()
        .unwrap()
        .to_owned()
}
fn stream(chunk: Value) -> String {
    format!("data: {chunk}\n\ndata: [DONE]\n\n")
}
fn tool_calls(alias: &str, ids: &[&str]) -> String {
    let calls: Vec<_> = ids
        .iter()
        .enumerate()
        .map(|(index, id)| json!({"index":index,"id":id,"type":"function","function":{"name":alias,"arguments":format!("{{\"marker\":\"{id}\"}}")}}))
        .collect();
    stream(json!({"choices":[{"delta":{"tool_calls":calls},"finish_reason":"tool_calls"}]}))
}
fn final_text() -> String {
    stream(json!({"choices":[{"delta":{"content":"done"},"finish_reason":"stop"}]}))
}
impl AppFixture {
    async fn run(&self, run: &str) -> AgentRun {
        self.app
            .snapshot()
            .await
            .unwrap()
            .runs
            .into_iter()
            .find(|r| r.id == run)
            .unwrap()
    }
    /// Starts a turn whose model asks for one `write` call per id, and waits
    /// until the run is waiting for approval.
    async fn waiting(&self, wire: &mut Wire, ids: &[&str]) -> (String, AgentRun) {
        let run = self.app.command(self.start()).await.unwrap().id.unwrap();
        wire.initialize(false).await;
        wire.list().await.rpc(tools(), false);
        let inference = wire.next().await;
        let alias = write_alias(&inference);
        inference.respond(200, "text/event-stream", "", tool_calls(&alias, ids));
        self.stopped.notified().await;
        let data = self.run(&run).await;
        assert_eq!(data.state, core::RunState::WaitingApproval);
        (run, data)
    }
}
async fn answer_call(wire: &mut Wire, marker: &str) {
    let call = wire.next().await;
    assert_eq!(call.body["method"], "tools/call");
    assert_eq!(call.body["params"]["arguments"]["marker"], marker);
    call.rpc(
        json!({"content":[{"type":"text","text":"ok"}],"isError":false}),
        false,
    );
}

#[tokio::test]
async fn decide_approvals_discovers_once_then_resumes_and_runs_every_call() {
    let mut wire = Wire::open().await;
    let f = AppFixture::open(&wire.url).await;
    let (run, data) = f.waiting(&mut wire, &["one", "two"]).await;
    let bindings: Vec<_> = data
        .attempts
        .iter()
        .map(|a| a.binding.clone().expect("every pending call is bound"))
        .collect();
    assert_eq!(bindings.len(), 2);

    // A stale binding fails the whole command after one discovery; nothing changes.
    let mut stale = bindings.clone();
    stale[1].arguments_json = r#"{"marker":"other"}"#.into();
    let app = f.app.clone();
    let command = AppCommand::DecideApprovals {
        run_id: run.clone(),
        approve: stale,
        deny: vec![],
    };
    let decision = tokio::spawn(async move { app.command(command).await });
    wire.initialize(false).await;
    wire.list().await.rpc(tools(), false);
    assert!(matches!(decision.await.unwrap(), Err(CoreError::Conflict)));
    let data = f.run(&run).await;
    assert_eq!(data.state, core::RunState::WaitingApproval);
    assert!(data
        .attempts
        .iter()
        .all(|a| a.state == core::AttemptState::Proposed));

    let app = f.app.clone();
    let command = AppCommand::DecideApprovals {
        run_id: run.clone(),
        approve: bindings,
        deny: vec![],
    };
    let decision = tokio::spawn(async move { app.command(command).await });
    // One discovery for the server, then the resumed run's own session.
    wire.initialize(false).await;
    wire.list().await.rpc(tools(), false);
    wire.initialize(false).await;
    wire.list().await.rpc(tools(), false);
    answer_call(&mut wire, "one").await;
    answer_call(&mut wire, "two").await;
    let inference = wire.next().await;
    assert!(inference.method.starts_with("POST /chat/completions "));
    inference.respond(200, "text/event-stream", "", final_text());
    decision.await.unwrap().unwrap();
    f.stopped.notified().await;
    let data = f.run(&run).await;
    assert_eq!(data.state, core::RunState::Completed);
    assert!(data
        .attempts
        .iter()
        .all(|a| a.state == core::AttemptState::Succeeded));
    assert!(wire.requests.try_recv().is_err());
    f.app.close().await.unwrap();
}

#[tokio::test]
async fn repeated_decide_approvals_needs_no_discovery_and_resumes() {
    let mut wire = Wire::open().await;
    let f = AppFixture::open(&wire.url).await;
    let (run, data) = f.waiting(&mut wire, &["one", "two"]).await;
    let first = data.attempts[0].binding.clone().unwrap();
    let second = data.attempts[1].id.clone();
    // Store the decisions without resuming, as if the app closed before resume.
    let app = f.app.clone();
    let command = AppCommand::Approve {
        binding: first.clone(),
    };
    let approval = tokio::spawn(async move { app.command(command).await });
    wire.initialize(false).await;
    wire.list().await.rpc(tools(), false);
    approval.await.unwrap().unwrap();
    // A repeated Approve is not stale even though its session is gone.
    f.app
        .command(AppCommand::Approve {
            binding: first.clone(),
        })
        .await
        .unwrap();
    for _ in 0..2 {
        f.app
            .command(AppCommand::Deny {
                attempt_id: second.clone(),
            })
            .await
            .unwrap();
    }
    let app = f.app.clone();
    let command = AppCommand::DecideApprovals {
        run_id: run.clone(),
        approve: vec![first],
        deny: vec![second],
    };
    let decision = tokio::spawn(async move { app.command(command).await });
    // The first request is the resumed run's own session, not a discovery.
    wire.initialize(false).await;
    wire.list().await.rpc(tools(), false);
    answer_call(&mut wire, "one").await;
    let inference = wire.next().await;
    inference.respond(200, "text/event-stream", "", final_text());
    decision.await.unwrap().unwrap();
    f.stopped.notified().await;
    let data = f.run(&run).await;
    assert_eq!(data.state, core::RunState::Completed);
    assert_eq!(data.attempts[0].state, core::AttemptState::Succeeded);
    assert_eq!(data.attempts[1].state, core::AttemptState::Denied);
    f.app.close().await.unwrap();
}

#[tokio::test]
async fn trusted_server_runs_calls_without_asking_until_its_url_changes() {
    let mut wire = Wire::open().await;
    let f = AppFixture::open(&wire.url).await;
    let server = |snapshot: &AgentAppSnapshot| {
        snapshot
            .servers
            .iter()
            .find(|s| s.id == f.server)
            .unwrap()
            .clone()
    };
    assert!(!server(&f.app.snapshot().await.unwrap()).trusted);
    f.app
        .command(AppCommand::SetServerTrust {
            server_id: f.server.clone(),
            trusted: true,
        })
        .await
        .unwrap();
    let mut saved = server(&f.app.snapshot().await.unwrap());
    assert!(saved.trusted);
    // Saving unchanged settings keeps trust, whatever `trusted` says.
    saved.trusted = false;
    f.app
        .command(AppCommand::SaveServer {
            server: saved.clone(),
        })
        .await
        .unwrap();
    assert!(server(&f.app.snapshot().await.unwrap()).trusted);

    let run = f.app.command(f.start()).await.unwrap().id.unwrap();
    wire.initialize(false).await;
    wire.list().await.rpc(tools(), false);
    let inference = wire.next().await;
    let alias = write_alias(&inference);
    inference.respond(200, "text/event-stream", "", tool_calls(&alias, &["one"]));
    answer_call(&mut wire, "one").await;
    wire.next()
        .await
        .respond(200, "text/event-stream", "", final_text());
    f.stopped.notified().await;
    assert_eq!(f.run(&run).await.state, core::RunState::Completed);

    // A different URL silently ends trust.
    saved.url = format!("{}/other", saved.url);
    f.app
        .command(AppCommand::SaveServer { server: saved })
        .await
        .unwrap();
    assert!(!server(&f.app.snapshot().await.unwrap()).trusted);
    f.app
        .command(AppCommand::SetServerTrust {
            server_id: f.server.clone(),
            trusted: false,
        })
        .await
        .unwrap();
    assert!(matches!(
        f.app
            .command(AppCommand::SetServerTrust {
                server_id: uuid::Uuid::new_v4().to_string(),
                trusted: true,
            })
            .await,
        Err(CoreError::Configuration)
    ));
    f.app.close().await.unwrap();
}

#[tokio::test]
async fn decide_approvals_on_a_running_run_stores_decisions_without_resuming() {
    let wire = Wire::open().await;
    let f = AppFixture::open(&wire.url).await;
    let repo = f.app.repository.clone();
    let profile = core::ProfileId(uuid(&f.profile).unwrap());
    let conversation = core::ConversationId(uuid(&f.conversation).unwrap());
    let run = core::RunId::new();
    let attempt = core::AttemptId::new();
    let binding = blocking(move || {
        repo.insert_run_for_test(run, conversation, core::RunState::Running)?;
        repo.insert_attempt_for_test(attempt, run, core::AttemptState::Proposed)?;
        let key = core::ToolKey {
            profile_id: profile,
            server: core::ToolServer::LocalFiles,
            name: "read".into(),
        };
        let definition =
            core::ToolDefinition::from_discovery(&repo, key.clone(), r#"{"type":"object"}"#)?;
        let call = core::ToolCall {
            key,
            provider_call_id: "call".into(),
            arguments_json: "{}".into(),
        };
        match crate::permissions::authorize(&repo, attempt, &call, &definition, None)? {
            core::Authorization::Ask(binding) => Ok(*binding),
            _ => Err(CoreError::Conflict),
        }
    })
    .await
    .unwrap();
    f.app
        .command(AppCommand::DecideApprovals {
            run_id: run.0.to_string(),
            approve: vec![binding.into()],
            deny: vec![],
        })
        .await
        .unwrap();
    let repo = f.app.repository.clone();
    let state = blocking(move || {
        repo.with_connection(move |conn| {
            conn.query_row(
                "SELECT state FROM attempts WHERE id=?1",
                [attempt.0.to_string()],
                |r| r.get::<_, String>(0),
            )
            .map_err(|_| CoreError::Storage)
        })
    })
    .await
    .unwrap();
    assert_eq!(state, "Approved");
    assert_eq!(
        blocking({
            let repo = f.app.repository.clone();
            move || crate::agent::run_state(&repo, run)
        })
        .await
        .unwrap(),
        core::RunState::Running
    );
    f.app.close().await.unwrap();
}

impl AppFixture {
    fn set_clock(&self, ms: u64) {
        *self.app.test_clock.lock().unwrap() =
            Some(std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms));
    }
    async fn connection_tests(&self) -> Vec<AgentConnectionTest> {
        self.app.snapshot().await.unwrap().connection_tests
    }
}

#[tokio::test]
async fn server_test_result_is_kept_replaced_and_survives_reopening() {
    let mut wire = Wire::open().await;
    let f = AppFixture::open(&wire.url).await;
    assert!(f.connection_tests().await.is_empty());
    f.set_clock(1_000);
    let app = f.app.clone();
    let command = f.discover();
    let passed = tokio::spawn(async move { app.command(command).await });
    wire.initialize(false).await;
    wire.list().await.rpc(
        json!({"tools":[{"name":"write","inputSchema":{"type":"object"}}, {"name":"bad","inputSchema":"not-an-object"}]}),
        false,
    );
    let reply = passed.await.unwrap().unwrap();
    let [test] = f.connection_tests().await.try_into().unwrap();
    assert_eq!(test.kind, ConnectionTestKind::DiscoverServer);
    assert_eq!(test.server_id.as_deref(), Some(f.server.as_str()));
    assert_eq!(test.profile_id.as_deref(), Some(f.profile.as_str()));
    assert_eq!(test.endpoint_id, None);
    assert_eq!(test.tested_at_ms, 1_000);
    assert_eq!(test.tool_count, Some(1));
    assert_eq!(test.skipped_tools, reply.skipped_tools);
    assert_eq!(test.skipped_tools.len(), 1);
    assert_eq!(test.failure_code, None);

    // A failing test replaces the passing one, keeping its sanitized detail.
    f.set_clock(2_000);
    let app = f.app.clone();
    let command = f.discover();
    let failed = tokio::spawn(async move { app.command_with_detail(command).await });
    let request = wire.next().await;
    assert_eq!(request.body["method"], "initialize");
    request.respond(
        401,
        "application/json",
        "",
        json!({"error":"unknown API key"}).to_string(),
    );
    let failure = failed.await.unwrap().unwrap_err();
    let [test] = f.connection_tests().await.try_into().unwrap();
    assert_eq!(test.tested_at_ms, 2_000);
    assert_eq!(test.failure_code, Some(CoreError::Authentication));
    assert_eq!(test.failure_detail.as_ref(), failure.detail.as_deref());
    assert_eq!(test.failure_detail.unwrap().status, Some(401));
    assert_eq!((test.tool_count, test.skipped_tools.len()), (None, 0));
    f.app.close().await.unwrap();

    // Leaving the screen, or the app, keeps the result.
    let reopened = Application::open(
        f._dir.path().to_string_lossy().into_owned(),
        |_| Box::pin(async { CredentialReply::Missing }),
        |_| Box::pin(async { PlatformReply::Ok }),
    )
    .await
    .unwrap();
    let [test] = reopened
        .snapshot()
        .await
        .unwrap()
        .connection_tests
        .try_into()
        .unwrap();
    assert_eq!(test.tested_at_ms, 2_000);
    assert_eq!(test.failure_code, Some(CoreError::Authentication));
    reopened.close().await.unwrap();
}

#[tokio::test]
async fn endpoint_keeps_one_result_per_test_kind() {
    let mut wire = Wire::open().await;
    let f = AppFixture::open(&wire.url).await;
    f.set_clock(5_000);
    let app = f.app.clone();
    let endpoint_id = f.endpoint.clone();
    let models = tokio::spawn(async move {
        app.command(AppCommand::DiscoverModels { endpoint_id })
            .await
    });
    let request = wire.next().await;
    assert!(
        request.method.starts_with("GET /models "),
        "{}",
        request.method
    );
    request.respond(
        200,
        "application/json",
        "",
        json!({"data":[{"id":"a"},{"id":"b"},{"id":"a"}]}).to_string(),
    );
    models.await.unwrap().unwrap();
    // A probe that cannot reach the endpoint is a second, separate result.
    f.set_clock(6_000);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let closed = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let mut endpoint: core::EndpointConfig =
        saved_endpoint(&f.app.repository, &f.endpoint).unwrap();
    endpoint.base_url = closed;
    f.app.repository.save_endpoint(endpoint).unwrap();
    f.app
        .command(AppCommand::ProbeEndpoint {
            endpoint_id: f.endpoint.clone(),
        })
        .await
        .unwrap_err();
    let [models, probe] = f.connection_tests().await.try_into().unwrap();
    assert_eq!(models.kind, ConnectionTestKind::DiscoverModels);
    assert_eq!(models.endpoint_id.as_deref(), Some(f.endpoint.as_str()));
    assert_eq!((models.tested_at_ms, models.model_count), (5_000, Some(2)));
    assert_eq!(probe.kind, ConnectionTestKind::ProbeTools);
    assert_eq!(probe.tested_at_ms, 6_000);
    assert_eq!(probe.failure_code, Some(CoreError::UnavailableTransport));
    assert_eq!(probe.tool_support, None);
    f.app.close().await.unwrap();
}

#[tokio::test]
async fn cancelled_or_unknown_target_tests_keep_the_previous_result() {
    let mut wire = Wire::open().await;
    let f = AppFixture::open(&wire.url).await;
    f.set_clock(1_000);
    let app = f.app.clone();
    let command = f.discover();
    let passed = tokio::spawn(async move { app.command(command).await });
    wire.initialize(false).await;
    wire.list().await.rpc(tools(), false);
    passed.await.unwrap().unwrap();

    // The caller goes away mid-test: the worker is cancelled and stores nothing.
    f.set_clock(2_000);
    let app = f.app.clone();
    let command = f.discover();
    let abandoned = tokio::spawn(async move { app.command(command).await });
    let request = wire.next().await;
    assert_eq!(request.body["method"], "initialize");
    let finished = f.app.diagnostic_finished.notified();
    abandoned.abort();
    assert!(abandoned.await.unwrap_err().is_cancelled());
    finished.await;
    drop(request);
    let [test] = f.connection_tests().await.try_into().unwrap();
    assert_eq!((test.tested_at_ms, test.tool_count), (1_000, Some(2)));

    // A test of an endpoint that was never saved stores nothing.
    assert_eq!(
        f.app
            .command(AppCommand::DiscoverModels {
                endpoint_id: core::EndpointId::new().0.to_string(),
            })
            .await
            .unwrap_err(),
        CoreError::Conflict
    );
    assert_eq!(f.connection_tests().await.len(), 1);
    f.app.close().await.unwrap();
}
