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
        let dir = tempfile::tempdir().unwrap();
        let stopped = Arc::new(Notify::new());
        let notice = stopped.clone();
        let app = Arc::new(
            Application::open(
                dir.path().to_string_lossy().into_owned(),
                |_| Box::pin(async { CredentialReply::Missing }),
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
    let detail = failure.detail.expect("failure detail");
    assert_eq!(detail.target_kind, "credential");
    assert_eq!(detail.target_name, None);
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
