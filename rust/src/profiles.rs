#[cfg(test)]
mod tests {
    use crate::model::core::{
        CredentialRef, EndpointConfig, EndpointId, Profile, ProfileId, RunLimits,
    };
    use crate::storage::Repository;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn endpoint(id: EndpointId, credential: Option<CredentialRef>) -> EndpointConfig {
        EndpointConfig {
            id,
            name: "endpoint".into(),
            base_url: "https://example.test/v1".into(),
            model: "test-model".into(),
            context_tokens: 1024,
            max_output_tokens: 128,
            temperature: 0.5,
            allow_http: false,
            credential,
        }
    }

    fn profile(id: ProfileId, endpoint_id: EndpointId) -> Profile {
        Profile {
            mode: crate::model::core::ProfileMode::Agent,
            id,
            name: "profile".into(),
            endpoint_id,
            server_ids: vec![],
            instructions: "be useful".into(),
            limits: RunLimits::default(),
        }
    }

    #[test]
    fn profile_edits_do_not_rewrite_snapshot() {
        let dir = tempdir().expect("temporary directory");
        let repository = Repository::open(&dir.path().join("core.db")).expect("open database");
        let endpoint_id = EndpointId::new();
        repository
            .save_endpoint(endpoint(endpoint_id, None))
            .expect("save endpoint");
        let profile_id = ProfileId::new();
        repository
            .save_profile(profile(profile_id, endpoint_id))
            .expect("save profile");
        let conversation = repository
            .create_conversation(profile_id)
            .expect("create conversation");
        let mut edited = profile(profile_id, endpoint_id);
        edited.instructions = "new instructions".into();
        repository.save_profile(edited).expect("edit profile");
        assert_eq!(
            repository
                .conversation(conversation)
                .expect("conversation")
                .profile
                .instructions,
            "be useful"
        );
    }

    #[test]
    fn apply_profile_changes_future_turns_only() {
        let dir = tempdir().expect("temporary directory");
        let repository = Repository::open(&dir.path().join("core.db")).expect("open database");
        let endpoint_id = EndpointId::new();
        repository
            .save_endpoint(endpoint(endpoint_id, None))
            .expect("save endpoint");
        let profile_id = ProfileId::new();
        repository
            .save_profile(profile(profile_id, endpoint_id))
            .expect("save profile");
        let conversation = repository
            .create_conversation(profile_id)
            .expect("create conversation");
        let first = repository
            .conversation(conversation)
            .expect("first snapshot");
        let mut updated = profile(profile_id, endpoint_id);
        updated.instructions = "future instructions".into();
        repository
            .save_profile(updated)
            .expect("save changed profile");
        repository
            .apply_profile(conversation, profile_id)
            .expect("apply profile");
        let second = repository
            .conversation(conversation)
            .expect("second snapshot");
        assert!(second.profile.revision > first.profile.revision);
        assert_eq!(first.profile.instructions, "be useful");
        assert_eq!(second.profile.instructions, "future instructions");
    }

    #[test]
    fn saved_revisions_survive_reopen_and_bind_runs() {
        let dir = tempdir().expect("temporary directory");
        let path = dir.path().join("core.db");
        let repository = Repository::open(&path).expect("open db");
        let endpoint_id = EndpointId::new();
        repository
            .save_endpoint(endpoint(endpoint_id, None))
            .expect("endpoint");
        let first_id = ProfileId::new();
        let second_id = ProfileId::new();
        repository
            .save_profile(profile(first_id, endpoint_id))
            .expect("first profile");
        let mut other = profile(second_id, endpoint_id);
        other.instructions = "second profile".into();
        repository.save_profile(other).expect("second profile");
        let conversation = repository
            .create_conversation(first_id)
            .expect("conversation");
        let run = crate::model::core::RunId::new();
        repository
            .insert_run_for_test(run, conversation, crate::model::core::RunState::Completed)
            .expect("historical run");
        repository
            .apply_profile(conversation, second_id)
            .expect("apply distinct profile");
        drop(repository);
        let reopened = Repository::open(&path).expect("reopen");
        let current = reopened.conversation(conversation).expect("current");
        assert_eq!(current.profile.revision, 2);
        assert_eq!(current.profile.instructions, "second profile");
        let historical = reopened
            .conversation_profile_revision(conversation, 1)
            .expect("historical snapshot");
        assert_eq!(historical.instructions, "be useful");
        assert_eq!(reopened.run_profile_revision(run).expect("run revision"), 1);
    }

    #[test]
    fn invalid_server_and_duplicate_profile_server_are_rejected() {
        let dir = tempdir().expect("temporary directory");
        let repository = Repository::open(&dir.path().join("core.db")).expect("open db");
        let endpoint_id = EndpointId::new();
        repository
            .save_endpoint(endpoint(endpoint_id, None))
            .expect("endpoint");
        let server_id = crate::model::core::ServerId::new();
        let server = crate::model::core::ServerConfig {
            id: server_id,
            name: "server".into(),
            url: "https://example.test".into(),
            allow_http: false,
            secret_headers: vec![],
        };
        let mut invalid = server.clone();
        invalid.name = "  ".into();
        assert!(matches!(
            repository.save_server(invalid),
            Err(crate::error::CoreError::InvalidInput)
        ));
        for header in ["", "bad name", "bad:name", "X-\u{00e9}"] {
            let mut invalid = server.clone();
            invalid
                .secret_headers
                .push((header.into(), CredentialRef::new()));
            assert!(matches!(
                repository.save_server(invalid),
                Err(crate::error::CoreError::InvalidInput)
            ));
        }
        repository.save_server(server).expect("valid server");
        let mut duplicate = profile(ProfileId::new(), endpoint_id);
        duplicate.server_ids = vec![server_id, server_id];
        assert!(matches!(
            repository.save_profile(duplicate),
            Err(crate::error::CoreError::InvalidInput)
        ));
    }

    #[test]
    fn missing_credential_reference_does_not_hide_history() {
        let dir = tempdir().expect("temporary directory");
        let repository = Repository::open(&dir.path().join("core.db")).expect("open database");
        let endpoint_id = EndpointId::new();
        repository
            .save_endpoint(endpoint(endpoint_id, Some(CredentialRef(Uuid::new_v4()))))
            .expect("save endpoint");
        let profile_id = ProfileId::new();
        repository
            .save_profile(profile(profile_id, endpoint_id))
            .expect("save profile");
        let conversation = repository
            .create_conversation(profile_id)
            .expect("create conversation");
        assert!(repository
            .conversation(conversation)
            .expect("history survives")
            .profile
            .endpoint
            .credential
            .is_some());
    }
}
