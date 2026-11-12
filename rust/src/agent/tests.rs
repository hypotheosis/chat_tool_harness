use super::context::{ContextChoiceReason, ContextSelection, TokenEstimation};

#[tokio::test]
async fn history_delete_waits_for_local_cleanup_even_after_terminal_checkpoint() {
    let mut f = Fixture::new();
    let store = crate::files::artifacts::ArtifactStore::open(
        f.repo.clone(),
        &f._dir.path().join("artifacts"),
    )
    .unwrap();
    let run = f.start().await;
    let request = f.request().await;
    assert_eq!(
        f.coordinator
            .delete_conversation(store.clone(), f.conversation, true)
            .await,
        Err(CoreError::Conflict)
    );
    state::cancel(&f.repo, run).unwrap();
    assert_eq!(f.data(run).await.state, RunState::Cancelled);
    // The DB is terminal, but the owned I/O/host execution still holds its slot.
    assert_eq!(
        f.coordinator
            .delete_conversation(store.clone(), f.conversation, true)
            .await,
        Err(CoreError::Conflict)
    );
    assert!(f.repo.conversation(f.conversation).is_ok());
    f.coordinator.cancel(run).await.unwrap();
    drop(request);
    f.coordinator
        .delete_conversation(store, f.conversation, false)
        .await
        .unwrap();
    assert!(f.repo.conversation(f.conversation).is_err());
    assert_eq!(
        f.coordinator
            .start_turn(f.conversation, "deleted".into())
            .await,
        Err(CoreError::Conflict)
    );
}

#[tokio::test]
async fn selected_reused_provider_id_cannot_rebind_persisted_results() {
    for interleaved in [false, true] {
        let mut f = Fixture::new();
        let a = f.start().await;
        let mut call_a = f.tool_call("shared-id");
        call_a.arguments_json = r#"{"value":"A"}"#.into();
        f.request().await.finish(Ok(turn(vec![call_a])));
        let binding_a = f.settled(a).await.attempts[0].binding.clone().unwrap();
        if !interleaved {
            f.approve(a, binding_a.clone()).await;
            f.calls
                .recv()
                .await
                .unwrap()
                .reply
                .send(Ok(ToolOutcome {
                    is_error: false,
                    text: vec!["RA".into()],
                    structured_content: None,
                    unsupported_content: vec![],
                }))
                .unwrap();
            f.request().await.finish(Ok(turn(vec![])));
            f.settled(a).await;
        }
        let b = f.start().await;
        if interleaved {
            // Both runs are real. An explicit selection lets B run while A awaits approval.
            f.idle().await;
            let input = f
                .repo
                .conversation(f.conversation)
                .unwrap()
                .messages
                .last()
                .unwrap()
                .id;
            f.coordinator
                .apply_context_selection(
                    b,
                    ContextSelection {
                        message_ids: vec![input],
                        summary_ids: vec![],
                    },
                )
                .await
                .unwrap();
            f.coordinator.resume(b).await.unwrap();
        }
        let mut call_b = f.tool_call("shared-id");
        call_b.arguments_json = r#"{"value":"B"}"#.into();
        f.request().await.finish(Ok(turn(vec![call_b])));
        let binding_b = f.settled(b).await.attempts[0].binding.clone().unwrap();
        if interleaved {
            f.approve(a, binding_a).await;
            f.calls
                .recv()
                .await
                .unwrap()
                .reply
                .send(Ok(ToolOutcome {
                    is_error: false,
                    text: vec!["RA".into()],
                    structured_content: None,
                    unsupported_content: vec![],
                }))
                .unwrap();
            assert_eq!(f.settled(a).await.state, RunState::Paused);
        }
        f.approve(b, binding_b).await;
        f.calls
            .recv()
            .await
            .unwrap()
            .reply
            .send(Ok(ToolOutcome {
                is_error: false,
                text: vec!["RB".into()],
                structured_content: None,
                unsupported_content: vec![],
            }))
            .unwrap();
        if interleaved {
            assert_eq!(f.settled(b).await.state, RunState::Paused);
        } else {
            f.request().await.finish(Ok(turn(vec![])));
            f.settled(b).await;
        }
        f.host.refuse.store(true, Ordering::SeqCst);
        let current = f.start().await;
        f.settled(current).await;
        f.host.refuse.store(false, Ordering::SeqCst);
        let before = f.repo.conversation(f.conversation).unwrap();
        let envelope_a = before
            .messages
            .iter()
            .find(|m| {
                m.message
                    .calls
                    .iter()
                    .any(|c| c.arguments_json == r#"{"value":"A"}"#)
            })
            .unwrap()
            .id;
        let envelope_b = before
            .messages
            .iter()
            .find(|m| {
                m.message
                    .calls
                    .iter()
                    .any(|c| c.arguments_json == r#"{"value":"B"}"#)
            })
            .unwrap()
            .id;
        let result_a = before
            .messages
            .iter()
            .find(|m| m.message.role == "tool" && m.message.content.contains("RA"))
            .unwrap()
            .id;
        let result_b = before
            .messages
            .iter()
            .find(|m| m.message.role == "tool" && m.message.content.contains("RB"))
            .unwrap()
            .id;
        let input = before.messages.last().unwrap().id;
        assert_eq!(
            before
                .messages
                .iter()
                .find(|m| m.id == result_a)
                .unwrap()
                .message
                .source_envelope,
            Some(envelope_a)
        );
        assert_eq!(
            before
                .messages
                .iter()
                .find(|m| m.id == result_b)
                .unwrap()
                .message
                .source_envelope,
            Some(envelope_b)
        );
        let attempts_a = f.data(a).await.attempts;
        let attempts_b = f.data(b).await.attempts;
        f.coordinator
            .apply_context_selection(
                current,
                ContextSelection {
                    message_ids: vec![envelope_a, result_b, input],
                    summary_ids: vec![],
                },
            )
            .await
            .unwrap();
        f.coordinator.resume(current).await.unwrap();
        f.idle().await;
        let rejected = f.data(current).await;
        assert_eq!(rejected.state, RunState::Paused);
        assert_eq!(rejected.inference_count, 0);
        assert_eq!(
            rejected.context.usage.unwrap().reason,
            Some(ContextChoiceReason::InvalidHistory)
        );
        assert!(f.requests.try_recv().is_err());
        let after = f.repo.conversation(f.conversation).unwrap();
        assert_eq!(after.messages, before.messages);
        assert_eq!(after.runs, before.runs);
        for (run, attempts) in [(a, attempts_a), (b, attempts_b)] {
            let after = f.data(run).await.attempts;
            assert_eq!(after.len(), attempts.len());
            for (after, before) in after.iter().zip(attempts) {
                assert_eq!(after.id, before.id);
                assert_eq!(after.state, before.state);
                assert_eq!(after.call, before.call);
                assert_eq!(after.binding, before.binding);
            }
        }
        // Correct pairs still work, including after a reopen and legitimate ID reuse.
        f.coordinator.shutdown().await.unwrap();
        f.reopen();
        f.coordinator.resume(current).await.unwrap();
        f.idle().await;
        for (envelope, result) in [(envelope_a, result_a), (envelope_b, result_b)] {
            f.coordinator
                .apply_context_selection(
                    current,
                    ContextSelection {
                        message_ids: vec![envelope, result, input],
                        summary_ids: vec![],
                    },
                )
                .await
                .unwrap();
            f.coordinator.resume(current).await.unwrap();
            let request = f.request().await;
            request.request.validate().unwrap();
            assert_eq!(
                request
                    .request
                    .messages
                    .iter()
                    .filter(|m| matches!(m.role, ChatRole::Tool))
                    .count(),
                1
            );
            request.finish(Err(CoreError::ContextLength));
            f.settled(current).await;
        }
        assert_eq!(f.writes.load(Ordering::SeqCst), 2);
    }
}
use super::*;

#[tokio::test]
async fn explicit_summary_can_shorten_completed_current_run_exchange_and_keep_new_calls() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let binding = f.waiting(run).await;
    f.approve(run, binding).await;
    f.calls
        .recv()
        .await
        .unwrap()
        .reply
        .send(Ok(outcome(false, 200)))
        .unwrap();
    f.request().await.finish(Err(CoreError::ContextLength));
    f.settled(run).await;
    let snapshot = f.repo.conversation(f.conversation).unwrap();
    let input = snapshot.messages[0].id;
    let summary = f
        .coordinator
        .summarize_context(
            f.conversation,
            ContextSelection {
                message_ids: snapshot.messages.iter().skip(1).map(|m| m.id).collect(),
                summary_ids: vec![],
            },
        )
        .await
        .unwrap();
    f.request().await.finish(Ok(turn(vec![])));
    f.settled(summary).await;
    let derived = f.repo.conversation(f.conversation).unwrap().summaries[0].id;
    f.coordinator
        .apply_context_selection(
            run,
            ContextSelection {
                message_ids: vec![input],
                summary_ids: vec![derived],
            },
        )
        .await
        .unwrap();
    f.coordinator.resume(run).await.unwrap();
    let request = f.request().await;
    assert!(!request
        .request
        .messages
        .iter()
        .any(|m| matches!(m.role, ChatRole::Tool)));
    assert!(request
        .request
        .messages
        .iter()
        .any(|m| m.content == "do it"));
    request.finish(Ok(turn(vec![f.tool_call("new-call")])));
    let binding = f
        .settled(run)
        .await
        .attempts
        .last()
        .unwrap()
        .binding
        .clone()
        .unwrap();
    f.approve(run, binding).await;
    f.calls
        .recv()
        .await
        .unwrap()
        .reply
        .send(Ok(outcome(false, 2)))
        .unwrap();
    let request = f.request().await;
    assert_eq!(
        request
            .request
            .messages
            .iter()
            .filter_map(|m| m.tool_call_id.as_deref())
            .collect::<Vec<_>>(),
        vec!["new-call"]
    );
    request.request.validate().unwrap();
    request.finish(Ok(turn(vec![])));
    f.settled(run).await;
    assert_eq!(f.writes.load(Ordering::SeqCst), 2);
    let after = f.repo.conversation(f.conversation).unwrap();
    assert!(after
        .messages
        .iter()
        .any(|m| m.message.call_id.as_deref() == Some("id-1")));
}

#[tokio::test]
async fn summary_context_budget_and_cancel_preserve_sources_and_partial_text() {
    let mut f = Fixture::new();
    let old = f.start().await;
    f.request().await.finish(Ok(turn(vec![])));
    f.settled(old).await;
    let before = f.repo.conversation(f.conversation).unwrap();
    let selection = ContextSelection {
        message_ids: vec![before.messages[0].id],
        summary_ids: vec![],
    };
    f.set_context_tokens(32);
    let over = f
        .coordinator
        .summarize_context(f.conversation, selection.clone())
        .await
        .unwrap();
    f.idle().await;
    let d = f.data(over).await;
    assert_eq!(d.state, RunState::Paused);
    assert_eq!(d.inference_count, 0);
    assert_eq!(
        d.context.usage.unwrap().reason,
        Some(ContextChoiceReason::WindowExceeded)
    );
    assert!(f.requests.try_recv().is_err());
    f.set_context_tokens(1_000_000);
    let run = f
        .coordinator
        .summarize_context(f.conversation, selection)
        .await
        .unwrap();
    let request = f.request().await;
    request.output("unfinished summary").await;
    f.clock.advance(Duration::from_secs(3));
    for _ in 0..10000 {
        if f.data(run).await.partial_text == "unfinished summary" {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(f.data(run).await.partial_text, "unfinished summary");
    f.coordinator.cancel(run).await.unwrap();
    drop(request);
    let data = f.data(run).await;
    assert_eq!(data.state, RunState::Cancelled);
    assert_eq!(data.inference_count, 1);
    assert_eq!(data.partial_text, "unfinished summary");
    assert_eq!(data.active_ms, 3000);
    let after = f.repo.conversation(f.conversation).unwrap();
    assert_eq!(after.messages, before.messages);
    assert!(after.summaries.is_empty());
    assert_eq!(f.writes.load(Ordering::SeqCst), 0);
    assert!(!f.host.active.load(Ordering::SeqCst));
}

#[tokio::test]
async fn summary_publication_failure_rolls_back_and_poison_is_sticky() {
    let mut f = Fixture::new();
    let old = f.start().await;
    f.request().await.finish(Ok(turn(vec![])));
    f.settled(old).await;
    let before = f.repo.conversation(f.conversation).unwrap();
    let run = f
        .coordinator
        .summarize_context(
            f.conversation,
            ContextSelection {
                message_ids: vec![before.messages[0].id],
                summary_ids: vec![],
            },
        )
        .await
        .unwrap();
    let request = f.request().await;
    f.sql("CREATE TRIGGER fail_summary AFTER INSERT ON derived_context BEGIN SELECT RAISE(FAIL,'publication'); END;").await;
    request.finish(Ok(turn(vec![])));
    f.idle().await;
    let after = f.repo.conversation(f.conversation).unwrap();
    assert_eq!(after.messages, before.messages);
    assert!(after.summaries.is_empty());
    assert!(f.data(run).await.context.derived_entry.is_none());
    assert_eq!(
        f.coordinator
            .start_turn(f.conversation, "do not send".into())
            .await,
        Err(CoreError::Storage)
    );
    assert!(!f.host.active.load(Ordering::SeqCst));
}

#[tokio::test]
async fn length_limited_summary_discloses_incomplete_coverage() {
    let mut f = Fixture::new();
    let original = f.start().await;
    f.request().await.finish(Ok(turn(vec![])));
    f.settled(original).await;
    let snapshot = f.repo.conversation(f.conversation).unwrap();
    let run = f
        .coordinator
        .summarize_context(
            f.conversation,
            ContextSelection {
                message_ids: snapshot.messages.iter().map(|m| m.id).collect(),
                summary_ids: vec![],
            },
        )
        .await
        .unwrap();
    let mut result = turn(vec![]);
    result.finish_reason = "length".into();
    f.request().await.finish(Ok(result));
    f.settled(run).await;
    assert!(f.repo.conversation(f.conversation).unwrap().summaries[0].partial_coverage);
}

#[tokio::test]
async fn context_snapshot_exposes_owned_message_refs_and_derived_entries() {
    let mut f = Fixture::new();
    let run = f.start().await;
    f.request().await.finish(Ok(turn(vec![])));
    f.settled(run).await;
    let snap = f
        .coordinator
        .context_snapshot(f.conversation)
        .await
        .unwrap();
    assert_eq!(snap.messages.len(), 2);
    assert_ne!(snap.messages[0].id, snap.messages[1].id);
    let second = f.start().await;
    let r = f.request().await;
    assert_eq!(
        r.request
            .messages
            .iter()
            .filter(|m| m.content == "do it")
            .count(),
        2
    );
    r.finish(Ok(turn(vec![])));
    f.settled(second).await;
}

#[tokio::test]
async fn summary_disclosure_is_durable_per_origin_and_never_an_mcp_allow() {
    let mut f = Fixture::new();
    let old = f.start().await;
    let binding = f.waiting(old).await;
    // Future document adapters will produce these derived references; no PDF I/O here.
    let source = context::DerivedContext {
        id: MessageId::new(),
        run_id: old,
        text: "historical document text".into(),
        selection: ContextSelection {
            message_ids: vec![],
            summary_ids: vec![],
        },
        page_references: vec![context::PageReference {
            attachment_id: AttachmentId::new(),
            physical_page: 2,
        }],
        partial_coverage: true,
        unresolved_attempts: vec![binding.attempt_id],
    };
    let payload = serde_json::to_string(&source).unwrap();
    let repo = f.repo.clone();
    let conversation = f.conversation;
    db(move ||repo.with_connection(move |conn| {
        conn.execute("INSERT INTO derived_context(id,conversation_id,run_id,payload) VALUES(?1,?2,?3,?4)",rusqlite::params![source.id.0.to_string(),conversation.0.to_string(),old.0.to_string(),payload]).map_err(|_|CoreError::Storage)?; Ok(())
    })).await.unwrap();
    let source_id = f.repo.conversation(f.conversation).unwrap().summaries[0].id;
    let selection = ContextSelection {
        message_ids: vec![],
        summary_ids: vec![source_id],
    };
    let run = f
        .coordinator
        .summarize_context(f.conversation, selection)
        .await
        .unwrap();
    f.idle().await;
    let data = f.data(run).await;
    assert_eq!(
        data.context.usage.unwrap().reason,
        Some(ContextChoiceReason::DisclosureRequired)
    );
    assert_eq!(data.inference_count, 0);
    assert!(f.requests.try_recv().is_err());
    assert_eq!(f.live.load(Ordering::SeqCst), 0);
    assert_eq!(
        f.coordinator
            .acknowledge_context_disclosure(
                f.conversation,
                context::DisclosureAcknowledgment {
                    origin: "https://wrong.test".into()
                }
            )
            .await,
        Err(CoreError::InvalidInput)
    );
    let origin = context::endpoint_origin(
        &f.repo
            .conversation(f.conversation)
            .unwrap()
            .profile
            .endpoint,
    )
    .unwrap();
    let mut endpoint = f
        .repo
        .conversation(f.conversation)
        .unwrap()
        .profile
        .endpoint;
    endpoint.base_url = "https://other.test".into();
    f.repo.save_endpoint(endpoint.clone()).unwrap();
    f.repo.apply_profile(f.conversation, f.profile).unwrap();
    // The paused run's origin can still be acknowledged after applying a profile.
    f.coordinator
        .acknowledge_context_disclosure(
            f.conversation,
            context::DisclosureAcknowledgment { origin },
        )
        .await
        .unwrap();
    f.coordinator.shutdown().await.unwrap();
    f.reopen();
    // Resume follows current settings: the run is rebound to the edited endpoint,
    // whose new host has not been acknowledged yet.
    f.coordinator.resume(run).await.unwrap();
    f.idle().await;
    assert_eq!(
        f.data(run).await.context.usage.unwrap().reason,
        Some(ContextChoiceReason::DisclosureRequired)
    );
    assert!(f.requests.try_recv().is_err());
    assert_eq!(f.repo.run_profile_revision(run).unwrap(), 2);
    let current = f.repo.conversation(f.conversation).unwrap().profile;
    f.coordinator
        .acknowledge_context_disclosure(
            f.conversation,
            context::DisclosureAcknowledgment {
                origin: context::endpoint_origin(&current.endpoint).unwrap(),
            },
        )
        .await
        .unwrap();
    f.coordinator.resume(run).await.unwrap();
    let r = f.request().await;
    assert_eq!(r.request.endpoint.base_url, "https://other.test");
    assert!(r.request.tools.is_empty());
    r.finish(Ok(turn(vec![])));
    f.settled(run).await;
    let sources = f.repo.conversation(f.conversation).unwrap();
    assert_eq!(
        sources.summaries[1].page_references,
        sources.summaries[0].page_references
    );
    // Returning to an acknowledged origin with a new path and model does not
    // re-prompt, even though the snapshot revision changes.
    endpoint.base_url = "https://model.test/v2".into();
    endpoint.model = "other-model".into();
    f.repo.save_endpoint(endpoint).unwrap();
    let run = f
        .coordinator
        .summarize_context(
            f.conversation,
            ContextSelection {
                message_ids: vec![],
                summary_ids: vec![source_id],
            },
        )
        .await
        .unwrap();
    let r = f.request().await;
    assert_eq!(r.request.endpoint.base_url, "https://model.test/v2");
    assert!(r.request.tools.is_empty());
    r.finish(Ok(turn(vec![])));
    f.settled(run).await;
    assert!(f.repo.run_profile_revision(run).unwrap() > 2);
    // Disclosure never acts as an MCP allow: the old approval still waits,
    // no tool ran and no permission rule was recorded.
    assert_eq!(
        f.data(old).await.attempts[0].binding.as_ref(),
        Some(&binding)
    );
    assert_eq!(f.writes.load(Ordering::SeqCst), 0);
    assert!(f.calls.try_recv().is_err());
    let repo = f.repo.clone();
    let rules: i64 = db(move || {
        repo.with_connection(|conn| {
            conn.query_row("SELECT count(*) FROM tool_permission_rules", [], |r| {
                r.get(0)
            })
            .map_err(|_| CoreError::Storage)
        })
    })
    .await
    .unwrap();
    assert_eq!(rules, 0);
}

#[tokio::test]
async fn disclosure_acknowledgment_skips_unreadable_old_revisions() {
    let f = Fixture::new();
    let mut endpoint = f
        .repo
        .conversation(f.conversation)
        .unwrap()
        .profile
        .endpoint;
    endpoint.base_url = "https://other.example.com/v1".into();
    f.repo.save_endpoint(endpoint).unwrap();
    f.repo.apply_profile(f.conversation, f.profile).unwrap();
    f.sql(&format!(
        "UPDATE conversation_snapshots SET payload='not json' WHERE conversation_id='{}' AND revision=1",
        f.conversation.0
    ))
    .await;
    let acknowledge = |origin: &str| {
        f.coordinator.acknowledge_context_disclosure(
            f.conversation,
            context::DisclosureAcknowledgment {
                origin: origin.into(),
            },
        )
    };
    assert_eq!(acknowledge("https://other.example.com").await, Ok(()));
    // Only the unreadable revision used this origin, so nothing readable matches.
    assert_eq!(
        acknowledge("https://model.test").await,
        Err(CoreError::InvalidInput)
    );
    assert_eq!(
        acknowledge("https://never.example.com").await,
        Err(CoreError::InvalidInput)
    );
}

#[tokio::test]
async fn disclosure_is_acknowledged_once_per_origin_across_conversations() {
    let f = Fixture::new();
    let reason = |snapshot: &ConversationSnapshot,
                  disclosure: Option<context::DisclosureAcknowledgment>| {
        let request = context::ContextRequest {
            selection: None,
            attachments: vec![context::AttachmentContext {
                conversation_id: snapshot.id,
                attachment_id: AttachmentId::new(),
                filename: "notes.txt".into(),
                total_pages: 1,
                extraction_status: "complete".into(),
                excerpts: vec![context::PageExcerpt {
                    physical_page: 1,
                    text: "document text".into(),
                }],
            }],
            purpose: context::ContextPurpose::Turn,
            disclosure,
            calibration: None,
            deadline: Instant::now(),
        };
        context::assemble_with_usage(snapshot, &[], &request)
            .unwrap()
            .1
            .reason
    };
    let first = f.repo.conversation(f.conversation).unwrap();
    assert_eq!(state::disclosure(&f.repo, &first.profile).unwrap(), None);
    assert_eq!(
        reason(&first, None),
        Some(ContextChoiceReason::DisclosureRequired)
    );
    let acknowledgment = context::DisclosureAcknowledgment {
        origin: "https://model.test".into(),
    };
    for _ in 0..2 {
        // Acknowledging the same origin again is harmless.
        f.coordinator
            .acknowledge_context_disclosure(f.conversation, acknowledgment.clone())
            .await
            .unwrap();
    }
    let mut endpoint = first.profile.endpoint.clone();
    endpoint.base_url = "https://model.test/v2".into();
    f.repo.save_endpoint(endpoint.clone()).unwrap();
    let second = f
        .repo
        .conversation(f.repo.create_conversation(f.profile).unwrap())
        .unwrap();
    let disclosure = state::disclosure(&f.repo, &second.profile).unwrap();
    assert_eq!(disclosure.as_ref(), Some(&acknowledgment));
    assert_eq!(reason(&second, disclosure), None);
    endpoint.base_url = "https://other.example.com/v1".into();
    f.repo.save_endpoint(endpoint).unwrap();
    let third = f
        .repo
        .conversation(f.repo.create_conversation(f.profile).unwrap())
        .unwrap();
    let disclosure = state::disclosure(&f.repo, &third.profile).unwrap();
    assert_eq!(disclosure, None);
    assert_eq!(
        reason(&third, disclosure),
        Some(ContextChoiceReason::DisclosureRequired)
    );
}

#[tokio::test]
async fn provider_context_rejection_preserves_input() {
    let mut f = Fixture::new();
    let run = f.start().await;
    f.request().await.finish(Err(CoreError::ContextLength));
    let data = f.settled(run).await;
    assert_eq!(data.state, RunState::Paused);
    assert_eq!(data.messages[0].content, "do it");
    assert_eq!(
        data.context.usage.unwrap().reason,
        Some(ContextChoiceReason::ProviderRejected)
    );
    f.coordinator.shutdown().await.unwrap();
    f.reopen();
    assert_eq!(f.data(run).await.messages[0].content, "do it");
    assert!(f.requests.try_recv().is_err());
}

#[tokio::test]
async fn summary_keeps_original_and_unresolved_attempts() {
    let mut f = Fixture::new();
    let original = f.start().await;
    let approval = f.waiting(original).await;
    let before = f.repo.conversation(f.conversation).unwrap();
    let selected = ContextSelection {
        message_ids: before.messages.iter().map(|m| m.id).collect(),
        summary_ids: vec![],
    };
    let run = f
        .coordinator
        .summarize_context(f.conversation, selected.clone())
        .await
        .unwrap();
    let request = f.request().await;
    assert!(request.request.tools.is_empty());
    assert!(request
        .request
        .messages
        .iter()
        .all(|m| m.tool_calls.is_empty()));
    assert!(request
        .request
        .messages
        .iter()
        .any(|m| m.content.contains(&approval.attempt_id.0.to_string())));
    let mut result = turn(vec![]);
    result.text = "derived old observation".into();
    result.usage = Some(crate::inference::Usage {
        prompt_tokens: 45,
        completion_tokens: 5,
    });
    request.finish(Ok(result));
    let data = f.settled(run).await;
    assert_eq!(data.state, RunState::Completed);
    assert_eq!(data.inference_count, 1);
    let usage = data.context.usage.unwrap();
    assert_eq!(usage.estimation, TokenEstimation::ConservativeUtf8Bytes);
    assert_eq!(usage.provider.unwrap().prompt_tokens, 45);
    let after = f.repo.conversation(f.conversation).unwrap();
    assert_eq!(
        serde_json::to_string(&before.messages).unwrap(),
        serde_json::to_string(&after.messages).unwrap()
    );
    assert_eq!(after.summaries.len(), 1);
    assert_eq!(after.summaries[0].selection, selected);
    assert_eq!(
        after.summaries[0].unresolved_attempts,
        vec![approval.attempt_id]
    );
    assert_eq!(
        f.data(original).await.attempts[0].binding.as_ref(),
        Some(&approval)
    );
    assert_eq!(f.live.load(Ordering::SeqCst), 0);
    f.coordinator.resume(original).await.unwrap();
    assert_eq!(f.settled(original).await.state, RunState::WaitingApproval);
    assert!(f.requests.try_recv().is_err());
    assert_eq!(f.writes.load(Ordering::SeqCst), 0);
    f.coordinator.shutdown().await.unwrap();
    f.reopen();
    assert_eq!(
        f.repo.conversation(f.conversation).unwrap().summaries.len(),
        1
    );
}

#[tokio::test]
async fn summary_explicit_selection_partial_coverage_and_apply() {
    let mut f = Fixture::new();
    let old = f.start().await;
    f.request().await.finish(Ok(turn(vec![])));
    f.settled(old).await;
    let snap = f.repo.conversation(f.conversation).unwrap();
    let selection = ContextSelection {
        message_ids: vec![snap.messages[0].id],
        summary_ids: vec![],
    };
    let summary = f
        .coordinator
        .summarize_context(f.conversation, selection)
        .await
        .unwrap();
    f.request().await.finish(Ok(turn(vec![])));
    f.settled(summary).await;
    let snap = f.repo.conversation(f.conversation).unwrap();
    assert!(snap.summaries[0].partial_coverage);
    let id = snap.summaries[0].id;
    f.set_context_tokens(32);
    let run = f.start().await;
    f.idle().await;
    assert_eq!(f.data(run).await.state, RunState::Paused);
    let current = f
        .repo
        .conversation(f.conversation)
        .unwrap()
        .messages
        .last()
        .unwrap()
        .id;
    assert_eq!(
        f.coordinator
            .apply_context_selection(
                run,
                ContextSelection {
                    message_ids: vec![],
                    summary_ids: vec![id]
                }
            )
            .await,
        Err(CoreError::InvalidInput)
    );
    f.coordinator
        .apply_context_selection(
            run,
            ContextSelection {
                message_ids: vec![current],
                summary_ids: vec![id],
            },
        )
        .await
        .unwrap();
    // Restore the endpoint window in settings; resume refreshes the run's snapshot without changing its context selection.
    f.set_context_tokens(1_000_000);
    f.coordinator.resume(run).await.unwrap();
    let r = f.request().await;
    assert_eq!(f.live.load(Ordering::SeqCst), 1);
    assert_eq!(r.request.tools.len(), 1);
    assert_eq!(r.request.tools[0].key.name, "write");
    assert!(r
        .request
        .messages
        .iter()
        .any(|m| m.content.contains("not fresh")));
    assert!(!r
        .request
        .messages
        .iter()
        .any(|m| matches!(m.role, ChatRole::Tool)));
    r.finish(Ok(turn(vec![])));
    f.settled(run).await;
}

#[tokio::test]
async fn summary_obeys_run_budgets_authentication_and_no_tool_dispatch() {
    for failure in [CoreError::Authentication, CoreError::ContextLength] {
        let mut f = Fixture::new();
        let old = f.start().await;
        f.request().await.finish(Ok(turn(vec![])));
        f.settled(old).await;
        let selection = ContextSelection {
            message_ids: vec![f.repo.conversation(f.conversation).unwrap().messages[0].id],
            summary_ids: vec![],
        };
        let run = f
            .coordinator
            .summarize_context(f.conversation, selection)
            .await
            .unwrap();
        f.request().await.finish(Err(failure));
        let d = f.settled(run).await;
        assert!(matches!(d.state, RunState::Failed | RunState::Paused));
        assert!(f
            .repo
            .conversation(f.conversation)
            .unwrap()
            .summaries
            .is_empty());
        assert_eq!(f.writes.load(Ordering::SeqCst), 0);
    }
    let mut f = Fixture::new();
    let old = f.start().await;
    f.request().await.finish(Ok(turn(vec![])));
    f.settled(old).await;
    let selection = ContextSelection {
        message_ids: vec![f.repo.conversation(f.conversation).unwrap().messages[0].id],
        summary_ids: vec![],
    };
    let run = f
        .coordinator
        .summarize_context(f.conversation, selection)
        .await
        .unwrap();
    f.request()
        .await
        .finish(Err(CoreError::UnavailableTransport));
    f.settled(run).await;
    f.sql(&format!(
        "UPDATE runs SET inference_count=20 WHERE id='{}'",
        run.0
    ))
    .await;
    f.coordinator.resume(run).await.unwrap();
    f.idle().await;
    assert_eq!(f.data(run).await.state, RunState::Failed);
    assert!(f.requests.try_recv().is_err());
    let run = f
        .coordinator
        .summarize_context(
            f.conversation,
            ContextSelection {
                message_ids: vec![f.repo.conversation(f.conversation).unwrap().messages[0].id],
                summary_ids: vec![],
            },
        )
        .await
        .unwrap();
    let request = f.request().await;
    request.output("summary").await;
    f.clock.advance(Duration::from_secs(900));
    assert_eq!(f.settled(run).await.state, RunState::Failed);
    drop(request);
    assert_eq!(f.writes.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn summary_rejects_foreign_refs_and_hallucinated_tool_calls() {
    let mut f = Fixture::new();
    let other = f.repo.create_conversation(f.profile).unwrap();
    let foreign = f
        .coordinator
        .start_turn(other, "foreign conversation".into())
        .await
        .unwrap();
    f.request().await.finish(Ok(turn(vec![])));
    f.settled(foreign).await;
    let foreign_id = f.repo.conversation(other).unwrap().messages[0].id;
    assert_eq!(
        f.coordinator
            .summarize_context(
                f.conversation,
                ContextSelection {
                    message_ids: vec![foreign_id],
                    summary_ids: vec![]
                }
            )
            .await,
        Err(CoreError::InvalidInput)
    );
    let invalid = ContextSelection {
        message_ids: vec![MessageId::new()],
        summary_ids: vec![],
    };
    assert_eq!(
        f.coordinator
            .summarize_context(f.conversation, invalid)
            .await,
        Err(CoreError::InvalidInput)
    );
    let old = f.start().await;
    f.request().await.finish(Ok(turn(vec![])));
    f.settled(old).await;
    let selection = ContextSelection {
        message_ids: vec![f.repo.conversation(f.conversation).unwrap().messages[0].id],
        summary_ids: vec![],
    };
    let run = f
        .coordinator
        .summarize_context(f.conversation, selection)
        .await
        .unwrap();
    f.request()
        .await
        .finish(Ok(turn(vec![f.tool_call("not allowed")])));
    assert_eq!(f.settled(run).await.state, RunState::Failed);
    assert_eq!(f.writes.load(Ordering::SeqCst), 0);
    assert!(f
        .repo
        .conversation(f.conversation)
        .unwrap()
        .summaries
        .is_empty());
}

#[tokio::test]
async fn over_budget_never_silently_drops_messages() {
    let mut f = Fixture::new();
    f.set_context_tokens(32);
    let run = f.start().await;
    // Waiting for slot release, not a request: over-budget input must never reach inference.
    f.idle().await;
    let data = f.data(run).await;
    assert_eq!(data.state, RunState::Paused);
    assert_eq!(data.inference_count, 0);
    assert_eq!(data.messages[0].content, "do it");
    assert!(f.requests.try_recv().is_err());
}

#[tokio::test]
async fn provider_usage_calibrates_the_next_turn_estimate() {
    // Regression for issue #12: a later turn must use the rate the provider
    // reported for an earlier one, not one token per byte.
    let mut f = Fixture::new();
    let first = f.start().await;
    let request = f.request().await;
    let mut result = turn(vec![]);
    let first_usage = f.data(first).await.context.usage.unwrap();
    assert_eq!(
        first_usage.estimation,
        TokenEstimation::ConservativeUtf8Bytes
    );
    let first_bytes = first_usage.raw_bytes;
    result.usage = Some(crate::inference::Usage {
        prompt_tokens: first_bytes / 3,
        completion_tokens: 1,
    });
    request.finish(Ok(result));
    assert_eq!(f.settled(first).await.state, RunState::Completed);
    // The byte bound for the next, larger request no longer fits this window,
    // but the calibrated estimate does.
    let reserve = u64::from(first_usage.output_reserve);
    f.set_context_tokens(u32::try_from(first_bytes + 256 + reserve).unwrap());
    let second = f
        .coordinator
        .start_turn(f.conversation, "again".into())
        .await
        .unwrap();
    let request = f.request().await;
    let usage = f.data(second).await.context.usage.unwrap();
    assert_eq!(usage.estimation, TokenEstimation::CalibratedUtf8Bytes);
    assert!(usage.raw_bytes + 256 + reserve > u64::from(usage.window));
    let sample = context::ContextCalibration {
        raw_bytes: first_bytes,
        prompt_tokens: first_bytes / 3,
    };
    assert_eq!(
        usage.estimated_input_tokens,
        context::estimate_input_tokens(usage.raw_bytes, Some(&sample)).0
    );
    assert_eq!(usage.reason, None);
    request.finish(Ok(turn(vec![])));
    assert_eq!(f.settled(second).await.state, RunState::Completed);
}

#[tokio::test]
async fn calibration_survives_a_later_request_that_is_interrupted() {
    // A tool round-trip sends a second, larger request. Admitting it replaces the
    // run's usage before the provider reports, so an interruption must not lose
    // the sample that admitted it.
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    let first_bytes = f.data(run).await.context.usage.unwrap().raw_bytes;
    let mut result = turn(vec![f.tool_call("id-1")]);
    result.usage = Some(crate::inference::Usage {
        prompt_tokens: first_bytes / 3,
        completion_tokens: 1,
    });
    request.finish(Ok(result));
    let binding = f.settled(run).await.attempts[0].binding.clone().unwrap();
    f.approve(run, binding).await;
    f.calls
        .recv()
        .await
        .unwrap()
        .reply
        .send(Ok(outcome(false, 2000)))
        .unwrap();
    let second = f.request().await;
    let usage = f.data(run).await.context.usage.unwrap();
    assert_eq!(usage.estimation, TokenEstimation::CalibratedUtf8Bytes);
    assert!(usage.raw_bytes > first_bytes);
    second.finish(Err(CoreError::UnavailableTransport));
    assert_eq!(f.settled(run).await.state, RunState::Paused);
    let sample = context::ContextCalibration {
        raw_bytes: first_bytes,
        prompt_tokens: first_bytes / 3,
    };
    let repo = f.repo.clone();
    let conversation = f.conversation;
    assert_eq!(
        db(move || state::latest_calibration(&repo, conversation))
            .await
            .unwrap(),
        Some(sample)
    );
    // Shrink the window below the byte bound of the resumed request.
    let reserve = u64::from(usage.output_reserve);
    f.set_context_tokens(u32::try_from(usage.raw_bytes + 256 + reserve - 1).unwrap());
    f.coordinator.resume(run).await.unwrap();
    let third = f.request().await;
    let usage = f.data(run).await.context.usage.unwrap();
    assert!(usage.raw_bytes + 256 + reserve > u64::from(usage.window));
    assert_eq!(usage.estimation, TokenEstimation::CalibratedUtf8Bytes);
    assert_eq!(
        usage.estimated_input_tokens,
        context::estimate_input_tokens(usage.raw_bytes, Some(&sample)).0
    );
    assert_eq!(usage.reason, None);
    third.finish(Ok(turn(vec![])));
    assert_eq!(f.settled(run).await.state, RunState::Completed);
}

#[tokio::test]
async fn latest_calibration_selects_the_newest_usable_sample() {
    fn usage(
        raw_bytes: u64,
        lower_bound: bool,
        prompt_tokens: Option<u64>,
    ) -> context::ContextUsage {
        context::ContextUsage {
            estimation: TokenEstimation::ConservativeUtf8Bytes,
            estimated_input_tokens: raw_bytes + 256,
            output_reserve: 0,
            window: 1_000_000,
            raw_bytes,
            raw_bytes_is_lower_bound: lower_bound,
            provider: prompt_tokens.map(|prompt_tokens| context::ProviderUsage {
                prompt_tokens,
                completion_tokens: 1,
            }),
            reason: None,
        }
    }
    /// Records each usage, in order, on a new run in `conversation`.
    fn record(
        repo: &Repository,
        conversation: ConversationId,
        usages: Vec<context::ContextUsage>,
    ) -> CoreResult<()> {
        let run = RunId::new();
        repo.with_connection(move |c| {
            c.execute(
                "INSERT INTO runs(id,conversation_id,snapshot_revision,state,context_json) \
                 VALUES(?1,?2,1,'Completed','{}')",
                rusqlite::params![run.0.to_string(), conversation.0.to_string()],
            )
            .map(|_| ())
            .map_err(|_| CoreError::Storage)
        })?;
        for usage in usages {
            state::context_usage(repo, run, usage)?;
        }
        Ok(())
    }
    let valid = context::ContextCalibration {
        raw_bytes: 900,
        prompt_tokens: 300,
    };
    let unusable = [
        ("no provider usage", usage(1000, false, None)),
        ("zero prompt tokens", usage(1000, false, Some(0))),
        ("zero bytes", usage(0, false, Some(100))),
        ("lower-bound bytes", usage(1000, true, Some(100))),
    ];
    for (case, newer) in unusable {
        let f = Fixture::new();
        let (repo, conversation) = (f.repo.clone(), f.conversation);
        let other = f.repo.create_conversation(f.profile).unwrap();
        let found = db(move || {
            assert_eq!(state::latest_calibration(&repo, conversation)?, None);
            record(&repo, conversation, vec![usage(900, false, Some(300))])?;
            record(&repo, conversation, vec![newer.clone()])?;
            // A newer unusable request in the same run keeps the run's sample.
            record(
                &repo,
                conversation,
                vec![usage(900, false, Some(300)), newer.clone()],
            )?;
            record(&repo, other, vec![usage(100, false, Some(99))])?;
            let only_unusable = repo.create_conversation(f.profile)?;
            record(&repo, only_unusable, vec![newer])?;
            Ok((
                state::latest_calibration(&repo, conversation)?,
                state::latest_calibration(&repo, only_unusable)?,
            ))
        })
        .await
        .unwrap();
        assert_eq!(found, (Some(valid), None), "{case}");
    }
    // The newest of several usable samples wins, and other conversations are
    // never consulted.
    let f = Fixture::new();
    let (repo, conversation) = (f.repo.clone(), f.conversation);
    let other = f.repo.create_conversation(f.profile).unwrap();
    let found = db(move || {
        record(&repo, conversation, vec![usage(900, false, Some(300))])?;
        record(&repo, conversation, vec![usage(800, false, Some(400))])?;
        record(&repo, other, vec![usage(100, false, Some(99))])?;
        state::latest_calibration(&repo, conversation)
    })
    .await
    .unwrap();
    assert_eq!(
        found,
        Some(context::ContextCalibration {
            raw_bytes: 800,
            prompt_tokens: 400,
        })
    );
}

#[tokio::test]
async fn resumed_inference_replaces_previous_partial_only_at_admission() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let first = f.request().await;
    first.output("old").await;
    f.clock.advance(Duration::from_secs(3));
    first.finish(Err(CoreError::UnavailableTransport));
    let data = f.settled(run).await;
    assert_eq!(data.state, RunState::Paused);
    assert_eq!(data.partial_text, "old");
    assert_eq!(data.inference_count, 1);
    assert_eq!(data.active_ms, 3000);

    f.coordinator.shutdown().await.unwrap();
    f.reopen();
    f.clock.advance(Duration::from_secs(3600));
    let data = f.data(run).await;
    assert_eq!(data.state, RunState::Interrupted);
    assert_eq!(data.partial_text, "old");
    assert_eq!(data.inference_count, 1);
    assert_eq!(data.active_ms, 3000);
    assert!(f.requests.try_recv().is_err());

    f.coordinator.resume(run).await.unwrap();
    let second = f.request().await;
    let admitted = f.data(run).await;
    assert_eq!(admitted.inference_count, 2);
    assert_eq!(admitted.active_ms, 3000);
    second.output("new").await;
    f.clock.advance(Duration::from_secs(5));
    second.finish(Err(CoreError::UnavailableTransport));
    let data = f.settled(run).await;
    assert_eq!(data.state, RunState::Paused);
    assert_eq!(data.partial_text, "new");
    assert_eq!(admitted.partial_text, "");
    assert_eq!(data.inference_count, 2);
    assert_eq!(data.active_ms, 8000);
    assert_eq!(data.attempt_count, 0);
    assert_eq!(f.writes.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn failed_inference_admission_keeps_previous_partial_and_request_count() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let first = f.request().await;
    first
        .events
        .send(InferenceEvent::Text("old".into()))
        .await
        .unwrap();
    first.finish(Err(CoreError::UnavailableTransport));
    assert_eq!(f.settled(run).await.partial_text, "old");
    f.sql("CREATE TRIGGER fail_admission AFTER UPDATE OF inference_count ON runs WHEN NEW.inference_count>OLD.inference_count BEGIN SELECT RAISE(FAIL,'admission'); END;").await;
    f.coordinator.resume(run).await.unwrap();
    f.idle().await;
    let data = f.data(run).await;
    assert_eq!(data.partial_text, "old");
    assert_eq!(data.inference_count, 1);
    assert!(f.requests.try_recv().is_err());
}

#[tokio::test]
async fn sibling_calls_survive_approval_and_restart_without_replaying_first() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    request.finish(Ok(turn(vec![f.tool_call("first"), f.tool_call("second")])));
    let first = f.settled(run).await.attempts[0].binding.clone().unwrap();
    f.approve(run, first).await;
    let call = f.calls.recv().await.unwrap();
    assert_eq!(call.call.provider_call_id, "first");
    call.reply.send(Ok(outcome(false, 2))).unwrap();
    let data = f.settled(run).await;
    assert_eq!(data.attempts[0].state, AttemptState::Succeeded);
    assert_eq!(data.attempts[1].state, AttemptState::Proposed);
    f.coordinator.shutdown().await.unwrap();
    f.reopen();
    f.coordinator.resume(run).await.unwrap();
    let data = f.settled(run).await;
    assert_eq!(f.writes.load(Ordering::SeqCst), 1);
    f.approve(run, data.attempts[1].binding.clone().unwrap())
        .await;
    let call = f.calls.recv().await.unwrap();
    assert_eq!(call.call.provider_call_id, "second");
    call.reply.send(Ok(outcome(false, 2))).unwrap();
    let request = f.request().await;
    let results: Vec<_> = request
        .request
        .messages
        .iter()
        .filter_map(|m| m.tool_call_id.as_deref())
        .collect();
    assert_eq!(results, ["first", "second"]);
    request.finish(Ok(turn(vec![])));
    assert_eq!(f.settled(run).await.state, RunState::Completed);
    assert_eq!(f.writes.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn service_timeout_checkpoints_without_replay() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let binding = f.waiting(run).await;
    assert!(!f.host.active.load(Ordering::SeqCst));
    f.approve(run, binding).await;
    let dispatched = f.calls.recv().await.expect("one controlled write");
    assert_eq!(f.writes.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.coordinator
            .platform_pause(RunId::new(), f.host.generation())
            .await,
        Err(CoreError::Conflict)
    );
    assert_eq!(f.data(run).await.state, RunState::Running);
    f.coordinator
        .platform_pause(run, f.host.generation())
        .await
        .unwrap();
    drop(dispatched);
    let data = f.settled(run).await;
    assert_eq!(data.state, RunState::Paused);
    assert_eq!(data.attempts[0].state, AttemptState::OutcomeUnknown);
    assert_eq!(f.coordinator.resume(run).await, Err(CoreError::Conflict));
    f.coordinator.shutdown().await.unwrap();
    f.reopen();
    assert_eq!(
        f.data(run).await.attempts[0].state,
        AttemptState::OutcomeUnknown
    );
    assert_eq!(f.coordinator.resume(run).await, Err(CoreError::Conflict));
    assert_eq!(f.writes.load(Ordering::SeqCst), 1);
    assert!(f.calls.try_recv().is_err());
}

#[tokio::test]
async fn retired_refusal_cannot_pause_resumed_execution_of_same_run() {
    let mut f = Fixture::new();
    f.host.refuse.store(true, Ordering::SeqCst);
    let run = f.start().await;
    assert_eq!(f.settled(run).await.state, RunState::Paused);
    let retired = f.host.generation();
    f.host.refuse.store(false, Ordering::SeqCst);
    f.coordinator.resume(run).await.unwrap();
    let request = f.request().await;
    // An already-emitted refusal from execution A reaches Core only after B starts.
    let current = f.host.generation();
    assert_ne!(retired, current);
    assert_eq!(
        f.coordinator.platform_pause(run, retired).await,
        Err(CoreError::Conflict)
    );
    assert_eq!(
        f.coordinator.platform_stop(run, retired).await,
        Err(CoreError::Conflict)
    );
    assert_eq!(f.data(run).await.state, RunState::Running);
    f.coordinator.platform_stop(run, current).await.unwrap();
    drop(request);
    assert_eq!(f.settled(run).await.state, RunState::Cancelled);
}

#[tokio::test]
async fn native_action_owns_termination_until_all_old_database_writes_finish() {
    for pause in [true, false] {
        let mut f = Fixture::new();
        let run = f.start().await;
        let request = f.request().await;
        let retired = f.host.generation();
        let cancel = f
            .coordinator
            .inner
            .running
            .lock()
            .await
            .as_ref()
            .unwrap()
            .cancel
            .clone();
        let repo = f.repo.clone();
        let (entered, entered_rx) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        let blocker = tokio::task::spawn_blocking(move || {
            repo.with_connection(move |_| {
                entered.send(()).unwrap();
                released.recv().unwrap();
                Ok(())
            })
        });
        entered_rx.await.unwrap();
        {
            let action = async {
                if pause {
                    f.coordinator.platform_pause(run, retired).await
                } else {
                    f.coordinator.platform_stop(run, retired).await
                }
            };
            tokio::pin!(action);
            tokio::select! { biased; result = &mut action => panic!("action finished with DB blocked: {result:?}"), _ = cancel.cancelled() => {} }
            // Dropping the caller cannot abandon a late state-changing DB operation.
        }
        assert_eq!(f.coordinator.resume(run).await, Err(CoreError::Conflict));
        release.send(()).unwrap();
        blocker.await.unwrap().unwrap();
        drop(request);
        let settled = f.settled(run).await;
        if !pause {
            assert_eq!(settled.state, RunState::Cancelled);
            assert_eq!(f.coordinator.resume(run).await, Err(CoreError::Conflict));
            continue;
        }
        assert_eq!(settled.state, RunState::Paused);
        f.coordinator.resume(run).await.unwrap();
        let current_request = f.request().await;
        let current = f.host.generation();
        assert_ne!(retired, current);
        assert_eq!(
            f.coordinator.platform_stop(run, retired).await,
            Err(CoreError::Conflict)
        );
        assert_eq!(
            f.coordinator.platform_pause(run, retired).await,
            Err(CoreError::Conflict)
        );
        assert_eq!(f.data(run).await.state, RunState::Running);
        f.coordinator.platform_pause(run, current).await.unwrap();
        drop(current_request);
        assert_eq!(f.settled(run).await.state, RunState::Paused);
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn late_native_actions_never_succeed_without_their_terminal_checkpoint() {
    let mut failures = Vec::new();
    for pause in [false, true] {
        for dropped in [false, true] {
            for waiting in [false, true] {
                let mut f = Fixture::new();
                let run = f.start().await;
                let request = f.request().await;
                let generation = f.host.generation();
                assert_eq!(f.data(run).await.state, RunState::Running);
                let cancel = f
                    .coordinator
                    .inner
                    .running
                    .lock()
                    .await
                    .as_ref()
                    .unwrap()
                    .cancel
                    .clone();
                let (sampled, sampled_rx) = oneshot::channel();
                let (release_sample, sample_release) = std::sync::mpsc::channel();
                *f.clock.settlement_pause.lock().unwrap() = Some(SettlementPause {
                    sampled,
                    release: sample_release,
                });
                let calls = if waiting {
                    vec![f.tool_call("late-action")]
                } else {
                    vec![]
                };
                request.finish(Ok(turn(calls)));
                // The close-armed clock sample occurs after the worker has read final intent.
                sampled_rx.await.unwrap();
                let repo = f.repo.clone();
                let (entered, entered_rx) = oneshot::channel();
                let (release_db, db_release) = std::sync::mpsc::channel();
                let blocker = tokio::task::spawn_blocking(move || {
                    repo.with_connection(move |_| {
                        entered.send(()).unwrap();
                        db_release.recv().unwrap();
                        Ok(())
                    })
                });
                entered_rx.await.unwrap();
                release_sample.send(()).unwrap();
                let (result, admitted) = {
                    let action = async {
                        if pause {
                            f.coordinator.platform_pause(run, generation).await
                        } else {
                            f.coordinator.platform_stop(run, generation).await
                        }
                    };
                    tokio::pin!(action);
                    let immediate = tokio::select! { biased; result = &mut action => Some(result), _ = cancel.cancelled() => None };
                    let admitted = immediate.is_none();
                    release_db.send(()).unwrap();
                    let result = if dropped {
                        immediate
                    } else {
                        Some(match immediate {
                            Some(result) => result,
                            None => action.await,
                        })
                    };
                    (result, admitted)
                };
                blocker.await.unwrap().unwrap();
                let data = f.settled(run).await;
                let expected = if pause {
                    RunState::Paused
                } else {
                    RunState::Cancelled
                };
                let accepted = admitted || result == Some(Ok(()));
                if accepted && data.state != expected {
                    failures.push(format!("pause={pause} dropped={dropped} waiting={waiting}: {result:?} persisted {:?}, expected {expected:?}", data.state));
                } else if !accepted {
                    assert_eq!(result, Some(Err(CoreError::Conflict)));
                    assert_eq!(
                        data.state,
                        if waiting {
                            RunState::WaitingApproval
                        } else {
                            RunState::Completed
                        }
                    );
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn platform_pause_during_inference_is_not_user_cancel() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    f.coordinator
        .platform_pause(run, f.host.generation())
        .await
        .unwrap();
    drop(request);
    assert_eq!(f.settled(run).await.state, RunState::Paused);
    assert!(!f.host.active.load(Ordering::SeqCst));
    f.coordinator.resume(run).await.unwrap();
    f.request().await.finish(Ok(turn(vec![])));
    assert_eq!(f.settled(run).await.state, RunState::Completed);
}

#[tokio::test]
async fn host_refusal_pauses_before_any_network_and_releases_slot() {
    let mut f = Fixture::new();
    f.host.refuse.store(true, Ordering::SeqCst);
    let run = f.start().await;
    assert_eq!(f.settled(run).await.state, RunState::Paused);
    assert!(f.requests.try_recv().is_err());
    assert_eq!(f.live.load(Ordering::SeqCst), 0);
    assert!(!f.host.active.load(Ordering::SeqCst));
    f.host.refuse.store(false, Ordering::SeqCst);
    f.coordinator.resume(run).await.unwrap();
    f.request().await.finish(Ok(turn(vec![])));
    assert_eq!(f.settled(run).await.state, RunState::Completed);
}
#[tokio::test]
async fn completion_flushes_partial_tail_and_event_reconnect_never_starts_work() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    request
        .events
        .send(InferenceEvent::Text("tail".into()))
        .await
        .unwrap();
    let mut complete = turn(vec![]);
    complete.text = "tail".into();
    request.finish(Ok(complete));
    let data = f.settled(run).await;
    assert_eq!(data.messages.last().unwrap().content, "tail");
    assert!(data.partial_text.is_empty());
    let events = f.coordinator.subscribe(0).poll().await.unwrap();
    let partial = events
        .iter()
        .position(|(_, _, p)| p.ends_with(":partial"))
        .unwrap();
    let assistant = events
        .iter()
        .position(|(_, _, p)| p.ends_with(":assistant"))
        .unwrap();
    assert!(partial < assistant);
    let after = events[partial].0;
    let replay = f.coordinator.subscribe(after).poll().await.unwrap();
    assert_eq!(replay, events[partial + 1..]);
    assert!(f.requests.try_recv().is_err());
    assert_eq!(f.writes.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn every_provider_call_is_validated_before_any_sibling_effect() {
    for invalid in ["empty-id", "long-id", "duplicate-id", "invalid-arguments"] {
        let mut f = Fixture::new();
        let run = f.start().await;
        let request = f.request().await;
        f.rule(PermissionRule::Allow).await;
        let first = f.tool_call("first");
        let mut second = f.tool_call("second");
        match invalid {
            "empty-id" => second.provider_call_id.clear(),
            "long-id" => second.provider_call_id = "x".repeat(257),
            "duplicate-id" => second.provider_call_id = first.provider_call_id.clone(),
            _ => second.arguments_json = "[]".into(),
        };
        request.finish(Ok(turn(vec![first, second])));
        let data = f.settled(run).await;
        assert_eq!(data.state, RunState::Failed, "{invalid}");
        assert!(data.attempts.is_empty());
        assert_eq!(f.writes.load(Ordering::SeqCst), 0);
        assert!(f.calls.try_recv().is_err());
    }
}
#[tokio::test]
async fn accepted_transports_construct_without_resolving_credentials() {
    #[derive(Clone)]
    struct Credentials;
    impl CredentialProvider for Credentials {
        fn resolve(
            &self,
            _: CredentialRef,
        ) -> Pin<Box<dyn Future<Output = CoreResult<crate::net::SecretValue>> + Send + '_>>
        {
            panic!("constructor must not resolve credentials or send network requests")
        }
    }
    let f = Fixture::new();
    assert!(RuntimeServices::with_transports(
        Credentials,
        Arc::new(f.host.clone()),
        Arc::new(Files),
        f.clock.clone()
    )
    .is_ok());
}

#[tokio::test]
async fn drop_releases_claimed_connection_under_state_lock_contention() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let binding = f.waiting(run).await;
    f.approve(run, binding).await;
    let call = f.calls.recv().await.unwrap();
    assert_eq!(f.live.load(Ordering::SeqCst), 1);
    let inner = f.coordinator.inner.clone();
    let lock = inner.running.lock().await;
    let host = f.host.clone();
    let live = f.live.clone();
    let stops = host.stops.load(Ordering::SeqCst);
    drop(f.coordinator);
    host.wait_stops(stops + 1).await;
    assert!(call.cancel.is_cancelled());
    assert!(call.reply.is_closed());
    assert_eq!(live.load(Ordering::SeqCst), 0);
    assert!(!host.active.load(Ordering::SeqCst));
    assert_eq!(f.writes.load(Ordering::SeqCst), 1);
    drop(lock);
}

#[tokio::test]
async fn handoff_never_repolls_unsent_work_after_stop() {
    let f = Fixture::new();
    let cancel = Cancellation::new();
    let release = Notify::new();
    let sent = AtomicBool::new(false);
    let mut future = Box::pin(Handoff {
        future: Box::pin(async {
            release.notified().await;
            sent.store(true, Ordering::SeqCst);
        }),
        gate: f.coordinator.inner.handoff.clone(),
        cancel: cancel.clone(),
    });
    std::future::poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    f.coordinator.inner.stop(&cancel);
    release.notify_one();
    assert_eq!(future.await, Err(CoreError::Cancelled));
    assert!(!sent.load(Ordering::SeqCst));
}
#[tokio::test]
async fn failed_run_persists_noted_failure_detail() {
    use crate::failure::{FailureDetail, FailureStep, FailureTarget};
    let mut f = Fixture::new();
    let run = f.start().await;
    f.request().await.finish_noted(
        Err(CoreError::Authentication),
        FailureDetail {
            target: FailureTarget::Inference {
                name: "fake".into(),
            },
            step: FailureStep::ChatRequest,
            url: None,
            status: Some(401),
            reason: Some("unknown API key".into()),
        },
    );
    let data = f.settled(run).await;
    assert_eq!(data.state, RunState::Failed);
    let stored = state::load(&f.repo, run).unwrap().failure.unwrap();
    assert_eq!(stored.code, CoreError::Authentication);
    let detail = stored.detail.expect("noted detail");
    assert_eq!(detail.step, FailureStep::ChatRequest);
    assert_eq!(detail.status, Some(401));
    assert_eq!(detail.reason.as_deref(), Some("unknown API key"));
    let raw: Option<String> = f
        .repo
        .with_connection(move |conn| {
            conn.query_row(
                "SELECT failure FROM runs WHERE id=?1",
                [run.0.to_string()],
                |r| r.get(0),
            )
            .map_err(|_| CoreError::Storage)
        })
        .unwrap();
    let raw: serde_json::Value = serde_json::from_str(&raw.unwrap()).unwrap();
    assert_eq!(raw["detail"]["reason"], "unknown API key");
}

#[tokio::test]
async fn failed_run_without_note_stores_no_detail() {
    // Regression: an un-noted failure (for example an MCP JSON-RPC error,
    // a batch validation conflict or a local limit) was blamed on the
    // inference endpoint's request.
    for code in [CoreError::Conflict, CoreError::Protocol] {
        let mut f = Fixture::new();
        let run = f.start().await;
        f.request().await.finish(Err(code.clone()));
        let data = f.settled(run).await;
        assert_eq!(data.state, RunState::Failed);
        let stored = data.failure.unwrap();
        assert_eq!(stored.code, code);
        assert_eq!(stored.detail, None);
        let (_, raw) = raw_run(&f, run).await;
        let raw: serde_json::Value = serde_json::from_str(&raw.unwrap()).unwrap();
        assert!(raw["detail"].is_null(), "{raw}");
    }
}

#[tokio::test]
async fn credential_note_promotes_authentication_to_credential_missing() {
    use crate::failure::FailureTarget;
    let mut f = Fixture::new();
    let run = f.start().await;
    f.request().await.finish_noted(
        Err(CoreError::Authentication),
        crate::failure::credential_missing(),
    );
    let stored = f.settled(run).await.failure.unwrap();
    assert_eq!(stored.code, CoreError::CredentialMissing);
    let detail = stored.detail.expect("credential detail");
    assert_eq!(detail.target, FailureTarget::Credential);
    assert_eq!(
        detail.reason.as_deref(),
        Some(crate::failure::CREDENTIAL_MISSING_REASON)
    );
}

#[tokio::test]
async fn legacy_plain_failure_string_loads_code_without_detail() {
    let mut f = Fixture::new();
    let run = f.start().await;
    f.request().await.finish(Err(CoreError::Authentication));
    f.settled(run).await;
    f.repo
        .with_connection(move |conn| {
            conn.execute(
                "UPDATE runs SET failure='core authentication failure' WHERE id=?1",
                [run.0.to_string()],
            )
            .map_err(|_| CoreError::Storage)
        })
        .unwrap();
    let stored = state::load(&f.repo, run).unwrap().failure.unwrap();
    assert_eq!(stored.code, CoreError::Authentication);
    // No target is invented for a row that never named one.
    assert_eq!(stored.detail, None);
    f.sql("UPDATE runs SET failure='something else'").await;
    let stored = state::load(&f.repo, run).unwrap().failure.unwrap();
    assert_eq!(stored.code, CoreError::Protocol);
    assert_eq!(stored.detail, None);
    // A row stored before the detail became optional still loads, including a
    // unit Credential target.
    f.sql(r#"UPDATE runs SET failure='{"code":"CredentialMissing","detail":{"target":"Credential","step":"ResolveCredential","url":null,"status":null,"reason":"stored credential missing; re-enter it in settings"}}'"#).await;
    let stored = state::load(&f.repo, run).unwrap().failure.unwrap();
    assert_eq!(stored.code, CoreError::CredentialMissing);
    let detail = stored.detail.expect("stored detail");
    assert_eq!(detail.target, crate::failure::FailureTarget::Credential);
    assert_eq!(
        detail.reason.as_deref(),
        Some("stored credential missing; re-enter it in settings")
    );
    // A row without the detail key loads with none.
    f.sql(r#"UPDATE runs SET failure='{"code":"Limit"}'"#).await;
    let stored = state::load(&f.repo, run).unwrap().failure.unwrap();
    assert_eq!((stored.code, stored.detail), (CoreError::Limit, None));
}

#[test]
fn stored_failure_display_is_reason_or_code_text() {
    let bare = state::StoredFailure {
        code: CoreError::Conflict,
        detail: None,
    };
    assert_eq!(bare.display(), CoreError::Conflict.to_string());
    let reasonless = state::StoredFailure {
        code: CoreError::Authentication,
        detail: Some(crate::failure::FailureDetail {
            reason: None,
            ..noted_401()
        }),
    };
    assert_eq!(reasonless.display(), CoreError::Authentication.to_string());
    let noted = state::StoredFailure {
        code: CoreError::Authentication,
        detail: Some(noted_401()),
    };
    assert_eq!(noted.display(), "unknown API key");
}

fn noted_401() -> crate::failure::FailureDetail {
    crate::failure::FailureDetail {
        target: crate::failure::FailureTarget::Inference {
            name: "fake".into(),
        },
        step: crate::failure::FailureStep::ChatRequest,
        url: None,
        status: Some(401),
        reason: Some("unknown API key".into()),
    }
}

async fn raw_run(f: &Fixture, run: RunId) -> (String, Option<String>) {
    let repo = f.repo.clone();
    db(move || {
        repo.with_connection(move |conn| {
            conn.query_row(
                "SELECT state,failure FROM runs WHERE id=?1",
                [run.0.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|_| CoreError::Storage)
        })
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn failed_run_settles_when_snapshot_cannot_name_endpoint() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    f.sql("UPDATE conversation_snapshots SET payload='{'").await;
    request.finish(Err(CoreError::Authentication));
    f.idle().await;
    let (state, failure) = raw_run(&f, run).await;
    assert_eq!(state, "Failed");
    let stored: state::StoredFailure = serde_json::from_str(&failure.unwrap()).unwrap();
    assert_eq!(stored.code, CoreError::Authentication);
    assert_eq!(stored.detail, None);
}

#[tokio::test]
async fn absorbed_tool_failure_note_is_not_attached_to_later_failure() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let binding = f.waiting(run).await;
    f.approve(run, binding).await;
    let call = f.calls.recv().await.unwrap();
    *call.note.lock().unwrap() = Some(crate::failure::FailureDetail {
        target: crate::failure::FailureTarget::McpServer { name: "mcp".into() },
        step: crate::failure::FailureStep::CallTool,
        url: None,
        status: Some(401),
        reason: Some("bad token".into()),
    });
    call.reply.send(Err(McpError::Authentication)).unwrap();
    f.request().await.finish(Err(CoreError::Limit));
    let data = f.settled(run).await;
    assert_eq!(data.attempts[0].state, AttemptState::Failed);
    let stored = data.failure.unwrap();
    assert_eq!(stored.code, CoreError::Limit);
    assert_eq!(stored.detail, None);
}

#[tokio::test]
async fn credential_lookup_failure_note_does_not_promote() {
    let mut f = Fixture::new();
    let run = f.start().await;
    f.request().await.finish_noted(
        Err(CoreError::Authentication),
        crate::failure::FailureDetail {
            target: crate::failure::FailureTarget::Credential,
            step: crate::failure::FailureStep::ResolveCredential,
            url: None,
            status: None,
            reason: Some("credential lookup failed".into()),
        },
    );
    let stored = f.settled(run).await.failure.unwrap();
    assert_eq!(stored.code, CoreError::Authentication);
    assert_eq!(
        stored.detail.expect("lookup detail").target,
        crate::failure::FailureTarget::Credential
    );
}

#[tokio::test]
async fn cancelled_run_does_not_keep_noted_detail() {
    let mut f = Fixture::new();
    let run = f.start().await;
    f.request()
        .await
        .finish_noted(Err(CoreError::Cancelled), noted_401());
    let data = f.settled(run).await;
    assert_eq!(data.state, RunState::Cancelled);
    let stored = data.failure.unwrap();
    assert_eq!(stored.code, CoreError::Cancelled);
    assert_eq!(stored.detail, None);
}

#[tokio::test]
async fn host_release_failure_keeps_original_failure_detail() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    f.host.release_pending.store(true, Ordering::SeqCst);
    request.finish_noted(Err(CoreError::Authentication), noted_401());
    for _ in 0..10000 {
        if f.host.stops.load(Ordering::SeqCst) > 0 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(f.host.stops.load(Ordering::SeqCst), 1);
    f.clock.advance(Duration::from_secs(2));
    let data = f.settled(run).await;
    assert_eq!(data.state, RunState::Paused);
    let stored = data.failure.unwrap();
    assert_eq!(stored.code, CoreError::Authentication);
    let detail = stored.detail.expect("original detail");
    assert_eq!(detail.status, Some(401));
    assert_eq!(detail.reason.as_deref(), Some("unknown API key"));
}

#[tokio::test]
async fn host_release_failure_persists_pause_instead_of_clean_completion() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    f.host.release_pending.store(true, Ordering::SeqCst);
    request.finish(Ok(turn(vec![])));
    for _ in 0..10000 {
        if f.host.stops.load(Ordering::SeqCst) > 0 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(f.host.stops.load(Ordering::SeqCst), 1);
    f.clock.advance(Duration::from_secs(2));
    let data = f.settled(run).await;
    assert_eq!(data.state, RunState::Paused);
    assert_eq!(
        data.failure.map(|f| f.code),
        Some(CoreError::UnavailableTransport)
    );
    assert_eq!(f.coordinator.resume(run).await, Err(CoreError::Conflict));
}

#[tokio::test]
async fn bounded_host_release_failure_is_reported_and_closes_admission() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    f.host.release_pending.store(true, Ordering::SeqCst);
    let stop = f.coordinator.cancel(run);
    tokio::pin!(stop);
    std::future::poll_fn(|cx| {
        assert!(stop.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    for _ in 0..10000 {
        if f.host.stops.load(Ordering::SeqCst) > 0 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(f.host.stops.load(Ordering::SeqCst), 1);
    f.clock.advance(Duration::from_secs(2));
    assert_eq!(stop.await, Err(CoreError::UnavailableTransport));
    drop(request);
    assert!(!f.host.active.load(Ordering::SeqCst));
    assert_eq!(f.data(run).await.active_ms, 2000);
    assert_eq!(
        f.coordinator.start_turn(f.conversation, "no".into()).await,
        Err(CoreError::Conflict)
    );
    assert_eq!(
        f.coordinator.shutdown().await,
        Err(CoreError::UnavailableTransport)
    );
}

#[tokio::test]
async fn exhausts_twenty_inference_requests_with_denied_attempts() {
    let mut f = Fixture::new();
    let run = f.start().await;
    for index in 0..20 {
        let request = f.request().await;
        if index == 0 {
            f.rule(PermissionRule::Block).await;
        }
        request.finish(Ok(turn(vec![f.tool_call(&format!("call-{index}"))])));
    }
    let data = f.settled(run).await;
    assert_eq!(data.state, RunState::Failed);
    assert_eq!(data.inference_count, 20);
    assert_eq!(data.attempt_count, 20);
    assert!(data
        .attempts
        .iter()
        .all(|a| a.state == AttemptState::Denied));
    assert!(f.requests.try_recv().is_err());
    assert_eq!(f.writes.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn exhausts_forty_failed_tool_attempts() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    f.rule(PermissionRule::Allow).await;
    request.finish(Ok(turn(
        (0..40).map(|i| f.tool_call(&format!("call-{i}"))).collect(),
    )));
    for _ in 0..40 {
        f.calls
            .recv()
            .await
            .unwrap()
            .reply
            .send(Ok(outcome(true, 2)))
            .unwrap();
    }
    f.request()
        .await
        .finish(Ok(turn(vec![f.tool_call("forty-one")])));
    let data = f.settled(run).await;
    assert_eq!(data.state, RunState::Failed);
    assert_eq!(data.attempt_count, 40);
    assert!(data
        .attempts
        .iter()
        .all(|a| a.state == AttemptState::Failed));
    assert_eq!(f.writes.load(Ordering::SeqCst), 40);
    assert!(f.calls.try_recv().is_err());
}
#[tokio::test]
async fn restart_never_replays_applied_write_with_lost_response() {
    for boundary in ["shutdown", "timeout", "lost-response"] {
        let mut f = Fixture::new();
        let run = f.start().await;
        let binding = f.waiting(run).await;
        f.approve(run, binding).await;
        let call = f.calls.recv().await.unwrap();
        assert_eq!(f.writes.load(Ordering::SeqCst), 1);
        match boundary {
            "shutdown" => {
                f.coordinator.shutdown().await.unwrap();
                drop(call);
            }
            "timeout" => {
                f.clock.advance(Duration::from_millis(900001));
                f.idle().await;
                drop(call);
            }
            _ => {
                call.reply.send(Err(McpError::OutcomeUnknown)).unwrap();
                f.idle().await;
            }
        }
        f.coordinator.shutdown().await.unwrap();
        f.reopen();
        let data = f.data(run).await;
        assert!(matches!(
            data.state,
            RunState::Interrupted | RunState::Failed
        ));
        assert_eq!(data.attempts[0].state, AttemptState::OutcomeUnknown);
        assert_eq!(f.coordinator.resume(run).await, Err(CoreError::Conflict));
        assert_eq!(f.writes.load(Ordering::SeqCst), 1);
        assert!(f.calls.try_recv().is_err());
        assert!(f.requests.try_recv().is_err());
    }
}
#[tokio::test]
async fn restart_preserves_unconsumed_approval_but_requires_explicit_resume() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let binding = f.waiting(run).await;
    f.coordinator.approve(binding).await.unwrap();
    f.coordinator.shutdown().await.unwrap();
    f.reopen();
    assert_eq!(f.data(run).await.state, RunState::Interrupted);
    assert_eq!(f.data(run).await.attempts[0].state, AttemptState::Approved);
    assert!(f.calls.try_recv().is_err());
    assert!(f.requests.try_recv().is_err());
    f.coordinator.resume(run).await.unwrap();
    f.calls
        .recv()
        .await
        .unwrap()
        .reply
        .send(Ok(outcome(false, 2)))
        .unwrap();
    f.request().await.finish(Ok(turn(vec![])));
    assert_eq!(f.settled(run).await.state, RunState::Completed);
    assert_eq!(f.writes.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn every_pending_ask_call_gets_a_binding_without_changing_state() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    request.finish(Ok(turn(vec![f.tool_call("first"), f.tool_call("second")])));
    let data = f.settled(run).await;
    assert_eq!(data.state, RunState::WaitingApproval);
    for (attempt, id) in data.attempts.iter().zip(["first", "second"]) {
        assert_eq!(attempt.state, AttemptState::Proposed);
        let binding = attempt.binding.as_ref().expect("binding prepared");
        assert_eq!(binding.attempt_id, attempt.id);
        assert_eq!(binding.provider_call_id, id);
    }
    assert_eq!(f.writes.load(Ordering::SeqCst), 0);
    assert!(f.calls.try_recv().is_err());
}
#[tokio::test]
async fn decide_is_all_or_nothing() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    request.finish(Ok(turn(vec![f.tool_call("first"), f.tool_call("second")])));
    let data = f.settled(run).await;
    let first = data.attempts[0].binding.clone().unwrap();
    let mut stale = data.attempts[1].binding.clone().unwrap();
    stale.arguments_json = r#"{"changed":true}"#.into();
    assert_eq!(
        f.coordinator
            .decide(run, vec![first.clone(), stale], vec![])
            .await,
        Err(CoreError::Conflict)
    );
    assert_eq!(
        f.coordinator
            .decide(run, vec![first.clone()], vec![AttemptId::new()])
            .await,
        Err(CoreError::Conflict)
    );
    assert_eq!(
        f.coordinator
            .decide(run, vec![first.clone()], vec![first.attempt_id])
            .await,
        Err(CoreError::InvalidInput)
    );
    assert_eq!(
        f.coordinator.decide(run, vec![], vec![]).await,
        Err(CoreError::InvalidInput)
    );
    assert_eq!(
        f.coordinator
            .decide(RunId::new(), vec![first], vec![])
            .await,
        Err(CoreError::Conflict)
    );
    let data = f.data(run).await;
    assert!(data
        .attempts
        .iter()
        .all(|a| a.state == AttemptState::Proposed));
}
#[tokio::test]
async fn repeated_decisions_are_idempotent_and_resume_dispatches_once() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    request.finish(Ok(turn(vec![f.tool_call("first"), f.tool_call("second")])));
    let data = f.settled(run).await;
    let first = data.attempts[0].binding.clone().unwrap();
    let second = data.attempts[1].id;
    let decided = f
        .coordinator
        .decide(run, vec![first.clone()], vec![second])
        .await
        .unwrap();
    assert!(decided.changed);
    assert_eq!(decided.run_state, RunState::WaitingApproval);
    let repeated = f
        .coordinator
        .decide(run, vec![first.clone()], vec![second])
        .await
        .unwrap();
    assert!(!repeated.changed);
    f.coordinator.deny(second).await.unwrap();
    f.coordinator.approve(first).await.unwrap();
    let data = f.data(run).await;
    assert_eq!(data.attempts[0].state, AttemptState::Approved);
    assert_eq!(data.attempts[1].state, AttemptState::Denied);
    f.coordinator.resume(run).await.unwrap();
    let call = f.calls.recv().await.unwrap();
    assert_eq!(call.call.provider_call_id, "first");
    call.reply.send(Ok(outcome(false, 2))).unwrap();
    f.request().await.finish(Ok(turn(vec![])));
    assert_eq!(f.settled(run).await.state, RunState::Completed);
    assert_eq!(f.writes.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn repeated_decision_racing_dispatch_is_already_decided() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    request.finish(Ok(turn(vec![f.tool_call("first"), f.tool_call("second")])));
    let data = f.settled(run).await;
    let first = data.attempts[0].binding.clone().unwrap();
    let second = data.attempts[1].id;
    f.coordinator
        .decide(run, vec![first.clone()], vec![second])
        .await
        .unwrap();
    f.coordinator.resume(run).await.unwrap();
    // The first call is now dispatching; a double tap changes nothing.
    let call = f.calls.recv().await.unwrap();
    let repeated = f
        .coordinator
        .decide(run, vec![first.clone()], vec![second])
        .await
        .unwrap();
    assert!(!repeated.changed);
    assert_eq!(
        f.data(run).await.attempts[0].state,
        AttemptState::Dispatching
    );
    call.reply.send(Ok(outcome(false, 2))).unwrap();
    f.request().await.finish(Ok(turn(vec![])));
    assert_eq!(f.settled(run).await.state, RunState::Completed);
    assert!(
        !f.coordinator
            .decide(run, vec![first], vec![second])
            .await
            .unwrap()
            .changed
    );
    assert_eq!(f.writes.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn deny_requires_a_live_run() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    request.finish(Ok(turn(vec![f.tool_call("first")])));
    let attempt = f.settled(run).await.attempts[0].id;
    f.coordinator.cancel(run).await.unwrap();
    assert_eq!(f.data(run).await.attempts[0].state, AttemptState::Proposed);
    assert_eq!(f.coordinator.deny(attempt).await, Err(CoreError::Conflict));
    assert_eq!(
        f.coordinator.decide(run, vec![], vec![attempt]).await,
        Err(CoreError::Conflict)
    );
    assert_eq!(f.data(run).await.attempts[0].state, AttemptState::Proposed);
}
#[tokio::test]
async fn stop_and_approval_race_has_no_dispatch() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let binding = f.waiting(run).await;
    let (approval, stop) = tokio::join!(f.coordinator.approve(binding), f.coordinator.cancel(run));
    assert!(matches!(approval, Ok(()) | Err(CoreError::Conflict)));
    stop.unwrap();
    assert_eq!(f.coordinator.resume(run).await, Err(CoreError::Conflict));
    assert_eq!(f.writes.load(Ordering::SeqCst), 0);
    assert!(f.calls.try_recv().is_err());
}
#[tokio::test]
async fn cancelled_handoff_does_not_even_construct_transport_future() {
    let f = Fixture::new();
    let cancel = Cancellation::new();
    cancel.cancel();
    let invoked = AtomicBool::new(false);
    let result = dispatch(
        &f.coordinator.inner,
        &cancel,
        f.clock.now() + Duration::from_secs(1),
        async {
            invoked.store(true, Ordering::SeqCst);
        },
    )
    .await;
    assert_eq!(result, Err(CoreError::Cancelled));
    assert!(!invoked.load(Ordering::SeqCst));
}
#[tokio::test]
async fn dispatch_transactions_fail_closed_before_after_and_at_commit() {
    for phase in ["BEFORE", "AFTER", "COMMIT"] {
        let mut f = Fixture::new();
        let run = f.start().await;
        let binding = f.waiting(run).await;
        let sql = if phase == "COMMIT" {
            "CREATE TABLE commit_fault(id TEXT REFERENCES conversations(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER fault AFTER UPDATE OF state ON attempts WHEN NEW.state='Dispatching' BEGIN INSERT INTO commit_fault VALUES('missing'); END;".into()
        } else {
            format!("CREATE TRIGGER fault {phase} UPDATE OF state ON attempts WHEN NEW.state='Dispatching' BEGIN SELECT RAISE(FAIL,'dispatch'); END;")
        };
        f.sql(&sql).await;
        f.approve(run, binding).await;
        f.idle().await;
        assert_eq!(f.writes.load(Ordering::SeqCst), 0, "{phase}");
        assert!(f.calls.try_recv().is_err());
        assert!(f.requests.try_recv().is_err());
        assert_eq!(
            f.coordinator
                .start_turn(f.conversation, "blocked".into())
                .await,
            Err(CoreError::Storage)
        );
    }
}
#[tokio::test]
async fn every_result_transaction_boundary_halts_following_io() {
    let stages = [
        (
            "attempt",
            "UPDATE OF state ON attempts",
            "NEW.state='Succeeded'",
        ),
        (
            "message",
            "INSERT ON messages",
            "json_extract(NEW.payload,'$.role')='tool'",
        ),
        (
            "event",
            "INSERT ON events",
            "NEW.payload LIKE 'attempt:%:Succeeded'",
        ),
        (
            "account",
            "UPDATE ON runs",
            "EXISTS(SELECT 1 FROM attempts WHERE run_id=NEW.id AND state='Succeeded')",
        ),
    ];
    for (name, operation, condition) in stages {
        for phase in ["BEFORE", "AFTER"] {
            let mut f = Fixture::new();
            let run = f.start().await;
            let binding = f.waiting(run).await;
            f.approve(run, binding).await;
            let call = f.calls.recv().await.unwrap();
            f.sql(&format!("CREATE TRIGGER fault {phase} {operation} WHEN {condition} BEGIN SELECT RAISE(FAIL,'result'); END;")).await;
            call.reply.send(Ok(outcome(false, 2))).unwrap();
            f.idle().await;
            assert_eq!(f.writes.load(Ordering::SeqCst), 1, "{name} {phase}");
            assert!(f.requests.try_recv().is_err());
            assert!(f.calls.try_recv().is_err());
            assert_eq!(
                f.coordinator
                    .start_turn(f.conversation, "blocked".into())
                    .await,
                Err(CoreError::Storage)
            );
            assert_eq!(
                f.data(run).await.attempts[0].state,
                AttemptState::OutcomeUnknown
            );
        }
    }
    let mut f = Fixture::new();
    let run = f.start().await;
    let binding = f.waiting(run).await;
    f.approve(run, binding).await;
    let call = f.calls.recv().await.unwrap();
    f.sql("CREATE TABLE commit_fault(id TEXT REFERENCES conversations(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER fault AFTER UPDATE OF state ON attempts WHEN NEW.state='Succeeded' BEGIN INSERT INTO commit_fault VALUES('missing'); END;").await;
    call.reply.send(Ok(outcome(false, 2))).unwrap();
    f.idle().await;
    assert!(f.requests.try_recv().is_err());
    assert_eq!(f.writes.load(Ordering::SeqCst), 1);
    assert_eq!(f.coordinator.resume(run).await, Err(CoreError::Storage));
}
#[tokio::test]
async fn injected_clock_drives_deadline_and_resume_keeps_spent_budget() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    request.output("a").await;
    f.clock.advance(Duration::from_secs(400));
    request.finish(Err(CoreError::UnavailableTransport));
    let data = f.settled(run).await;
    assert_eq!(data.state, RunState::Paused);
    assert_eq!(data.active_ms, 400_000);
    f.coordinator.resume(run).await.unwrap();
    let request = f.request().await;
    request.output("b").await;
    f.clock.advance(Duration::from_secs(501));
    let data = f.settled(run).await;
    drop(request);
    assert_eq!(data.state, RunState::Failed);
    assert_eq!(data.active_ms, 901_000);
    assert_eq!(data.inference_count, 2);
    assert!(f.requests.try_recv().is_err());
}
#[tokio::test]
async fn partial_byte_time_and_cancel_boundaries_flush_without_per_fragment_writes() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    for _ in 0..4096 {
        request
            .events
            .send(InferenceEvent::Text("x".into()))
            .await
            .unwrap();
    }
    for _ in 0..10000 {
        if f.data(run).await.partial_text.len() == 4096 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(f.data(run).await.partial_text.len(), 4096);
    request
        .events
        .send(InferenceEvent::Text("time".into()))
        .await
        .unwrap();
    while request.events.capacity() != crate::inference::EVENT_CHANNEL_CAPACITY {
        tokio::task::yield_now().await;
    }
    f.clock.advance(Duration::from_millis(250));
    for _ in 0..10000 {
        if f.data(run).await.partial_text.ends_with("time") {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(f.data(run).await.partial_text.ends_with("time"));
    request
        .events
        .send(InferenceEvent::Text("cancel".into()))
        .await
        .unwrap();
    while request.events.capacity() != crate::inference::EVENT_CHANNEL_CAPACITY {
        tokio::task::yield_now().await;
    }
    f.coordinator.cancel(run).await.unwrap();
    drop(request);
    assert!(f.data(run).await.partial_text.ends_with("timecancel"));
    let events = f.coordinator.subscribe(0).poll().await.unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|(_, _, p)| p.ends_with(":partial"))
            .count(),
        3
    );
}

use crate::inference::ChatRole;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::{mpsc, oneshot, Notify};

type Reply = (
    CoreResult<InferenceTurn>,
    Option<crate::failure::FailureDetail>,
);
struct Request {
    request: InferenceRequest,
    events: mpsc::Sender<InferenceEvent>,
    reply: oneshot::Sender<Reply>,
}
impl Request {
    /// Sends `text` and waits until the run has taken it, so that time
    /// advanced afterwards is generation time rather than an uncharged wait
    /// for the first output.
    async fn output(&self, text: &str) {
        self.events
            .send(InferenceEvent::Text(text.into()))
            .await
            .unwrap();
        while self.events.capacity() != crate::inference::EVENT_CHANNEL_CAPACITY {
            tokio::task::yield_now().await;
        }
    }
    fn finish(self, result: CoreResult<InferenceTurn>) {
        drop(self.events);
        self.reply.send((result, None)).unwrap();
    }
    /// Finishes after the fake transport notes `detail` in the run's task.
    fn finish_noted(
        self,
        result: CoreResult<InferenceTurn>,
        detail: crate::failure::FailureDetail,
    ) {
        drop(self.events);
        self.reply.send((result, Some(detail))).unwrap();
    }
}
struct FakeInference(mpsc::UnboundedSender<Request>);
impl InferenceTransport for FakeInference {
    fn stream(
        &self,
        request: InferenceRequest,
        events: mpsc::Sender<InferenceEvent>,
        _: Cancellation,
    ) -> InferenceFuture {
        let (reply, rx) = oneshot::channel();
        self.0
            .send(Request {
                request,
                events,
                reply,
            })
            .unwrap();
        Box::pin(async move {
            let (result, detail) = rx.await.map_err(|_| CoreError::UnavailableTransport)?;
            if let Some(detail) = detail {
                crate::failure::note(detail);
            }
            result
        })
    }
}
struct CallRequest {
    call: ToolCall,
    cancel: Cancellation,
    reply: oneshot::Sender<Result<ToolOutcome, McpError>>,
    /// Noted by the fake call future, in the run's task, after `reply` arrives.
    note: Arc<std::sync::Mutex<Option<crate::failure::FailureDetail>>>,
}
struct FakeMcp {
    clock: Arc<FakeClock>,
    calls: mpsc::UnboundedSender<CallRequest>,
    writes: Arc<AtomicUsize>,
    live: Arc<AtomicUsize>,
    handoff: Arc<Notify>,
    hold: Arc<AtomicBool>,
}
impl McpTransport for FakeMcp {
    fn connect(
        &self,
        profile: ProfileId,
        server: ServerConfig,
        repo: Repository,
        _: Instant,
        _: Cancellation,
    ) -> McpConnectFuture {
        let clock = self.clock.clone();
        let calls = self.calls.clone();
        let writes = self.writes.clone();
        let live = self.live.clone();
        let handoff = self.handoff.clone();
        let hold = self.hold.clone();
        Box::pin(async move {
            let definition = db(move || {
                ToolDefinition::from_discovery(
                    &repo,
                    ToolKey {
                        profile_id: profile,
                        server: ToolServer::Mcp(server.id),
                        name: "write".into(),
                    },
                    r#"{"type":"object"}"#,
                )
            })
            .await
            .map_err(McpError::from)?;
            live.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(Connection {
                clock,
                definition,
                calls,
                writes,
                live,
                handoff,
                hold,
            }) as Box<dyn McpConnection>)
        })
    }
}
struct Connection {
    clock: Arc<FakeClock>,
    definition: ToolDefinition,
    calls: mpsc::UnboundedSender<CallRequest>,
    writes: Arc<AtomicUsize>,
    live: Arc<AtomicUsize>,
    handoff: Arc<Notify>,
    hold: Arc<AtomicBool>,
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.live.fetch_sub(1, Ordering::SeqCst);
    }
}
impl McpConnection for Connection {
    fn list_tools(
        &mut self,
        _: Instant,
        _: Cancellation,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ToolDefinition>, McpError>> + Send + '_>> {
        let d = self.definition.clone();
        Box::pin(async move { Ok(vec![d]) })
    }
    fn call_tool(
        &mut self,
        call: ToolCall,
        _: Instant,
        cancel: Cancellation,
    ) -> Pin<Box<dyn Future<Output = Result<ToolOutcome, McpError>> + Send + '_>> {
        let (reply, rx) = oneshot::channel();
        let note = Arc::new(std::sync::Mutex::new(None));
        self.calls
            .send(CallRequest {
                call,
                cancel: cancel.clone(),
                reply,
                note: note.clone(),
            })
            .unwrap();
        Box::pin(async move {
            if self.hold.load(Ordering::SeqCst) {
                self.handoff.notified().await;
            }
            // Deliberately ignores cancel: coordinator must fence polling at its handoff.
            self.writes.fetch_add(1, Ordering::SeqCst);
            let result = rx.await.map_err(|_| McpError::OutcomeUnknown)?;
            if let Some(detail) = note.lock().unwrap().take() {
                crate::failure::note(detail);
            }
            result
        })
    }
    fn close(
        &mut self,
        _: Instant,
        _: Cancellation,
    ) -> Pin<Box<dyn Future<Output = Result<(), McpError>> + Send + '_>> {
        Box::pin(async move {
            if self.clock.settlement_pause.lock().unwrap().is_some() {
                self.clock.after_close.store(true, Ordering::SeqCst);
            }
            Ok(())
        })
    }
}
struct FakeHost {
    requests: std::sync::Mutex<Vec<crate::model::RunHostRequest>>,
    active: AtomicBool,
    starts: AtomicUsize,
    stops: AtomicUsize,
    pending: AtomicBool,
    refuse: AtomicBool,
    release_pending: AtomicBool,
    changed: Notify,
}
impl FakeHost {
    fn generation(&self) -> uuid::Uuid {
        let requests = self.requests.lock().unwrap();
        uuid::Uuid::parse_str(&requests.iter().rev().find(|r| r.active).unwrap().generation)
            .unwrap()
    }
    async fn wait_stops(&self, count: usize) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.stops.load(Ordering::SeqCst) >= count {
                return;
            }
            changed.await;
        }
    }
    async fn wait_started(&self) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.starts.load(Ordering::SeqCst) > 0 {
                return;
            }
            changed.await;
        }
    }
}
impl PlatformRunHost for Arc<FakeHost> {
    fn set_active(&self, request: crate::model::RunHostRequest) -> HostFuture {
        let active = request.active;
        let host = self.clone();
        Box::pin(async move {
            host.requests.lock().unwrap().push(request);
            if active {
                host.starts.fetch_add(1, Ordering::SeqCst);
                host.changed.notify_waiters();
                if host.refuse.load(Ordering::SeqCst) {
                    return Err(CoreError::UnavailableTransport);
                }
                if host.pending.load(Ordering::SeqCst) {
                    std::future::pending::<()>().await;
                }
            } else {
                host.stops.fetch_add(1, Ordering::SeqCst);
            }
            host.active.store(active, Ordering::SeqCst);
            host.changed.notify_waiters();
            if !active && host.release_pending.load(Ordering::SeqCst) {
                std::future::pending::<()>().await;
            }
            Ok(())
        })
    }
}
struct Files;
impl LocalFileTools for Files {
    fn invoke(&self, _: ConversationId, _: ToolCall) -> ToolFuture {
        Box::pin(async { Err(CoreError::ToolUnsupported) })
    }
}
struct SettlementPause {
    sampled: oneshot::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}
struct FakeClock {
    settlement_pause: std::sync::Mutex<Option<SettlementPause>>,
    after_close: AtomicBool,
    base: Instant,
    elapsed: std::sync::atomic::AtomicU64,
    changed: Notify,
}
impl FakeClock {
    fn advance(&self, duration: Duration) {
        self.elapsed
            .fetch_add(duration.as_millis() as u64, Ordering::SeqCst);
        self.changed.notify_waiters();
    }
}
impl RunClock for FakeClock {
    fn now(&self) -> Instant {
        if self.after_close.swap(false, Ordering::SeqCst) {
            let pause = self.settlement_pause.lock().unwrap().take();
            if let Some(pause) = pause {
                pause.sampled.send(()).unwrap();
                pause.release.recv().unwrap();
            }
        }
        self.base + Duration::from_millis(self.elapsed.load(Ordering::SeqCst))
    }
    fn sleep_until(&self, deadline: Instant) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            loop {
                let changed = self.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if self.now() >= deadline {
                    return;
                }
                changed.await;
            }
        })
    }
}
#[tokio::test]
async fn generated_artifacts_use_real_coordinator_ask_then_exact_approved_creation() {
    let mut f = Fixture::with_artifacts(true);
    let run = f.start().await;
    let request = f.request().await;
    let mut call = f.tool_call("artifact-1");
    call.key.server = ToolServer::LocalFiles;
    call.key.name = "create_artifact".into();
    call.arguments_json = r##"{"name":"answer.md","content":"# generated"}"##.into();
    request.finish(Ok(turn(vec![call])));
    let waiting = f.settled(run).await;
    assert_eq!(waiting.state, RunState::WaitingApproval);
    let store = crate::files::artifacts::ArtifactStore::open(
        f.repo.clone(),
        &f._dir.path().join("artifacts"),
    )
    .unwrap();
    assert!(store.list(f.conversation).unwrap().is_empty());
    let binding = waiting.attempts[0].binding.clone().unwrap();
    f.approve(run, binding).await;
    f.request().await.finish(Ok(turn(vec![])));
    assert_eq!(
        f.settled(run).await.attempts[0].state,
        AttemptState::Succeeded
    );
    let artifacts = store.list(f.conversation).unwrap();
    assert_eq!(artifacts.len(), 1);
    assert_eq!(
        store.read(f.conversation, artifacts[0].id).unwrap().1,
        "# generated"
    );
}

#[tokio::test]
async fn chat_only_snapshot_skips_mcp_and_local_tools() {
    let mut f = Fixture::with_artifacts(true);
    let mut profile = f
        .repo
        .with_connection(move |conn| {
            let json: String = conn
                .query_row(
                    "SELECT payload FROM profiles WHERE id=?1",
                    [f.profile.0.to_string()],
                    |r| r.get(0),
                )
                .unwrap();
            Ok(serde_json::from_str::<Profile>(&json).unwrap())
        })
        .unwrap();
    profile.mode = ProfileMode::ChatOnly;
    f.repo.save_profile(profile.clone()).unwrap();
    // Saving does not change the existing immutable conversation snapshot.
    assert_eq!(
        f.repo.conversation(f.conversation).unwrap().profile.mode,
        ProfileMode::Agent
    );
    f.repo.apply_profile(f.conversation, f.profile).unwrap();
    let run = f.start().await;
    let request = f.request().await;
    assert!(request.request.tools.is_empty());
    assert_eq!(f.live.load(Ordering::SeqCst), 0);
    request.finish(Ok(turn(vec![])));
    assert_eq!(f.settled(run).await.state, RunState::Completed);
    f.coordinator.shutdown().await.unwrap();
    f.reopen();
    assert_eq!(
        f.repo.conversation(f.conversation).unwrap().profile.mode,
        ProfileMode::ChatOnly
    );
}

#[test]
fn old_profile_and_snapshot_default_to_agent() {
    let f = Fixture::new();
    let profile = f
        .repo
        .with_connection(move |conn| {
            let json: String = conn
                .query_row(
                    "SELECT payload FROM profiles WHERE id=?1",
                    [f.profile.0.to_string()],
                    |r| r.get(0),
                )
                .unwrap();
            Ok(serde_json::from_str::<Profile>(&json).unwrap())
        })
        .unwrap();
    let mut value = serde_json::to_value(profile).unwrap();
    value.as_object_mut().unwrap().remove("mode");
    assert_eq!(
        serde_json::from_value::<Profile>(value).unwrap().mode,
        ProfileMode::Agent
    );
    let mut value =
        serde_json::to_value(f.repo.conversation(f.conversation).unwrap().profile).unwrap();
    value.as_object_mut().unwrap().remove("mode");
    assert_eq!(
        serde_json::from_value::<ProfileSnapshot>(value)
            .unwrap()
            .mode,
        ProfileMode::Agent
    );
}

struct Fixture {
    _dir: tempfile::TempDir,
    repo: Repository,
    conversation: ConversationId,
    profile: ProfileId,
    requests: mpsc::UnboundedReceiver<Request>,
    calls: mpsc::UnboundedReceiver<CallRequest>,
    writes: Arc<AtomicUsize>,
    live: Arc<AtomicUsize>,
    host: Arc<FakeHost>,
    hold: Arc<AtomicBool>,
    handoff: Arc<Notify>,
    coordinator: Coordinator,
    clock: Arc<FakeClock>,
}
impl Fixture {
    fn new() -> Self {
        Self::with_artifacts(false)
    }
    fn with_artifacts(real: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::open(&dir.path().join("core.db")).unwrap();
        let endpoint = EndpointId::new();
        repo.save_endpoint(EndpointConfig {
            id: endpoint,
            name: "model".into(),
            base_url: "https://model.test".into(),
            model: "model".into(),
            context_tokens: 1_000_000,
            max_output_tokens: 10,
            temperature: 0.0,
            allow_http: false,
            credential: None,
        })
        .unwrap();
        let server = ServerId::new();
        repo.save_server(ServerConfig {
            id: server,
            name: "mcp".into(),
            url: "https://mcp.test".into(),
            allow_http: false,
            secret_headers: vec![],
        })
        .unwrap();
        let profile = ProfileId::new();
        repo.save_profile(Profile {
            mode: crate::model::core::ProfileMode::Agent,
            id: profile,
            name: "profile".into(),
            endpoint_id: endpoint,
            server_ids: vec![server],
            instructions: String::new(),
            limits: RunLimits::default(),
        })
        .unwrap();
        let conversation = repo.create_conversation(profile).unwrap();
        let (tx, requests) = mpsc::unbounded_channel();
        let (calls_tx, calls) = mpsc::unbounded_channel();
        let writes = Arc::new(AtomicUsize::new(0));
        let live = Arc::new(AtomicUsize::new(0));
        let hold = Arc::new(AtomicBool::new(false));
        let handoff = Arc::new(Notify::new());
        let host = Arc::new(FakeHost {
            requests: std::sync::Mutex::new(Vec::new()),
            active: AtomicBool::new(false),
            starts: AtomicUsize::new(0),
            stops: AtomicUsize::new(0),
            pending: AtomicBool::new(false),
            refuse: AtomicBool::new(false),
            release_pending: AtomicBool::new(false),
            changed: Notify::new(),
        });
        let clock = Arc::new(FakeClock {
            settlement_pause: std::sync::Mutex::new(None),
            after_close: AtomicBool::new(false),
            base: Instant::now(),
            elapsed: std::sync::atomic::AtomicU64::new(0),
            changed: Notify::new(),
        });
        let services = RuntimeServices {
            inference: Arc::new(FakeInference(tx)),
            mcp: Arc::new(FakeMcp {
                clock: clock.clone(),
                calls: calls_tx,
                writes: writes.clone(),
                live: live.clone(),
                hold: hold.clone(),
                handoff: handoff.clone(),
            }),
            host: Arc::new(host.clone()),
            files: if real {
                Arc::new(
                    crate::files::artifacts::ArtifactStore::open(
                        repo.clone(),
                        &dir.path().join("artifacts"),
                    )
                    .unwrap(),
                )
            } else {
                Arc::new(Files)
            },
            clock: clock.clone(),
        };
        let coordinator = Coordinator::open(
            RuntimeConfig {
                repository: repo.clone(),
            },
            services,
        )
        .unwrap();
        Self {
            _dir: dir,
            repo,
            conversation,
            profile,
            requests,
            calls,
            writes,
            live,
            host,
            hold,
            handoff,
            coordinator,
            clock,
        }
    }
    async fn start(&self) -> RunId {
        self.coordinator
            .start_turn(self.conversation, "do it".into())
            .await
            .unwrap()
    }
    async fn request(&mut self) -> Request {
        self.requests.recv().await.expect("inference request")
    }
    fn tool_call(&self, id: &str) -> ToolCall {
        ToolCall {
            key: ToolKey {
                profile_id: self.profile,
                server: ToolServer::Mcp(
                    self.repo
                        .conversation(self.conversation)
                        .unwrap()
                        .profile
                        .servers[0]
                        .id,
                ),
                name: "write".into(),
            },
            provider_call_id: id.into(),
            arguments_json: "{}".into(),
        }
    }
    async fn data(&self, run: RunId) -> state::RunData {
        self.coordinator
            .snapshot()
            .await
            .unwrap()
            .runs
            .into_iter()
            .find(|r| r.id == run)
            .unwrap()
    }
    async fn settled(&mut self, run: RunId) -> state::RunData {
        self.idle().await;
        let data = self.data(run).await;
        assert_ne!(data.state, RunState::Running, "run did not settle");
        data
    }
    async fn waiting(&mut self, run: RunId) -> ApprovalBinding {
        let request = self.request().await;
        request.finish(Ok(turn(vec![self.tool_call("id-1")])));
        self.settled(run).await.attempts[0].binding.clone().unwrap()
    }
    async fn approve(&self, run: RunId, binding: ApprovalBinding) {
        self.coordinator.approve(binding).await.unwrap();
        self.coordinator.resume(run).await.unwrap();
    }
    /// Waits for the worker to finish; an inference request arriving meanwhile
    /// fails the test instead of leaving the run (and the test) waiting forever.
    async fn idle(&mut self) {
        let done = {
            let slot = self.coordinator.inner.running.lock().await;
            slot.as_ref().map(|running| running.done.clone())
        };
        if let Some(done) = done {
            tokio::select! {
                biased;
                () = wait_done(done) => {}
                Some(_) = self.requests.recv() => {
                    panic!("unexpected inference request while waiting for idle")
                }
            }
        }
        assert!(
            self.coordinator.inner.running.lock().await.is_none(),
            "worker did not release slot"
        );
    }
    async fn rule(&self, rule: PermissionRule) {
        let repo = self.repo.clone();
        let key = self.tool_call("rule").key;
        db(move || {
            let definition = ToolDefinition::from_discovery(&repo, key, r#"{"type":"object"}"#)?;
            permissions::set_rule(&repo, &definition, rule)
        })
        .await
        .unwrap();
    }
    fn reopen(&mut self) {
        let s = &self.coordinator.inner.services;
        let services = RuntimeServices {
            inference: s.inference.clone(),
            mcp: s.mcp.clone(),
            host: s.host.clone(),
            files: s.files.clone(),
            clock: s.clock.clone(),
        };
        self.coordinator = Coordinator::open(
            RuntimeConfig {
                repository: self.repo.clone(),
            },
            services,
        )
        .unwrap();
    }
    /// Edits the endpoint's context window in settings; the next turn, summary
    /// or resume refreshes the conversation snapshot from it.
    /// Replaces the profile's run limits; the next turn runs with them.
    fn set_limits(&self, limits: RunLimits) {
        let snapshot = self.repo.conversation(self.conversation).unwrap().profile;
        self.repo
            .save_profile(Profile {
                mode: snapshot.mode,
                id: snapshot.profile_id,
                name: snapshot.name,
                endpoint_id: snapshot.endpoint.id,
                server_ids: snapshot.servers.iter().map(|s| s.id).collect(),
                instructions: snapshot.instructions,
                limits,
            })
            .unwrap();
    }
    fn set_context_tokens(&self, tokens: u32) {
        let mut endpoint = self
            .repo
            .conversation(self.conversation)
            .unwrap()
            .profile
            .endpoint;
        endpoint.context_tokens = tokens;
        self.repo.save_endpoint(endpoint).unwrap();
    }
    async fn sql(&self, sql: &str) {
        let sql = sql.to_owned();
        let repo = self.repo.clone();
        db(move || {
            repo.with_connection(move |c| c.execute_batch(&sql).map_err(|_| CoreError::Storage))
        })
        .await
        .unwrap();
    }
}
fn turn(calls: Vec<ToolCall>) -> InferenceTurn {
    InferenceTurn {
        text: "answer".into(),
        calls,
        usage: None,
        finish_reason: "stop".into(),
    }
}
fn outcome(is_error: bool, size: usize) -> ToolOutcome {
    ToolOutcome {
        is_error,
        text: vec!["x".repeat(size)],
        structured_content: None,
        unsupported_content: vec![],
    }
}

#[tokio::test]
async fn one_active_run() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    assert_eq!(
        f.coordinator
            .start_turn(f.conversation, "second".into())
            .await,
        Err(CoreError::Conflict)
    );
    f.coordinator.cancel(run).await.unwrap();
    drop(request);
}
#[tokio::test]
async fn waiting_releases_slot_and_resume_reacquires() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let binding = f.waiting(run).await;
    let other = f.repo.create_conversation(f.profile).unwrap();
    let second = f
        .coordinator
        .start_turn(other, "other".into())
        .await
        .unwrap();
    let request = f.request().await;
    f.coordinator.approve(binding).await.unwrap();
    assert_eq!(f.coordinator.resume(run).await, Err(CoreError::Conflict));
    request.finish(Ok(turn(vec![])));
    f.settled(second).await;
    f.coordinator.resume(run).await.unwrap();
    let call = f.calls.recv().await.unwrap();
    call.reply.send(Ok(outcome(false, 2))).unwrap();
    let request = f.request().await;
    assert!(request
        .request
        .messages
        .iter()
        .any(|m| matches!(m.role, ChatRole::Tool) && m.tool_call_id.as_deref() == Some("id-1")));
    request.finish(Ok(turn(vec![])));
    assert_eq!(f.settled(run).await.state, RunState::Completed);
    assert_eq!(f.writes.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn oversized_complete_results_preserve_both_is_error_values() {
    for is_error in [false, true] {
        let mut f = Fixture::new();
        let run = f.start().await;
        let binding = f.waiting(run).await;
        f.approve(run, binding).await;
        f.calls
            .recv()
            .await
            .unwrap()
            .reply
            .send(Ok(outcome(is_error, 1024 * 1024 + 1)))
            .unwrap();
        f.request().await.finish(Ok(turn(vec![])));
        let data = f.settled(run).await;
        assert_eq!(
            data.attempts[0].state,
            if is_error {
                AttemptState::Failed
            } else {
                AttemptState::Succeeded
            }
        );
        assert!(data
            .messages
            .iter()
            .any(|m| m.content.contains("too large")));
        assert_eq!(f.writes.load(Ordering::SeqCst), 1);
    }
}
#[tokio::test]
async fn duplicate_provider_ids_rejected_before_any_effect() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    request.finish(Ok(turn(vec![f.tool_call("same"), f.tool_call("same")])));
    let data = f.settled(run).await;
    assert_eq!(data.state, RunState::Failed);
    assert!(data.attempts.is_empty());
    assert_eq!(f.writes.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn persistent_result_fault_poison_blocks_new_turn_even_if_recovery_fails() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let binding = f.waiting(run).await;
    f.approve(run, binding).await;
    let call = f.calls.recv().await.unwrap();
    f.sql("CREATE TRIGGER fail_attempt BEFORE UPDATE ON attempts BEGIN SELECT RAISE(FAIL,'persistent'); END; CREATE TRIGGER fail_run BEFORE UPDATE ON runs BEGIN SELECT RAISE(FAIL,'persistent'); END;").await;
    let stops = f.host.stops.load(Ordering::SeqCst);
    call.reply.send(Ok(outcome(false, 2))).unwrap();
    f.host.wait_stops(stops + 1).await;
    assert_eq!(
        f.coordinator
            .start_turn(f.conversation, "must not send".into())
            .await,
        Err(CoreError::Storage)
    );
    assert_eq!(f.writes.load(Ordering::SeqCst), 1);
    assert!(f.requests.try_recv().is_err());
    assert_eq!(
        f.data(run).await.attempts[0].state,
        AttemptState::Dispatching
    );
    f.sql("DROP TRIGGER fail_attempt; DROP TRIGGER fail_run;")
        .await;
    assert_eq!(f.coordinator.shutdown().await, Err(CoreError::Storage));
}
#[tokio::test]
async fn stop_after_claim_fences_unsent_transport() {
    let mut f = Fixture::new();
    f.hold.store(true, Ordering::SeqCst);
    let run = f.start().await;
    let binding = f.waiting(run).await;
    f.approve(run, binding).await;
    let call = f.calls.recv().await.unwrap();
    assert_eq!(call.call.provider_call_id, "id-1");
    assert!(!call.cancel.is_cancelled());
    assert_eq!(
        f.data(run).await.attempts[0].state,
        AttemptState::Dispatching
    );
    f.coordinator.cancel(run).await.unwrap();
    f.handoff.notify_one();
    drop(call);
    assert_eq!(f.writes.load(Ordering::SeqCst), 0);
    let data = f.data(run).await;
    assert_eq!(data.state, RunState::Cancelled);
    assert_eq!(data.attempts[0].state, AttemptState::OutcomeUnknown);
}
#[tokio::test]
async fn pending_host_startup_times_out_pauses_and_releases_slot() {
    let mut f = Fixture::new();
    f.host.pending.store(true, Ordering::SeqCst);
    let run = f.start().await;
    f.host.wait_started().await;
    f.clock.advance(Duration::from_secs(31));
    for _ in 0..1000 {
        tokio::task::yield_now().await;
    }
    assert_eq!(f.data(run).await.state, RunState::Paused);
    f.settled(run).await;
    assert!(f.requests.try_recv().is_err());
    f.host.pending.store(false, Ordering::SeqCst);
    let second = f.start().await;
    f.request().await.finish(Ok(turn(vec![])));
    f.settled(second).await;
}
#[tokio::test]
async fn drop_under_state_lock_contention_cancels_and_releases_host() {
    let mut f = Fixture::new();
    let _run = f.start().await;
    let request = f.request().await;
    assert!(f.host.active.load(Ordering::SeqCst));
    let inner = f.coordinator.inner.clone();
    let lock = inner.running.lock().await;
    let host = f.host.clone();
    drop(f.coordinator);
    for _ in 0..10000 {
        if !host.active.load(Ordering::SeqCst) {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(!host.active.load(Ordering::SeqCst));
    assert!(request.reply.is_closed());
    drop(lock);
}
#[tokio::test]
async fn failed_request_and_cancel_charge_active_time_but_wait_does_not() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    request.output("partial").await;
    f.clock.advance(Duration::from_secs(12));
    request.finish(Err(CoreError::UnavailableTransport));
    let data = f.settled(run).await;
    assert!(data.active_ms >= 12_000, "{}", data.active_ms);
    let run = f.start().await;
    let binding = f.waiting(run).await;
    let before = f.data(run).await.active_ms;
    f.clock.advance(Duration::from_secs(3600));
    f.approve(run, binding).await;
    let call = f.calls.recv().await.unwrap();
    f.clock.advance(Duration::from_secs(7));
    f.coordinator.cancel(run).await.unwrap();
    drop(call);
    let after = f.data(run).await.active_ms;
    assert!(
        (before + 7000..before + 8000).contains(&after),
        "before {before}, after {after}"
    );
}
#[tokio::test]
async fn partial_fragments_are_batched_and_error_tail_flushed() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    for _ in 0..100 {
        request
            .events
            .send(InferenceEvent::Text("x".into()))
            .await
            .unwrap();
    }
    request.finish(Err(CoreError::UnavailableTransport));
    let data = f.settled(run).await;
    assert_eq!(data.partial_text, "x".repeat(100));
    let events = f.coordinator.subscribe(0).poll().await.unwrap();
    let writes = events
        .iter()
        .filter(|(_, _, s)| s.ends_with(":partial"))
        .count();
    assert!(writes <= 2, "{writes} partial SQL writes for 100 bytes");
}

fn current_conversation_revision(f: &Fixture) -> i64 {
    let conversation = f.conversation;
    f.repo
        .with_connection(move |c| {
            Ok(c.query_row(
                "SELECT current_revision FROM conversations WHERE id=?1",
                [conversation.0.to_string()],
                |r| r.get::<_, i64>(0),
            )
            .unwrap())
        })
        .unwrap()
}

#[tokio::test]
async fn resume_rebinds_paused_run_to_refreshed_settings() {
    let mut f = Fixture::new();
    let run = f.start().await;
    f.request()
        .await
        .finish(Err(CoreError::UnavailableTransport));
    assert_eq!(f.settled(run).await.state, RunState::Paused);
    let mut endpoint = f
        .repo
        .conversation(f.conversation)
        .unwrap()
        .profile
        .endpoint;
    endpoint.base_url = "https://example.com/v1".into();
    f.repo.save_endpoint(endpoint).unwrap();
    f.coordinator.resume(run).await.unwrap();
    let request = f.request().await;
    assert_eq!(request.request.endpoint.base_url, "https://example.com/v1");
    request.finish(Ok(turn(vec![])));
    assert_eq!(f.settled(run).await.state, RunState::Completed);
    assert_eq!(current_conversation_revision(&f), 2);
    assert_eq!(f.repo.run_profile_revision(run).unwrap(), 2);
}

#[tokio::test]
async fn new_turn_uses_edited_settings_and_unchanged_settings_add_no_revision() {
    let mut f = Fixture::new();
    let run = f.start().await;
    f.request().await.finish(Ok(turn(vec![])));
    f.settled(run).await;
    assert_eq!(current_conversation_revision(&f), 1);
    let mut endpoint = f
        .repo
        .conversation(f.conversation)
        .unwrap()
        .profile
        .endpoint;
    endpoint.base_url = "https://example.com/v1".into();
    f.repo.save_endpoint(endpoint).unwrap();
    let run = f.start().await;
    let request = f.request().await;
    assert_eq!(request.request.endpoint.base_url, "https://example.com/v1");
    request.finish(Ok(turn(vec![])));
    f.settled(run).await;
    assert_eq!(f.repo.run_profile_revision(run).unwrap(), 2);
    let run = f.start().await;
    f.request().await.finish(Ok(turn(vec![])));
    f.settled(run).await;
    assert_eq!(current_conversation_revision(&f), 2);
    assert_eq!(f.repo.run_profile_revision(run).unwrap(), 2);
}

fn delete_profile_sql(f: &Fixture) -> String {
    format!(
        "PRAGMA foreign_keys=OFF; DELETE FROM profile_servers WHERE profile_id='{0}'; DELETE FROM profiles WHERE id='{0}'; PRAGMA foreign_keys=ON;",
        f.profile.0
    )
}

fn assert_refresh_detail(detail: Option<crate::failure::FailureDetail>) {
    let detail = detail.expect("refresh failure detail reaches the caller");
    assert_eq!(
        detail.target,
        crate::failure::FailureTarget::Profile {
            name: "profile".into()
        }
    );
    assert_eq!(detail.step, crate::failure::FailureStep::RefreshProfile);
    assert_eq!(
        detail.reason.as_deref(),
        Some("profile, endpoint or server no longer exists")
    );
}

fn edit_endpoint_url(f: &Fixture) {
    let mut endpoint = f
        .repo
        .conversation(f.conversation)
        .unwrap()
        .profile
        .endpoint;
    endpoint.base_url = "https://example.com/v1".into();
    f.repo.save_endpoint(endpoint).unwrap();
}

#[tokio::test]
async fn turn_fails_with_configuration_when_profile_was_deleted() {
    let f = Fixture::new();
    f.sql(&delete_profile_sql(&f)).await;
    let (result, detail) =
        crate::failure::scope(f.coordinator.start_turn(f.conversation, "do it".into())).await;
    assert_eq!(result, Err(CoreError::Configuration));
    assert_refresh_detail(detail);
    assert!(f.coordinator.snapshot().await.unwrap().runs.is_empty());
    assert_eq!(current_conversation_revision(&f), 1);
}

#[tokio::test]
async fn summary_fails_with_configuration_detail_when_profile_was_deleted() {
    let mut f = Fixture::new();
    let run = f.start().await;
    f.request().await.finish(Ok(turn(vec![])));
    f.settled(run).await;
    let message = f.repo.conversation(f.conversation).unwrap().messages[0].id;
    f.sql(&delete_profile_sql(&f)).await;
    let (result, detail) = crate::failure::scope(f.coordinator.summarize_context(
        f.conversation,
        ContextSelection {
            message_ids: vec![message],
            summary_ids: vec![],
        },
    ))
    .await;
    assert_eq!(result, Err(CoreError::Configuration));
    assert_refresh_detail(detail);
    assert_eq!(f.coordinator.snapshot().await.unwrap().runs.len(), 1);
    assert_eq!(current_conversation_revision(&f), 1);
}

#[tokio::test]
async fn resume_fails_with_configuration_detail_when_profile_was_deleted() {
    let mut f = Fixture::new();
    let run = f.start().await;
    f.request()
        .await
        .finish(Err(CoreError::UnavailableTransport));
    assert_eq!(f.settled(run).await.state, RunState::Paused);
    f.sql(&delete_profile_sql(&f)).await;
    let (result, detail) = crate::failure::scope(f.coordinator.resume(run)).await;
    assert_eq!(result, Err(CoreError::Configuration));
    assert_refresh_detail(detail);
    assert_eq!(f.data(run).await.state, RunState::Paused);
    assert_eq!(f.repo.run_profile_revision(run).unwrap(), 1);
    assert!(f.requests.try_recv().is_err());
}

#[tokio::test]
async fn rejected_message_does_not_record_profile_revision() {
    let f = Fixture::new();
    edit_endpoint_url(&f);
    for text in [
        "   ".to_string(),
        "x".repeat(crate::inference::MAX_TEXT_BYTES + 1),
    ] {
        assert_eq!(
            f.coordinator.start_turn(f.conversation, text).await,
            Err(CoreError::InvalidInput)
        );
    }
    assert_eq!(current_conversation_revision(&f), 1);
    assert!(f.coordinator.snapshot().await.unwrap().runs.is_empty());
}

#[tokio::test]
async fn resume_after_concurrent_cancel_keeps_original_revision() {
    let mut f = Fixture::new();
    let run = f.start().await;
    f.request()
        .await
        .finish(Err(CoreError::UnavailableTransport));
    assert_eq!(f.settled(run).await.state, RunState::Paused);
    edit_endpoint_url(&f);
    // Resume's refresh committed, then a cancel lands before the rebind.
    assert!(f.repo.refresh_profile(f.conversation).unwrap());
    state::cancel(&f.repo, run).unwrap();
    assert_eq!(
        state::resume_on_current(&f.repo, run, RunState::Paused),
        Err(CoreError::Conflict)
    );
    assert_eq!(f.data(run).await.state, RunState::Cancelled);
    assert_eq!(f.repo.run_profile_revision(run).unwrap(), 1);
}

#[tokio::test]
async fn unknown_outcome_resume_after_settings_edit_is_refused_without_refresh() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let binding = f.waiting(run).await;
    f.approve(run, binding).await;
    let dispatched = f.calls.recv().await.expect("one controlled write");
    f.coordinator
        .platform_pause(run, f.host.generation())
        .await
        .unwrap();
    drop(dispatched);
    let data = f.settled(run).await;
    assert_eq!(data.state, RunState::Paused);
    assert_eq!(data.attempts[0].state, AttemptState::OutcomeUnknown);
    edit_endpoint_url(&f);
    assert_eq!(f.coordinator.resume(run).await, Err(CoreError::Conflict));
    assert_eq!(current_conversation_revision(&f), 1);
    assert_eq!(f.repo.run_profile_revision(run).unwrap(), 1);
    assert!(f.requests.try_recv().is_err());
    assert_eq!(f.writes.load(Ordering::SeqCst), 1);
}

/// The user messages of `conversation`, in order.
fn user_messages(f: &Fixture) -> Vec<String> {
    f.repo
        .conversation(f.conversation)
        .unwrap()
        .messages
        .into_iter()
        .filter(|m| m.message.role == "user")
        .map(|m| m.message.content)
        .collect()
}

async fn failed_turn(f: &mut Fixture) -> RunId {
    let run = f.start().await;
    f.request().await.finish(Err(CoreError::Authentication));
    assert_eq!(f.settled(run).await.state, RunState::Failed);
    run
}

#[tokio::test]
async fn retry_reruns_failed_turn_from_its_input_without_a_new_message() {
    let mut f = Fixture::new();
    let failed = failed_turn(&mut f).await;
    let input = f.data(failed).await.input_message_id;
    assert!(input.is_some());
    edit_endpoint_url(&f);
    let retry = f.coordinator.retry(failed).await.unwrap();
    assert_ne!(retry, failed);
    let request = f.request().await;
    // The fixed settings apply, and the input is sent once.
    assert_eq!(request.request.endpoint.base_url, "https://example.com/v1");
    let sent: Vec<_> = request
        .request
        .messages
        .iter()
        .filter(|m| matches!(m.role, ChatRole::User))
        .map(|m| m.content.as_str())
        .collect();
    assert_eq!(sent, ["do it"]);
    request.finish(Ok(turn(vec![])));
    let data = f.settled(retry).await;
    assert_eq!(data.state, RunState::Completed);
    assert_eq!(data.input_message_id, input);
    assert_eq!(user_messages(&f), ["do it"]);
    // The failed run stays failed and is not retried twice.
    assert_eq!(f.data(failed).await.state, RunState::Failed);
    assert_eq!(f.coordinator.retry(failed).await, Err(CoreError::Conflict));
    assert!(f.requests.try_recv().is_err());
}

#[tokio::test]
async fn retry_is_refused_for_a_run_that_did_not_fail() {
    let mut f = Fixture::new();
    let run = f.start().await;
    f.request()
        .await
        .finish(Err(CoreError::UnavailableTransport));
    assert_eq!(f.settled(run).await.state, RunState::Paused);
    assert_eq!(f.coordinator.retry(run).await, Err(CoreError::Conflict));
    f.coordinator.resume(run).await.unwrap();
    f.request().await.finish(Ok(turn(vec![])));
    assert_eq!(f.settled(run).await.state, RunState::Completed);
    assert_eq!(f.coordinator.retry(run).await, Err(CoreError::Conflict));
    assert!(f.requests.try_recv().is_err());
    assert_eq!(f.coordinator.snapshot().await.unwrap().runs.len(), 1);
}

#[tokio::test]
async fn retry_is_refused_while_a_tool_outcome_is_unresolved() {
    for unresolved in ["Dispatching", "OutcomeUnknown"] {
        let mut f = Fixture::new();
        let run = f.start().await;
        f.waiting(run).await;
        f.sql(&format!(
            "UPDATE attempts SET state='{unresolved}' WHERE run_id='{}';
             UPDATE runs SET state='Failed' WHERE id='{}';",
            run.0, run.0
        ))
        .await;
        assert_eq!(
            f.coordinator.retry(run).await,
            Err(CoreError::Conflict),
            "{unresolved}"
        );
        assert!(f.requests.try_recv().is_err());
        assert_eq!(f.writes.load(Ordering::SeqCst), 0);
        assert_eq!(f.coordinator.snapshot().await.unwrap().runs.len(), 1);
    }
}

#[tokio::test]
async fn retry_is_refused_once_a_later_run_exists() {
    let mut f = Fixture::new();
    let failed = failed_turn(&mut f).await;
    let later = f
        .coordinator
        .start_turn(f.conversation, "next".into())
        .await
        .unwrap();
    f.request().await.finish(Ok(turn(vec![])));
    f.settled(later).await;
    assert_eq!(f.coordinator.retry(failed).await, Err(CoreError::Conflict));
    assert!(f.requests.try_recv().is_err());
    assert_eq!(user_messages(&f), ["do it", "next"]);
}

#[tokio::test]
async fn retry_is_refused_for_a_summary_run_and_an_unknown_run() {
    let mut f = Fixture::new();
    let first = f.start().await;
    f.request().await.finish(Ok(turn(vec![])));
    f.settled(first).await;
    let snapshot = f.repo.conversation(f.conversation).unwrap();
    let selection = ContextSelection {
        message_ids: snapshot.messages.iter().map(|m| m.id).collect(),
        summary_ids: vec![],
    };
    let summary = f
        .coordinator
        .summarize_context(f.conversation, selection)
        .await
        .unwrap();
    f.request().await.finish(Err(CoreError::Authentication));
    assert_eq!(f.settled(summary).await.state, RunState::Failed);
    assert_eq!(f.coordinator.retry(summary).await, Err(CoreError::Conflict));
    assert_eq!(
        f.coordinator.retry(RunId::new()).await,
        Err(CoreError::Conflict)
    );
    assert!(f.requests.try_recv().is_err());
}

/// The ids of every run, and the number of events, in storage.
fn runs_and_events(f: &Fixture) -> (Vec<String>, i64) {
    f.repo
        .with_connection(|c| {
            let mut stmt = c.prepare("SELECT id FROM runs ORDER BY rowid").unwrap();
            let runs = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            let events = c
                .query_row("SELECT count(*) FROM events", [], |r| r.get(0))
                .unwrap();
            Ok((runs, events))
        })
        .unwrap()
}

#[tokio::test]
async fn start_retry_rechecks_eligibility_inside_its_transaction() {
    // A turn started after the preliminary check makes the failed run stale.
    let mut f = Fixture::new();
    let failed = failed_turn(&mut f).await;
    assert_eq!(state::retryable(&f.repo, failed), Ok(f.conversation));
    let later = RunId::new();
    state::start(&f.repo, later, f.conversation, "next".into()).unwrap();
    let before = runs_and_events(&f);
    assert_eq!(
        state::start_retry(&f.repo, RunId::new(), failed),
        Err(CoreError::Conflict)
    );
    assert_eq!(runs_and_events(&f), before);

    // A second retry of the same failed run, both past the preliminary check.
    let mut f = Fixture::new();
    let failed = failed_turn(&mut f).await;
    assert_eq!(state::retryable(&f.repo, failed), Ok(f.conversation));
    let first = RunId::new();
    state::start_retry(&f.repo, first, failed).unwrap();
    let before = runs_and_events(&f);
    assert_eq!(
        state::start_retry(&f.repo, RunId::new(), failed),
        Err(CoreError::Conflict)
    );
    assert_eq!(runs_and_events(&f), before);
    assert_eq!(before.0, [failed.0.to_string(), first.0.to_string()]);
}

#[tokio::test]
async fn archive_waits_for_local_cleanup_like_delete_and_blocks_turns_until_restored() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    assert_eq!(
        f.coordinator.set_archived(f.conversation, true).await,
        Err(CoreError::Conflict)
    );
    state::cancel(&f.repo, run).unwrap();
    // The DB is terminal, but the owned execution still holds its slot.
    assert_eq!(
        f.coordinator.set_archived(f.conversation, true).await,
        Err(CoreError::Conflict)
    );
    f.coordinator.cancel(run).await.unwrap();
    drop(request);
    f.coordinator
        .set_archived(f.conversation, true)
        .await
        .unwrap();
    let revision = f
        .repo
        .conversation(f.conversation)
        .unwrap()
        .profile
        .revision;
    let messages = user_messages(&f);
    assert_eq!(
        f.coordinator
            .start_turn(f.conversation, "archived".into())
            .await,
        Err(CoreError::Conflict)
    );
    // The refused turn left no message, run or profile revision behind.
    assert_eq!(user_messages(&f), messages);
    assert_eq!(f.coordinator.snapshot().await.unwrap().runs.len(), 1);
    assert_eq!(
        f.repo
            .conversation(f.conversation)
            .unwrap()
            .profile
            .revision,
        revision
    );
    assert!(f.requests.try_recv().is_err());
    f.coordinator
        .set_archived(f.conversation, false)
        .await
        .unwrap();
    let next = f.start().await;
    f.request().await.finish(Ok(turn(vec![])));
    assert_eq!(f.settled(next).await.state, RunState::Completed);
}

#[tokio::test]
async fn archived_conversation_refuses_resume_retry_and_summary_until_restored() {
    let mut f = Fixture::new();
    let paused = f.start().await;
    f.request()
        .await
        .finish(Err(CoreError::UnavailableTransport));
    assert_eq!(f.settled(paused).await.state, RunState::Paused);
    // A paused run is not active work, so archiving is allowed.
    f.coordinator
        .set_archived(f.conversation, true)
        .await
        .unwrap();
    assert_eq!(f.coordinator.resume(paused).await, Err(CoreError::Conflict));
    assert_eq!(f.data(paused).await.state, RunState::Paused);
    f.coordinator
        .set_archived(f.conversation, false)
        .await
        .unwrap();
    f.coordinator.resume(paused).await.unwrap();
    f.request().await.finish(Ok(turn(vec![])));
    assert_eq!(f.settled(paused).await.state, RunState::Completed);

    let failed = failed_turn(&mut f).await;
    f.coordinator
        .set_archived(f.conversation, true)
        .await
        .unwrap();
    assert_eq!(f.coordinator.retry(failed).await, Err(CoreError::Conflict));
    let snapshot = f.repo.conversation(f.conversation).unwrap();
    assert_eq!(
        f.coordinator
            .summarize_context(
                f.conversation,
                ContextSelection {
                    message_ids: snapshot.messages.iter().map(|m| m.id).collect(),
                    summary_ids: vec![],
                },
            )
            .await,
        Err(CoreError::Conflict)
    );
    assert!(f.requests.try_recv().is_err());
    f.coordinator
        .set_archived(f.conversation, false)
        .await
        .unwrap();
    let retry = f.coordinator.retry(failed).await.unwrap();
    f.request().await.finish(Ok(turn(vec![])));
    assert_eq!(f.settled(retry).await.state, RunState::Completed);
}

/// The stored failure's code and the reason of its run-limit detail.
fn limit_failure(data: &state::RunData) -> (CoreError, crate::failure::FailureDetail) {
    let failure = data.failure.clone().expect("stored failure");
    (failure.code, failure.detail.expect("failure detail"))
}

#[tokio::test]
async fn waiting_for_the_first_output_is_shown_and_not_charged() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    let wait = f.data(run).await.model_wait.expect("waiting");
    assert_eq!((wait.waited_ms, wait.silent_ms, wait.keepalives), (0, 0, 0));
    // The device case from issue #14: 884.6 s to the first token against the
    // default 15 min budget. Keepalives arrive every 30 s meanwhile.
    for _ in 0..29 {
        f.clock.advance(Duration::from_secs(30));
        request.events.send(InferenceEvent::Alive).await.unwrap();
        while request.events.capacity() != crate::inference::EVENT_CHANNEL_CAPACITY {
            tokio::task::yield_now().await;
        }
    }
    f.clock.advance(Duration::from_millis(14_620));
    let waiting = f.data(run).await;
    assert_eq!(waiting.state, RunState::Running);
    assert_eq!(
        waiting.model_wait,
        Some(ModelWait {
            waited_ms: 884_620,
            silent_ms: 14_620,
            keepalives: 29,
        })
    );
    assert_eq!(waiting.active_ms, 0);
    request.output("answer").await;
    assert_eq!(f.data(run).await.model_wait, None);
    // Generation is charged; the reply finishes inside the budget.
    f.clock.advance(Duration::from_secs(60));
    request.finish(Ok(turn(vec![])));
    let done = f.settled(run).await;
    assert_eq!(done.state, RunState::Completed);
    assert_eq!(done.active_ms, 60_000);
    assert_eq!(done.model_wait, None);
    let events = f.coordinator.subscribe(0).poll().await.unwrap();
    let count = |suffix: &str| {
        events
            .iter()
            .filter(|(_, _, p)| p == &format!("run:{}:{suffix}", run.0))
            .count()
    };
    assert_eq!(
        (count("waiting"), count("alive"), count("output")),
        (1, 29, 1)
    );
}

#[tokio::test]
async fn non_text_output_ends_the_wait() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    f.clock.advance(Duration::from_secs(600));
    request.events.send(InferenceEvent::Output).await.unwrap();
    while request.events.capacity() != crate::inference::EVENT_CHANNEL_CAPACITY {
        tokio::task::yield_now().await;
    }
    assert_eq!(f.data(run).await.model_wait, None);
    // Keepalives after output started change nothing.
    request.events.send(InferenceEvent::Alive).await.unwrap();
    f.clock.advance(Duration::from_secs(5));
    request.finish(Ok(turn(vec![f.tool_call("id-1")])));
    let data = f.settled(run).await;
    assert_eq!(data.state, RunState::WaitingApproval);
    assert_eq!(data.active_ms, 5000);
}

#[tokio::test]
async fn run_time_limit_counts_generation_and_names_the_limit() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    f.clock.advance(Duration::from_secs(1200));
    request.output("slow start").await;
    // The deadline moved by the uncharged 20 min wait.
    f.clock.advance(Duration::from_secs(899));
    tokio::task::yield_now().await;
    assert_eq!(f.data(run).await.state, RunState::Running);
    f.clock.advance(Duration::from_secs(1));
    let data = f.settled(run).await;
    drop(request);
    assert_eq!(data.state, RunState::Failed);
    assert_eq!(data.active_ms, 900_000);
    let (code, detail) = limit_failure(&data);
    assert_eq!(code, CoreError::Limit);
    assert_eq!(
        detail.target,
        crate::failure::FailureTarget::Profile {
            name: "profile".into()
        }
    );
    assert_eq!(detail.step, crate::failure::FailureStep::RunLimit);
    assert_eq!(
        detail.reason.as_deref(),
        Some("Run time limit (15 min) reached; time waiting for the model's first output is not counted")
    );
}

#[tokio::test]
async fn first_output_wait_is_bounded_and_named() {
    let mut f = Fixture::new();
    let run = f.start().await;
    let request = f.request().await;
    f.clock.advance(FIRST_OUTPUT_WAIT - Duration::from_secs(1));
    tokio::task::yield_now().await;
    assert_eq!(f.data(run).await.state, RunState::Running);
    f.clock.advance(Duration::from_secs(1));
    let data = f.settled(run).await;
    drop(request);
    assert_eq!(data.state, RunState::Failed);
    assert_eq!(data.active_ms, 0);
    let (code, detail) = limit_failure(&data);
    assert_eq!(code, CoreError::Limit);
    assert_eq!(
        detail.target,
        crate::failure::FailureTarget::Inference {
            name: "model".into()
        }
    );
    assert_eq!(
        detail.reason.as_deref(),
        Some("no output from the model within 1 h of sending the request")
    );
}

#[tokio::test]
async fn inference_request_limit_names_the_limit() {
    let mut f = Fixture::new();
    f.set_limits(RunLimits {
        max_inference_requests: 1,
        ..RunLimits::default()
    });
    let run = f.start().await;
    let binding = f.waiting(run).await;
    f.approve(run, binding).await;
    f.calls
        .recv()
        .await
        .unwrap()
        .reply
        .send(Ok(outcome(false, 2)))
        .unwrap();
    let data = f.settled(run).await;
    assert_eq!(data.state, RunState::Failed);
    assert_eq!(data.inference_count, 1);
    let (code, detail) = limit_failure(&data);
    assert_eq!(code, CoreError::Limit);
    assert_eq!(detail.step, crate::failure::FailureStep::RunLimit);
    assert_eq!(
        detail.reason.as_deref(),
        Some("Inference request limit (1 per run) reached")
    );
}

#[tokio::test]
async fn tool_attempt_limit_names_the_limit() {
    let mut f = Fixture::new();
    f.set_limits(RunLimits {
        max_tool_attempts: 1,
        ..RunLimits::default()
    });
    let run = f.start().await;
    f.request()
        .await
        .finish(Ok(turn(vec![f.tool_call("a"), f.tool_call("b")])));
    let data = f.settled(run).await;
    assert_eq!(data.state, RunState::Failed);
    assert_eq!(data.attempt_count, 0);
    let (code, detail) = limit_failure(&data);
    assert_eq!(code, CoreError::Limit);
    assert_eq!(detail.step, crate::failure::FailureStep::RunLimit);
    assert_eq!(
        detail.reason.as_deref(),
        Some("Tool call limit (1 per run) reached: the model asked for 2 more after 0")
    );
}

#[test]
fn durations_read_as_short_text() {
    for (ms, text) in [
        (900_000, "15 min"),
        (885_000, "14 min 45 s"),
        (3_600_000, "1 h"),
        (5_430_000, "1 h 30 min 30 s"),
        (45_000, "45 s"),
        (500, "500 ms"),
    ] {
        assert_eq!(duration_text(ms), text);
    }
}
