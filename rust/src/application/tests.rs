use super::*;
use crate::error::CoreError;
use crate::model::PlatformReply;

// Loopback fixtures only: inspect every request and never expose a live service.
async fn diagnostic_http(
    replies: Vec<(u16, &'static str, String)>,
) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut requests = Vec::new();
        for (status, content_type, body) in replies {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let header_end = loop {
                let mut chunk = [0u8; 4096];
                let count = stream.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(pos) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    break pos + 4;
                }
                assert!(bytes.len() < 65536);
            };
            let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
            let length = headers
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            while bytes.len() < header_end + length {
                let mut chunk = [0u8; 4096];
                let count = stream.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
            }
            requests.push(String::from_utf8(bytes).unwrap());
            stream.write_all(format!("HTTP/1.1 {status} Fixture\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
        requests
    });
    (url, task)
}

#[tokio::test]
async fn diagnostics_use_real_safe_discovery_and_do_not_mutate_profiles() {
    let (url, server) = diagnostic_http(vec![
        (200, "application/json", r#"{"data":[{"id":"manual"},{"id":"other"}]}"#.into()),
        (400, "application/json", r#"{"error":{"code":"unsupported_parameter","param":"tools","message":"private-payload"}}"#.into()),
    ]).await;
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(
        dir.path().to_string_lossy().into_owned(),
        |_| Box::pin(async { CredentialReply::Missing }),
        |_| Box::pin(async { PlatformReply::Ok }),
    )
    .await
    .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    app.command(AppCommand::SaveEndpoint {
        endpoint: AgentEndpointConfig {
            id: id.clone(),
            name: "local".into(),
            base_url: url,
            model: "manual".into(),
            context_tokens: 4096,
            max_output_tokens: 128,
            temperature: 0.0,
            allow_http: true,
            credential: None,
        },
    })
    .await
    .unwrap();
    let models = app
        .command(AppCommand::DiscoverModels {
            endpoint_id: id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(models.models, vec!["manual", "other"]);
    let probe = app
        .command(AppCommand::ProbeEndpoint { endpoint_id: id })
        .await
        .unwrap();
    assert_eq!(probe.tool_support, Some(ToolSupport::Unsupported));
    let requests = server.await.unwrap();
    assert!(requests[0].starts_with("GET /models "));
    let payload: serde_json::Value =
        serde_json::from_str(requests[1].split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(payload["tool_choice"], "required");
    assert_eq!(payload["messages"].as_array().unwrap().len(), 1);
    assert!(app.snapshot().await.unwrap().profiles.is_empty());
    app.close().await.unwrap();
}

#[tokio::test]
async fn mcp_diagnostic_only_initializes_and_lists_linked_server() {
    let (url, server) = diagnostic_http(vec![
        (200, "application/json", r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"fixture","version":"1"}}}"#.into()),
        (202, "application/json", String::new()),
        (200, "application/json", r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"write","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},{"name":"remote","inputSchema":{"$ref":"https://example.invalid/schema"}}]}}"#.into()),
    ].into_iter().cycle().take(9).collect()).await;
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(
        dir.path().to_string_lossy().into_owned(),
        |_| Box::pin(async { CredentialReply::Missing }),
        |_| Box::pin(async { PlatformReply::Ok }),
    )
    .await
    .unwrap();
    let endpoint_id = uuid::Uuid::new_v4().to_string();
    let server_id = uuid::Uuid::new_v4().to_string();
    let profile_id = uuid::Uuid::new_v4().to_string();
    app.command(AppCommand::SaveEndpoint {
        endpoint: AgentEndpointConfig {
            id: endpoint_id.clone(),
            name: "e".into(),
            base_url: "https://example.invalid".into(),
            model: "m".into(),
            context_tokens: 4096,
            max_output_tokens: 128,
            temperature: 0.0,
            allow_http: false,
            credential: None,
        },
    })
    .await
    .unwrap();
    app.command(AppCommand::SaveServer {
        server: AgentServerConfig {
            id: server_id.clone(),
            name: "s".into(),
            url,
            allow_http: true,
            secret_headers: vec![],
            trusted: false,
        },
    })
    .await
    .unwrap();
    app.command(AppCommand::SaveProfile {
        profile: AgentProfile {
            id: profile_id.clone(),
            name: "p".into(),
            mode: core::ProfileMode::ChatOnly,
            endpoint_id,
            server_ids: vec![server_id.clone()],
            instructions: String::new(),
            limits: AgentRunLimits {
                max_inference_requests: 20,
                max_tool_attempts: 40,
                active_timeout_ms: 900000,
            },
        },
    })
    .await
    .unwrap();
    let reply = app
        .command(AppCommand::DiscoverServer {
            profile_id: profile_id.clone(),
            server_id: server_id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(reply.discovered_tools, vec!["write"]);
    assert_eq!(
        reply.skipped_tools,
        vec![AgentSkippedTool {
            name: "remote".into(),
            reason: crate::permissions::SchemaRejection::ExternalReference
                .reason()
                .into(),
        }]
    );
    let rule = reply.discovered_rules.into_iter().next().unwrap();
    assert_eq!(rule.rule, PermissionRule::Ask);
    app.command(AppCommand::SetRule {
        key: rule.key,
        schema_digest: rule.schema_digest,
        destination_digest: rule.destination_digest,
        rule: PermissionRule::Allow,
    })
    .await
    .unwrap();
    let reply = app
        .command(AppCommand::DiscoverServer {
            profile_id,
            server_id,
        })
        .await
        .unwrap();
    assert_eq!(reply.discovered_rules[0].rule, PermissionRule::Allow);
    let requests = server.await.unwrap();
    let methods: Vec<_> = requests
        .iter()
        .map(|r| {
            serde_json::from_str::<serde_json::Value>(r.split("\r\n\r\n").nth(1).unwrap()).unwrap()
                ["method"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(
        methods,
        ["initialize", "notifications/initialized", "tools/list"].repeat(3)
    );
    assert_eq!(
        app.snapshot().await.unwrap().profiles[0].mode,
        core::ProfileMode::ChatOnly
    );
    assert!(app.snapshot().await.unwrap().rules.is_empty());
    app.close().await.unwrap();
}

#[tokio::test]
async fn diagnostics_missing_credentials_are_credential_missing_not_tool_unsupported() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(
        dir.path().to_string_lossy().into_owned(),
        |_| Box::pin(async { CredentialReply::Missing }),
        |_| Box::pin(async { PlatformReply::Ok }),
    )
    .await
    .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    app.command(AppCommand::SaveEndpoint {
        endpoint: AgentEndpointConfig {
            id: id.clone(),
            name: "test".into(),
            base_url: "https://never-contact.invalid/v1".into(),
            model: "manual".into(),
            context_tokens: 4096,
            max_output_tokens: 128,
            temperature: 0.0,
            allow_http: false,
            credential: Some(uuid::Uuid::new_v4().to_string()),
        },
    })
    .await
    .unwrap();
    for command in [
        AppCommand::DiscoverModels {
            endpoint_id: id.clone(),
        },
        AppCommand::ProbeEndpoint { endpoint_id: id },
    ] {
        assert!(matches!(
            app.command(command).await,
            Err(CoreError::CredentialMissing)
        ));
    }
    assert!(app.snapshot().await.unwrap().profiles.is_empty());
    assert!(matches!(
        app.command(AppCommand::DiscoverServer {
            profile_id: uuid::Uuid::new_v4().to_string(),
            server_id: uuid::Uuid::new_v4().to_string()
        })
        .await,
        Err(CoreError::Conflict)
    ));
    app.close().await.unwrap();
}

#[tokio::test]
async fn shutdown_cancels_inflight_diagnostics_before_waiting_for_command_gate() {
    let dir = tempfile::tempdir().unwrap();
    let (started, mut receive) = tokio::sync::mpsc::unbounded_channel();
    let app = Arc::new(
        Application::open(
            dir.path().to_string_lossy().into_owned(),
            move |_| {
                let started = started.clone();
                Box::pin(async move {
                    started.send(()).unwrap();
                    std::future::pending().await
                })
            },
            |_| Box::pin(async { PlatformReply::Ok }),
        )
        .await
        .unwrap(),
    );
    let id = uuid::Uuid::new_v4().to_string();
    app.command(AppCommand::SaveEndpoint {
        endpoint: AgentEndpointConfig {
            id: id.clone(),
            name: "e".into(),
            base_url: "https://never-contact.invalid".into(),
            model: "m".into(),
            context_tokens: 4000,
            max_output_tokens: 10,
            temperature: 0.0,
            allow_http: false,
            credential: Some(uuid::Uuid::new_v4().to_string()),
        },
    })
    .await
    .unwrap();
    let command_app = app.clone();
    let command = tokio::spawn(async move {
        command_app
            .command(AppCommand::ProbeEndpoint { endpoint_id: id })
            .await
    });
    receive.recv().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), app.close())
        .await
        .expect("shutdown must cancel the diagnostic before waiting for its read gate")
        .unwrap();
    assert!(matches!(command.await.unwrap(), Err(CoreError::Cancelled)));
}

#[tokio::test]
async fn observed_mcp_rule_can_be_revoked_without_connectivity_or_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(
        dir.path().to_string_lossy().into_owned(),
        |_| Box::pin(async { CredentialReply::Missing }),
        |_| Box::pin(async { PlatformReply::Ok }),
    )
    .await
    .unwrap();
    let endpoint = core::EndpointId::new();
    let server = core::ServerId::new();
    let profile = core::ProfileId::new();
    app.repository
        .save_endpoint(core::EndpointConfig {
            id: endpoint,
            name: "e".into(),
            base_url: "https://never-contact.invalid".into(),
            model: "m".into(),
            context_tokens: 4096,
            max_output_tokens: 128,
            temperature: 0.0,
            allow_http: false,
            credential: None,
        })
        .unwrap();
    app.repository
        .save_server(core::ServerConfig {
            id: server,
            name: "s".into(),
            url: "https://never-contact.invalid/mcp".into(),
            allow_http: false,
            secret_headers: vec![(
                "Authorization".into(),
                core::CredentialRef(uuid::Uuid::new_v4()),
            )],
        })
        .unwrap();
    app.repository
        .save_profile(core::Profile {
            id: profile,
            mode: core::ProfileMode::Agent,
            name: "p".into(),
            endpoint_id: endpoint,
            server_ids: vec![server],
            instructions: String::new(),
            limits: core::RunLimits::default(),
        })
        .unwrap();
    let definition = core::ToolDefinition::from_discovery(
        &app.repository,
        core::ToolKey {
            profile_id: profile,
            server: core::ToolServer::Mcp(server),
            name: "write".into(),
        },
        r#"{"type":"object"}"#,
    )
    .unwrap();
    app.command(AppCommand::SetRule {
        key: definition.key.into(),
        schema_digest: definition.schema_digest,
        destination_digest: definition.destination_digest,
        rule: PermissionRule::Block,
    })
    .await
    .unwrap();
    assert_eq!(
        app.snapshot().await.unwrap().rules[0].rule,
        PermissionRule::Block
    );
    app.close().await.unwrap();
}

#[tokio::test]
async fn credential_missing_reply_maps_to_credential_missing() {
    let (result, detail) =
        crate::failure::scope(async { credential_value(CredentialReply::Missing).err() }).await;
    assert_eq!(result, Some(CoreError::CredentialMissing));
    let detail = detail.expect("credential note");
    assert_eq!(detail.target, crate::failure::FailureTarget::Credential);
    assert_eq!(detail.step, crate::failure::FailureStep::ResolveCredential);
    let (result, detail) = crate::failure::scope(async {
        credential_value(CredentialReply::Failed("x".into())).err()
    })
    .await;
    assert_eq!(result, Some(CoreError::Authentication));
    let detail = detail.expect("lookup failure note");
    assert_eq!(detail.target, crate::failure::FailureTarget::Credential);
    assert_eq!(detail.reason.as_deref(), Some("credential lookup failed"));
}

#[test]
fn callback_values_and_errors_are_redacted() {
    let reply = CredentialReply::Value("private-secret".into());
    assert!(!format!("{reply:?}").contains("private-secret"));
    let secret = credential_value(reply).expect("secret");
    assert_eq!(format!("{secret:?}"), "SecretValue([REDACTED])");
    assert!(matches!(
        credential_value(CredentialReply::Failed("private".into())),
        Err(CoreError::Authentication)
    ));
    assert!(matches!(
        platform_result(PlatformReply::Failed("private".into())),
        Err(CoreError::UnavailableTransport)
    ));
}

#[tokio::test]
async fn typed_configuration_reopens_and_closed_handle_rejects_commands() {
    let dir = tempfile::tempdir().unwrap();
    let open = || {
        Application::open(
            dir.path().to_string_lossy().into_owned(),
            |_| Box::pin(async { CredentialReply::Missing }),
            |_| Box::pin(async { PlatformReply::Ok }),
        )
    };
    let app = open().await.unwrap();
    let first_id = app.command(AppCommand::NewId).await.unwrap().id.unwrap();
    let second_id = app.command(AppCommand::NewId).await.unwrap().id.unwrap();
    assert!(uuid(&first_id).is_ok());
    assert_ne!(first_id, second_id);
    let endpoint = dto::AgentEndpointConfig {
        id: uuid::Uuid::new_v4().to_string(),
        name: "test".into(),
        base_url: "https://example.invalid/v1".into(),
        model: "model".into(),
        context_tokens: 4096,
        max_output_tokens: 128,
        temperature: 0.0,
        allow_http: false,
        credential: None,
    };
    app.command(dto::AppCommand::SaveEndpoint {
        endpoint: endpoint.clone(),
    })
    .await
    .unwrap();
    let profile = dto::AgentProfile {
        mode: crate::model::core::ProfileMode::Agent,
        id: uuid::Uuid::new_v4().to_string(),
        name: "profile".into(),
        endpoint_id: endpoint.id,
        server_ids: vec![],
        instructions: "original".into(),
        limits: dto::AgentRunLimits {
            max_inference_requests: 20,
            max_tool_attempts: 40,
            active_timeout_ms: 900000,
        },
    };
    app.command(dto::AppCommand::SaveProfile {
        profile: profile.clone(),
    })
    .await
    .unwrap();
    app.command(dto::AppCommand::CreateConversation {
        profile_id: profile.id.clone(),
    })
    .await
    .unwrap();
    let snap = app.snapshot().await.unwrap();
    assert_eq!(snap.conversations[0].profile.instructions, "original");
    assert_eq!(
        snap.conversations[0].profile.inference_origin.as_deref(),
        Some("https://example.invalid")
    );
    assert!(!snap.conversations[0].origin_acknowledged);
    app.command(AppCommand::AcknowledgeContextDisclosure {
        conversation_id: snap.conversations[0].id.clone(),
        origin: snap.conversations[0]
            .profile
            .inference_origin
            .clone()
            .unwrap(),
    })
    .await
    .unwrap();
    assert!(app.snapshot().await.unwrap().conversations[0].origin_acknowledged);
    assert_eq!(snap.endpoints.len(), 1);
    let definition = core::ToolDefinition::from_discovery(
        &app.repository,
        core::ToolKey {
            profile_id: core::ProfileId(uuid(&profile.id).unwrap()),
            server: core::ToolServer::LocalFiles,
            name: "read_file".into(),
        },
        r#"{"type":"object","properties":{},"additionalProperties":false}"#,
    )
    .unwrap();
    let observed = app.snapshot().await.unwrap().rules.pop().unwrap();
    assert_eq!(observed.rule, PermissionRule::Ask);
    assert!(matches!(
        app.command(AppCommand::SetRule {
            key: definition.key.clone().into(),
            schema_digest: "stale".into(),
            destination_digest: definition.destination_digest.clone(),
            rule: PermissionRule::Allow,
        })
        .await,
        Err(CoreError::Conflict)
    ));
    app.command(AppCommand::SetRule {
        key: definition.key.into(),
        schema_digest: definition.schema_digest,
        destination_digest: definition.destination_digest,
        rule: PermissionRule::Allow,
    })
    .await
    .unwrap();
    assert_eq!(
        app.snapshot().await.unwrap().rules[0].rule,
        PermissionRule::Allow
    );
    assert!(matches!(
        app.command(dto::AppCommand::CancelRun {
            run_id: "payload-private".into()
        })
        .await,
        Err(CoreError::InvalidInput)
    ));
    let owner = snap.conversations[0].id.clone();
    let artifact_id = app
        .command(AppCommand::CreateArtifact {
            conversation_id: owner.clone(),
            name: "answer.md".into(),
            content: "generated".into(),
        })
        .await
        .unwrap()
        .id
        .unwrap();
    assert_eq!(app.snapshot().await.unwrap().artifacts.len(), 1);
    assert!(app
        .command(AppCommand::ReadArtifact {
            conversation_id: uuid::Uuid::new_v4().to_string(),
            artifact_id: artifact_id.clone()
        })
        .await
        .is_err());
    assert_eq!(
        app.command(AppCommand::ReadArtifact {
            conversation_id: owner.clone(),
            artifact_id: artifact_id.clone()
        })
        .await
        .unwrap()
        .artifact
        .unwrap()
        .text,
        "generated"
    );
    app.close().await.unwrap();
    assert!(matches!(
        app.command(dto::AppCommand::SaveEndpoint {
            endpoint: snap.endpoints[0].clone()
        })
        .await,
        Err(CoreError::Conflict)
    ));
    drop(app);
    let app = open().await.unwrap();
    assert_eq!(app.snapshot().await.unwrap().conversations.len(), 1);
    assert_eq!(app.snapshot().await.unwrap().artifacts[0].id, artifact_id);
    app.command(AppCommand::DeleteArtifact {
        conversation_id: owner,
        artifact_id,
    })
    .await
    .unwrap();
    assert!(app.snapshot().await.unwrap().artifacts.is_empty());
    app.close().await.unwrap();
}

#[tokio::test]
async fn export_with_missing_credential_is_credential_missing_across_bridge_path() {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(
        dir.path().to_string_lossy().into_owned(),
        |_| Box::pin(async { CredentialReply::Missing }),
        |_| Box::pin(async { PlatformReply::Ok }),
    )
    .await
    .unwrap();
    let endpoint = core::EndpointId::new();
    let profile = core::ProfileId::new();
    app.repository
        .save_endpoint(core::EndpointConfig {
            id: endpoint,
            name: "e".into(),
            base_url: "https://example.invalid/v1".into(),
            model: "m".into(),
            context_tokens: 4096,
            max_output_tokens: 128,
            temperature: 0.0,
            allow_http: false,
            credential: Some(core::CredentialRef::new()),
        })
        .unwrap();
    app.repository
        .save_profile(core::Profile {
            id: profile,
            name: "p".into(),
            endpoint_id: endpoint,
            server_ids: vec![],
            mode: core::ProfileMode::ChatOnly,
            instructions: String::new(),
            limits: core::RunLimits::default(),
        })
        .unwrap();
    let conversation = app.repository.create_conversation(profile).unwrap();
    let export = || AppCommand::ExportTranscript {
        conversation_id: conversation.0.to_string(),
        include_tool_payloads: false,
    };
    let failure = app.command_with_detail(export()).await.unwrap_err();
    assert_eq!(failure.code, CoreError::CredentialMissing);
    // The plain command path agrees on the precise kind.
    assert_eq!(
        app.command(export()).await.unwrap_err(),
        CoreError::CredentialMissing
    );
    app.close().await.unwrap();
}

#[tokio::test]
async fn chat_turn_with_missing_credential_fails_as_credential_missing() {
    let dir = tempfile::tempdir().unwrap();
    let stopped = Arc::new(tokio::sync::Notify::new());
    let notice = stopped.clone();
    // Missing reaches Rust through `Credentials`, the provider net.rs resolves.
    let app = Application::open(
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
    .unwrap();
    let endpoint = core::EndpointId::new();
    let profile = core::ProfileId::new();
    app.repository
        .save_endpoint(core::EndpointConfig {
            id: endpoint,
            name: "e".into(),
            base_url: "https://never-contact.invalid/v1".into(),
            model: "m".into(),
            context_tokens: 4096,
            max_output_tokens: 128,
            temperature: 0.0,
            allow_http: false,
            credential: Some(core::CredentialRef::new()),
        })
        .unwrap();
    app.repository
        .save_profile(core::Profile {
            id: profile,
            name: "p".into(),
            endpoint_id: endpoint,
            server_ids: vec![],
            mode: core::ProfileMode::ChatOnly,
            instructions: String::new(),
            limits: core::RunLimits::default(),
        })
        .unwrap();
    let conversation = app.repository.create_conversation(profile).unwrap();
    let run = app
        .command(AppCommand::StartTurn {
            conversation_id: conversation.0.to_string(),
            text: "hello".into(),
        })
        .await
        .unwrap()
        .id
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), stopped.notified())
        .await
        .expect("run settles");

    // Stored run: the persisted failure carries the code and credential detail.
    let id = run.clone();
    let raw: Option<String> = app
        .repository
        .with_connection(move |conn| {
            conn.query_row("SELECT failure FROM runs WHERE id=?1", [id], |r| r.get(0))
                .map_err(|_| CoreError::Storage)
        })
        .unwrap();
    let raw: serde_json::Value = serde_json::from_str(&raw.expect("stored failure")).unwrap();
    assert_eq!(
        raw["code"],
        serde_json::to_value(CoreError::CredentialMissing).unwrap()
    );
    assert_eq!(
        raw["detail"]["target"],
        serde_json::to_value(crate::failure::FailureTarget::Inference { name: "e".into() })
            .unwrap()
    );
    assert_eq!(
        raw["detail"]["step"],
        serde_json::to_value(crate::failure::FailureStep::ResolveCredential).unwrap()
    );

    // Snapshot DTO.
    let snapshot = app.snapshot().await.unwrap();
    let dto = snapshot
        .runs
        .iter()
        .find(|r| r.id == run)
        .expect("run in snapshot");
    assert_eq!(dto.state, RunState::Failed);
    assert_eq!(dto.failure_code, Some(CoreError::CredentialMissing));
    assert_eq!(
        dto.failure.as_deref(),
        Some(crate::failure::CREDENTIAL_MISSING_REASON)
    );
    // The note names the endpoint that holds the credential.
    let detail = dto.failure_detail.as_ref().expect("failure detail");
    assert_eq!(detail.target_kind, "inference");
    assert_eq!(detail.target_name.as_deref(), Some("e"));
    assert_eq!(detail.step, "resolveCredential");
    app.close().await.unwrap();
}
