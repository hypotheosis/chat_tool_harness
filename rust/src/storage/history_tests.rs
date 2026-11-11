use super::*;
use crate::files::artifacts::ArtifactStore;
use crate::history::{export_transcript, ExportOptions};
use crate::net::{CredentialProvider, SecretValue};
use std::{future::Future, pin::Pin};

fn fixture() -> (
    tempfile::TempDir,
    Repository,
    ProfileId,
    ConversationId,
    ArtifactStore,
) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::open(&dir.path().join("db")).unwrap();
    let endpoint = EndpointId::new();
    repo.save_endpoint(EndpointConfig {
        id: endpoint,
        name: "private-config".into(),
        base_url: "https://example.test".into(),
        model: "model".into(),
        context_tokens: 4096,
        max_output_tokens: 100,
        temperature: 0.0,
        allow_http: false,
        credential: None,
    })
    .unwrap();
    let profile = ProfileId::new();
    repo.save_profile(Profile {
        id: profile,
        name: "profile".into(),
        mode: ProfileMode::ChatOnly,
        endpoint_id: endpoint,
        server_ids: vec![],
        instructions: "private instructions".into(),
        limits: RunLimits::default(),
    })
    .unwrap();
    let owner = repo.create_conversation(profile).unwrap();
    let store = ArtifactStore::open(repo.clone(), &dir.path().join("artifacts")).unwrap();
    (dir, repo, profile, owner, store)
}
fn message(repo: &Repository, owner: ConversationId, role: &str, text: &str) {
    let payload =
        serde_json::json!({"role":role,"content":text,"calls":[],"call_id":null}).to_string();
    repo.with_connection(move |c| { c.execute("INSERT INTO messages(id,conversation_id,snapshot_revision,ordinal,payload) VALUES(?1,?2,1,(SELECT count(*) FROM messages),?3)",params![Uuid::new_v4().to_string(),id(owner),payload]).map_err(|_|CoreError::Storage)?; Ok(()) }).unwrap();
}
#[test]
fn history_search_titles_and_messages_only() {
    let (_dir, repo, profile, owner, store) = fixture();
    message(&repo, owner, "user", "quote ' 100% _ 東京");
    store
        .create(owner, "excluded.txt", "attachment-only")
        .unwrap();
    for q in ["'", "%", "_", "東京"] {
        assert_eq!(
            repo.search_conversations(q, false, 0, 100).unwrap().len(),
            1
        );
    }
    for q in [
        "attachment-only",
        "private instructions",
        "private-config",
        "' OR 1=1 --",
    ] {
        assert!(repo
            .search_conversations(q, false, 0, 100)
            .unwrap()
            .is_empty());
    }
    for _ in 0..101 {
        repo.create_conversation(profile).unwrap();
    }
    let first = repo.search_conversations("", false, 0, u32::MAX).unwrap();
    let second = repo.search_conversations("", false, 100, 100).unwrap();
    assert_eq!(first.len(), 100);
    assert_eq!(second.len(), 2);
    assert!(!first.iter().any(|a| second.iter().any(|b| a.id == b.id)));
    assert!(repo
        .search_conversations("", false, 0, 0)
        .unwrap()
        .is_empty());
}
#[test]
fn rename_persists() {
    let (dir, repo, _, owner, _) = fixture();
    repo.rename_conversation(owner, "Renamed 東京 %").unwrap();
    assert!(repo.rename_conversation(owner, "  ").is_err());
    let reopened = Repository::open(&dir.path().join("db")).unwrap();
    assert_eq!(
        reopened
            .search_conversations("Renamed", false, 0, 1)
            .unwrap()[0]
            .title,
        "Renamed 東京 %"
    );
}
#[test]
fn delete_active_conversation_requires_stop() {
    let (_dir, repo, _, owner, store) = fixture();
    let run = RunId::new();
    repo.insert_run_for_test(run, owner, RunState::Running)
        .unwrap();
    assert_eq!(
        store.delete_conversation(owner, false),
        Err(CoreError::Conflict)
    );
    repo.with_connection(move |c| {
        c.execute("UPDATE runs SET state='Cancelled' WHERE id=?1", [id(run)])
            .unwrap();
        Ok(())
    })
    .unwrap();
    repo.insert_attempt_for_test(AttemptId::new(), run, AttemptState::OutcomeUnknown)
        .unwrap();
    assert_eq!(
        store.delete_conversation(owner, false),
        Err(CoreError::Conflict)
    );
    store.delete_conversation(owner, true).unwrap();
    assert!(repo.conversation(owner).is_err());
}
#[test]
fn delete_cascades_owned_files_not_shared_config() {
    let (dir, repo, profile, owner, store) = fixture();
    let other = repo.create_conversation(profile).unwrap();
    let owned = store.create(owner, "owned.txt", "owned").unwrap();
    let kept = store.create(other, "kept.txt", "kept").unwrap();
    store.delete_conversation(owner, false).unwrap();
    assert!(!dir
        .path()
        .join(format!("artifacts/{}.txt", owned.id.0))
        .exists());
    assert_eq!(store.read(other, kept.id).unwrap().1, "kept");
    assert!(repo.create_conversation(profile).is_ok());
    assert!(repo.conversation(owner).is_err());
}
#[test]
fn history_delete_fault_rolls_back_and_crash_reconciles() {
    let (dir, repo, _, owner, store) = fixture();
    let artifact = store.create(owner, "a.txt", "kept").unwrap();
    repo.with_connection(|c| { c.execute_batch("CREATE TRIGGER reject_history_delete BEFORE DELETE ON conversations BEGIN SELECT RAISE(ABORT,'fault'); END;").unwrap(); Ok(()) }).unwrap();
    assert_eq!(
        store.delete_conversation(owner, false),
        Err(CoreError::Storage)
    );
    assert_eq!(store.read(owner, artifact.id).unwrap().1, "kept");
    assert!(repo.conversation(owner).is_ok());
    let original = dir.path().join(format!("artifacts/{}.txt", artifact.id.0));
    let tombstone = original.with_extension("deleting");
    // Crash before metadata commit: reopening restores all staged files.
    std::fs::rename(&original, &tombstone).unwrap();
    let reopened = ArtifactStore::open(repo.clone(), &dir.path().join("artifacts")).unwrap();
    assert_eq!(reopened.read(owner, artifact.id).unwrap().1, "kept");
    repo.with_connection(|c| {
        c.execute_batch("DROP TRIGGER reject_history_delete")
            .unwrap();
        Ok(())
    })
    .unwrap();
    reopened.delete_conversation(owner, false).unwrap();
    // Crash after commit, before unlink: only UUID-owned orphan tombstones are reclaimed.
    std::fs::write(&tombstone, "orphan").unwrap();
    let unrelated = dir.path().join("artifacts/not-owned.deleting");
    std::fs::write(&unrelated, "foreign").unwrap();
    ArtifactStore::open(repo.clone(), &dir.path().join("artifacts")).unwrap();
    assert!(!tombstone.exists());
    assert!(unrelated.exists());
    assert!(repo
        .search_conversations("", false, 0, 100)
        .unwrap()
        .is_empty());
}
#[tokio::test]
async fn history_export_limit_keeps_history_and_creates_no_artifact() {
    let (_dir, repo, _, owner, store) = fixture();
    message(&repo, owner, "user", &"東京".repeat(400_000));
    assert!(matches!(
        export_transcript(&repo, &store, owner, ExportOptions::default(), &Keys(None)).await,
        Err(CoreError::Limit)
    ));
    assert!(store.all().unwrap().is_empty());
    assert_eq!(repo.conversation(owner).unwrap().messages.len(), 1);
}
#[tokio::test(start_paused = true)]
async fn history_export_unavailable_credential_times_out_closed() {
    let (_dir, repo, profile, owner, store) = fixture();
    let mut endpoint = repo.conversation(owner).unwrap().profile.endpoint;
    endpoint.credential = Some(CredentialRef::new());
    repo.save_endpoint(endpoint).unwrap();
    repo.apply_profile(owner, profile).unwrap();
    struct Pending(tokio::sync::Notify);
    impl CredentialProvider for Pending {
        fn resolve(
            &self,
            _: CredentialRef,
        ) -> Pin<Box<dyn Future<Output = CoreResult<SecretValue>> + Send + '_>> {
            Box::pin(async {
                self.0.notify_one();
                std::future::pending().await
            })
        }
    }
    let keys = Pending(tokio::sync::Notify::new());
    let export = export_transcript(&repo, &store, owner, ExportOptions::default(), &keys);
    tokio::pin!(export);
    tokio::select! { _=keys.0.notified()=>{}, _=&mut export=>panic!("must wait for credential") }
    tokio::time::advance(std::time::Duration::from_secs(31)).await;
    std::future::poll_fn(|cx| {
        match export.as_mut().poll(cx) {
            std::task::Poll::Ready(result) => {
                assert!(matches!(result, Err(CoreError::Authentication)))
            }
            std::task::Poll::Pending => panic!("credential resolution must be bounded"),
        };
        std::task::Poll::Ready(())
    })
    .await;
    assert!(store.all().unwrap().is_empty());
    assert!(repo.conversation(owner).is_ok());
}
struct Keys(Option<String>);
impl CredentialProvider for Keys {
    fn resolve(
        &self,
        _: CredentialRef,
    ) -> Pin<Box<dyn Future<Output = CoreResult<SecretValue>> + Send + '_>> {
        Box::pin(async {
            self.0
                .clone()
                .map(SecretValue::new)
                .ok_or(CoreError::Authentication)
        })
    }
}
#[tokio::test]
async fn export_defaults_to_tool_summaries() {
    let (_dir, repo, _, owner, store) = fixture();
    message(&repo, owner, "user", "hello");
    message(&repo, owner, "tool", "private-payload");
    let bundle = export_transcript(&repo, &store, owner, ExportOptions::default(), &Keys(None))
        .await
        .unwrap();
    let text = store.read(owner, bundle.transcript.id).unwrap().1;
    assert!(text.contains("Export detail: ToolSummaries"));
    assert!(text.contains("hello"));
    assert!(text.contains("Tool result omitted"));
    assert!(!text.contains("private-payload"));
    let bundle = export_transcript(
        &repo,
        &store,
        owner,
        ExportOptions {
            include_tool_payloads: true,
        },
        &Keys(None),
    )
    .await
    .unwrap();
    assert!(store
        .read(owner, bundle.transcript.id)
        .unwrap()
        .1
        .contains("private-payload"));
}
#[tokio::test]
async fn export_never_includes_configured_secrets() {
    let (_dir, repo, profile, owner, store) = fixture();
    let mut endpoint = repo.conversation(owner).unwrap().profile.endpoint;
    endpoint.credential = Some(CredentialRef::new());
    repo.save_endpoint(endpoint.clone()).unwrap();
    repo.apply_profile(owner, profile).unwrap();
    endpoint.credential = Some(CredentialRef::new());
    repo.save_endpoint(endpoint).unwrap();
    let secret = "token\"slash\\line\n東京";
    message(&repo, owner, "user", &format!("upstream echoed {secret}"));
    message(
        &repo,
        owner,
        "tool",
        &serde_json::json!({"nested":{"key":secret},"array":[secret]})
            .to_string()
            .replace("token", r"\u0074oken")
            .replace("東京", r"\u6771\u4eac"),
    );
    let bundle = export_transcript(
        &repo,
        &store,
        owner,
        ExportOptions {
            include_tool_payloads: true,
        },
        &Keys(Some(secret.into())),
    )
    .await
    .unwrap();
    let text = store.read(owner, bundle.transcript.id).unwrap().1;
    assert!(!text.contains(secret));
    assert!(!text.contains("token"));
    assert!(text.contains("[REDACTED]"));
    let before = store.all().unwrap().len();
    assert!(matches!(
        export_transcript(&repo, &store, owner, ExportOptions::default(), &Keys(None)).await,
        Err(CoreError::Authentication)
    ));
    assert_eq!(store.all().unwrap().len(), before);
    assert!(repo.search_conversations("echoed", false, 0, 100).is_ok());
    assert!(repo.conversation(owner).is_ok());
}
#[tokio::test]
async fn history_export_labels_tool_outcomes_partial_and_derived_without_payloads() {
    let (_dir, repo, profile, owner, store) = fixture();
    let run = RunId::new();
    repo.insert_run_for_test(run, owner, RunState::Interrupted)
        .unwrap();
    let call = crate::agent::context::StoredCall {
        key: ToolKey {
            profile_id: profile,
            server: ToolServer::LocalFiles,
            name: "write".into(),
        },
        provider_call_id: "hidden-provider-id".into(),
        arguments_json: r#"{"private":"payload-only"}"#.into(),
        source_envelope: None,
    };
    let summary = crate::agent::context::DerivedContext {
        id: MessageId::new(),
        run_id: run,
        text: "derived text".into(),
        selection: crate::agent::context::ContextSelection {
            message_ids: vec![],
            summary_ids: vec![],
        },
        page_references: vec![],
        partial_coverage: true,
        unresolved_attempts: vec![],
    };
    repo.with_connection(move |c| {
        for state in ["Denied","Failed","OutcomeUnknown"] {
            c.execute("INSERT INTO attempts(id,run_id,state,call_json,result_json) VALUES(?1,?2,?3,?4,'private-result')",params![Uuid::new_v4().to_string(),id(run),state,serde_json::to_string(&call).unwrap()]).unwrap();
        }
        c.execute("UPDATE runs SET partial_text='partial text',failure='upstream error' WHERE id=?1",[id(run)]).unwrap();
        c.execute("INSERT INTO derived_context(id,conversation_id,run_id,payload) VALUES(?1,?2,?3,?4)",params![summary.id.0.to_string(),id(owner),id(run),serde_json::to_string(&summary).unwrap()]).unwrap(); Ok(())
    }).unwrap();
    let bundle = export_transcript(&repo, &store, owner, ExportOptions::default(), &Keys(None))
        .await
        .unwrap();
    let text = store.read(owner, bundle.transcript.id).unwrap().1;
    for label in [
        "Tool outcome: write — Denied",
        "Tool outcome: write — Failed",
        "OutcomeUnknown",
        "Partial assistant message (not completed)",
        "Derived summary (not original messages; partial coverage: true)",
        "upstream error",
    ] {
        assert!(text.contains(label), "missing {label}");
    }
    for excluded in ["payload-only", "private-result", "hidden-provider-id"] {
        assert!(!text.contains(excluded));
    }
    assert_eq!(
        repo.conversation(owner).unwrap().summaries[0].text,
        "derived text"
    );
}
fn configured_key(repo: &Repository, profile: ProfileId, owner: ConversationId) {
    let mut endpoint = repo.conversation(owner).unwrap().profile.endpoint;
    endpoint.credential = Some(CredentialRef::new());
    repo.save_endpoint(endpoint).unwrap();
    repo.apply_profile(owner, profile).unwrap();
}

#[tokio::test]
async fn history_b1_redacts_all_configured_json_scalar_variants() {
    for secret in [
        "123456789",
        "-123456789",
        "12.5",
        "1.25e+20",
        "true",
        "false",
        "null",
    ] {
        let (_dir, repo, profile, owner, store) = fixture();
        configured_key(&repo, profile, owner);
        let original = format!(r#"{{"credential":{secret},"array":[{secret}]}}"#);
        message(&repo, owner, "user", &original);
        let bundle = export_transcript(
            &repo,
            &store,
            owner,
            ExportOptions {
                include_tool_payloads: true,
            },
            &Keys(Some(secret.into())),
        )
        .await
        .unwrap();
        let text = store.read(owner, bundle.transcript.id).unwrap().1;
        let json = text
            .split("user:\n")
            .nth(1)
            .unwrap()
            .split("\n\n")
            .next()
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(
            value["credential"], "[REDACTED]",
            "scalar variant was not redacted"
        );
        assert_eq!(value["array"][0], "[REDACTED]");
        assert_eq!(
            repo.conversation(owner).unwrap().messages[0]
                .message
                .content,
            original
        );
    }
}

#[tokio::test]
async fn history_b1_redacts_escaped_json_in_persisted_mcp_text_blocks() {
    let (_dir, repo, profile, owner, store) = fixture();
    configured_key(&repo, profile, owner);
    let secret = "token";
    let inner = r#"{"credential":"\u0074oken","nested":["\u0074oken"]}"#;
    let outcome = crate::mcp::ToolOutcome {
        is_error: false,
        text: vec![inner.into()],
        structured_content: Some(serde_json::json!({"safe":"retained"})),
        unsupported_content: vec![],
    };
    // Same MCP ToolOutcome envelope persisted as both attempt result and tool message by the runtime.
    let original = serde_json::json!({"isError":outcome.is_error,"text":outcome.text,"structuredContent":outcome.structured_content,"unsupportedContent":[]}).to_string();
    message(&repo, owner, "tool", &original);
    let run = RunId::new();
    repo.insert_run_for_test(run, owner, RunState::Completed)
        .unwrap();
    let saved = original.clone();
    let call = crate::agent::context::StoredCall {
        key: ToolKey {
            profile_id: profile,
            server: ToolServer::Mcp(ServerId::new()),
            name: "read".into(),
        },
        provider_call_id: "fixture".into(),
        arguments_json: inner.into(),
        source_envelope: None,
    };
    repo.with_connection(move |c| { c.execute("INSERT INTO attempts(id,run_id,state,call_json,result_json) VALUES(?1,?2,'Succeeded',?3,?4)",params![Uuid::new_v4().to_string(),id(run),serde_json::to_string(&call).unwrap(),saved]).unwrap(); Ok(()) }).unwrap();
    let bundle = export_transcript(
        &repo,
        &store,
        owner,
        ExportOptions {
            include_tool_payloads: true,
        },
        &Keys(Some(secret.into())),
    )
    .await
    .unwrap();
    let text = store.read(owner, bundle.transcript.id).unwrap().1;
    for section in ["tool:\n", "Result: "] {
        let json = text.split(section).nth(1).unwrap();
        let envelope = serde_json::Deserializer::from_str(json)
            .into_iter::<serde_json::Value>()
            .next()
            .unwrap()
            .unwrap();
        let block: serde_json::Value =
            serde_json::from_str(envelope["text"][0].as_str().unwrap()).unwrap();
        assert_eq!(block["credential"], "[REDACTED]");
        assert_eq!(block["nested"][0], "[REDACTED]");
        assert_eq!(envelope["structuredContent"]["safe"], "retained");
    }
    assert!(!text.contains(secret));
    assert_eq!(
        repo.conversation(owner).unwrap().messages[0]
            .message
            .content,
        original
    );
    let saved: String = repo
        .with_connection(move |c| {
            c.query_row(
                "SELECT result_json FROM attempts WHERE run_id=?1",
                [id(run)],
                |r| r.get(0),
            )
            .map_err(|_| CoreError::Storage)
        })
        .unwrap();
    assert_eq!(saved, original);
}

#[tokio::test]
async fn history_b1_redaction_bounds_fail_closed_without_artifact_or_history_changes() {
    for kind in ["structure", "decoding", "nodes", "work"] {
        let (_dir, repo, profile, owner, store) = fixture();
        configured_key(&repo, profile, owner);
        let original = match kind {
            "structure" => format!("{}0{}", "[".repeat(40), "]".repeat(40)),
            "decoding" => (0..14).fold(r#"{"credential":"\u0074oken"}"#.to_owned(), |s, _| {
                serde_json::to_string(&s).unwrap()
            }),
            "nodes" => format!("[{}]", vec!["0"; 20_000].join(",")),
            "work" => {
                repo.save_server(ServerConfig {
                    id: ServerId::new(),
                    name: "fixture".into(),
                    url: "https://work.test".into(),
                    allow_http: false,
                    secret_headers: (0..60)
                        .map(|n| (format!("X-Key-{n}"), CredentialRef::new()))
                        .collect(),
                })
                .unwrap();
                "a".repeat(200_000)
            }
            _ => unreachable!(),
        };
        message(&repo, owner, "user", &original);
        let result = export_transcript(
            &repo,
            &store,
            owner,
            ExportOptions {
                include_tool_payloads: true,
            },
            &Keys(Some("token".into())),
        )
        .await;
        assert!(
            matches!(result, Err(CoreError::Limit)),
            "redaction must fail closed at {kind} bound"
        );
        assert!(store.all().unwrap().is_empty());
        assert_eq!(
            repo.conversation(owner).unwrap().messages[0]
                .message
                .content,
            original
        );
        assert_eq!(
            repo.search_conversations("", false, 0, 100).unwrap().len(),
            1
        );
    }
}

#[tokio::test]
async fn export_not_a_restorable_backup() {
    let (_dir, repo, _, owner, store) = fixture();
    let bundle = export_transcript(&repo, &store, owner, ExportOptions::default(), &Keys(None))
        .await
        .unwrap();
    let text = store.read(owner, bundle.transcript.id).unwrap().1;
    assert!(text.contains("not a restorable backup"));
    assert!(text.contains("sensitive"));
    assert!(!text.contains("private instructions"));
    assert!(!text.contains("example.test"));
    assert!(!text.contains(&owner.0.to_string()));
}

// Enough summary-only rows to pass the 1 MiB cap without redaction work, then
// one corrupt row: an export that reads everything first trips over the corrupt
// row, while an incrementally bounded one stops at the cap before reaching it.
const ROWS_PAST_CAP: i64 = 40_000;

#[tokio::test]
async fn history_export_stops_reading_messages_at_cap() {
    let (_dir, repo, _, owner, store) = fixture();
    repo.with_connection(move |c| {
        let tx = c.transaction().unwrap();
        let payload = serde_json::json!({"role":"tool","content":"x","calls":[],"call_id":null})
            .to_string();
        for ordinal in 0..ROWS_PAST_CAP {
            tx.execute("INSERT INTO messages(id,conversation_id,snapshot_revision,ordinal,payload) VALUES(?1,?2,1,?3,?4)",params![Uuid::new_v4().to_string(),id(owner),ordinal,payload]).unwrap();
        }
        tx.execute("INSERT INTO messages(id,conversation_id,snapshot_revision,ordinal,payload) VALUES(?1,?2,1,?3,'not json')",params![Uuid::new_v4().to_string(),id(owner),ROWS_PAST_CAP]).unwrap();
        tx.commit().unwrap();
        Ok(())
    })
    .unwrap();
    assert!(matches!(
        export_transcript(&repo, &store, owner, ExportOptions::default(), &Keys(None)).await,
        Err(CoreError::Limit)
    ));
    assert!(store.all().unwrap().is_empty());
}

#[tokio::test]
async fn history_export_checks_cap_on_every_attempt_append() {
    let (_dir, repo, _, owner, store) = fixture();
    let run = RunId::new();
    repo.insert_run_for_test(run, owner, RunState::Failed)
        .unwrap();
    repo.with_connection(move |c| {
        let tx = c.transaction().unwrap();
        for _ in 0..ROWS_PAST_CAP {
            tx.execute("INSERT INTO attempts(id,run_id,state,call_json,result_json) VALUES(?1,?2,'Failed',NULL,NULL)",params![Uuid::new_v4().to_string(),id(run)]).unwrap();
        }
        tx.execute("INSERT INTO attempts(id,run_id,state,call_json,result_json) VALUES(?1,?2,'Failed','not json',NULL)",params![Uuid::new_v4().to_string(),id(run)]).unwrap();
        tx.commit().unwrap();
        Ok(())
    })
    .unwrap();
    assert!(matches!(
        export_transcript(&repo, &store, owner, ExportOptions::default(), &Keys(None)).await,
        Err(CoreError::Limit)
    ));
    assert!(store.all().unwrap().is_empty());
}

#[tokio::test]
async fn history_export_refuses_credentials_configured_during_export() {
    let (_dir, repo, profile, owner, store) = fixture();
    configured_key(&repo, profile, owner);
    message(&repo, owner, "user", "late-secret");
    // Configures another credential while the first one resolves, as a user
    // saving a server mid-export would. Its value was never resolved, so it
    // could not be redacted: the export must refuse rather than leak it.
    struct Reconfigures(Repository);
    impl CredentialProvider for Reconfigures {
        fn resolve(
            &self,
            _: CredentialRef,
        ) -> Pin<Box<dyn Future<Output = CoreResult<SecretValue>> + Send + '_>> {
            Box::pin(async {
                self.0
                    .save_server(ServerConfig {
                        id: ServerId::new(),
                        name: "late".into(),
                        url: "https://late.test".into(),
                        allow_http: false,
                        secret_headers: vec![("X-Key".into(), CredentialRef::new())],
                    })
                    .unwrap();
                Ok(SecretValue::new("token".into()))
            })
        }
    }
    assert!(matches!(
        export_transcript(
            &repo,
            &store,
            owner,
            ExportOptions::default(),
            &Reconfigures(repo.clone())
        )
        .await,
        Err(CoreError::Conflict)
    ));
    assert!(store.all().unwrap().is_empty());
}

#[test]
fn history_export_queries_stream_from_indexes() {
    let (_dir, repo, _, owner, _) = fixture();
    let plans = repo
        .with_connection(move |c| {
            crate::history::HISTORY_QUERIES
                .iter()
                .map(|sql| {
                    let mut stmt = c.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
                    let detail = stmt
                        .query_map([id(owner)], |r| r.get::<_, String>(3))
                        .unwrap()
                        .collect::<Result<Vec<_>, _>>()
                        .unwrap();
                    Ok((sql.to_string(), detail))
                })
                .collect::<CoreResult<Vec<_>>>()
        })
        .unwrap();
    for (sql, detail) in plans {
        // A temporary B-tree means SQLite materialises every matching row before
        // returning the first; a SCAN reads every conversation's rows.
        assert!(
            detail
                .iter()
                .all(|d| d.starts_with("SEARCH") && !d.contains("TEMP B-TREE")),
            "{sql} is not streamed from an index: {detail:?}"
        );
    }
}
fn events(repo: &Repository, owner: ConversationId) -> Vec<String> {
    repo.with_connection(move |c| {
        let mut stmt = c
            .prepare("SELECT payload FROM events WHERE conversation_id=?1 ORDER BY id")
            .unwrap();
        let rows = stmt
            .query_map([id(owner)], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        Ok(rows)
    })
    .unwrap()
}
#[test]
fn archive_keeps_history_searchable_and_restore_clears_it() {
    let (_dir, repo, profile, owner, store) = fixture();
    let other = repo.create_conversation(profile).unwrap();
    message(&repo, owner, "user", "archived needle");
    let file = store.create(owner, "kept.txt", "kept").unwrap();
    let run = RunId::new();
    repo.insert_run_for_test(run, owner, RunState::Completed)
        .unwrap();
    repo.set_conversation_archived(owner, true).unwrap();

    // Still found by default search, marked, and kept with everything it owns.
    let found = repo.search_conversations("needle", false, 0, 100).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, owner);
    assert!(found[0].archived);
    let all = repo.search_conversations("", false, 0, 100).unwrap();
    assert_eq!(
        all.iter().map(|s| (s.id, s.archived)).collect::<Vec<_>>(),
        [(owner, true), (other, false)]
    );
    let archived = repo.search_conversations("", true, 0, 100).unwrap();
    assert_eq!(archived.iter().map(|s| s.id).collect::<Vec<_>>(), [owner]);
    assert_eq!(repo.conversation(owner).unwrap().messages.len(), 1);
    assert_eq!(store.read(owner, file.id).unwrap().1, "kept");
    let runs: i64 = repo
        .with_connection(move |c| {
            Ok(c.query_row(
                "SELECT count(*) FROM runs WHERE conversation_id=?1",
                [id(owner)],
                |r| r.get(0),
            )
            .unwrap())
        })
        .unwrap();
    assert_eq!(runs, 1);

    repo.set_conversation_archived(owner, false).unwrap();
    assert!(repo
        .search_conversations("", true, 0, 100)
        .unwrap()
        .is_empty());
    assert!(!repo.search_conversations("needle", false, 0, 100).unwrap()[0].archived);
    assert_eq!(
        events(&repo, owner)
            .into_iter()
            .filter(|e| e.starts_with("conversation-"))
            .collect::<Vec<_>>(),
        ["conversation-archived", "conversation-restored"]
    );
}
#[test]
fn archive_is_refused_while_a_run_is_running_and_repeats_change_nothing() {
    let (_dir, repo, _, owner, _store) = fixture();
    let run = RunId::new();
    repo.insert_run_for_test(run, owner, RunState::Running)
        .unwrap();
    assert_eq!(
        repo.set_conversation_archived(owner, true),
        Err(CoreError::Conflict)
    );
    assert!(repo
        .search_conversations("", true, 0, 100)
        .unwrap()
        .is_empty());
    repo.with_connection(move |c| {
        c.execute("UPDATE runs SET state='Cancelled' WHERE id=?1", [id(run)])
            .unwrap();
        Ok(())
    })
    .unwrap();
    // Unknown tool outcomes need no confirmation: archiving keeps the record.
    repo.insert_attempt_for_test(AttemptId::new(), run, AttemptState::OutcomeUnknown)
        .unwrap();
    let before = events(&repo, owner).len();
    repo.set_conversation_archived(owner, true).unwrap();
    repo.set_conversation_archived(owner, true).unwrap();
    assert_eq!(events(&repo, owner).len(), before + 1);
    repo.set_conversation_archived(owner, false).unwrap();
    repo.set_conversation_archived(owner, false).unwrap();
    assert_eq!(events(&repo, owner).len(), before + 2);
}
#[test]
fn archive_of_missing_conversation_is_a_conflict() {
    let (_dir, repo, _, _, _) = fixture();
    for archived in [true, false] {
        assert_eq!(
            repo.set_conversation_archived(ConversationId::new(), archived),
            Err(CoreError::Conflict)
        );
    }
}
