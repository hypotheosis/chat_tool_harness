use crate::model::core::{
    AttemptId, AttemptState, EndpointConfig, EndpointId, Profile, ProfileId, RunId, RunLimits,
    RunState,
};
use crate::storage::Repository;
use rusqlite::Connection;
use tempfile::tempdir;

fn endpoint(id: EndpointId) -> EndpointConfig {
    EndpointConfig {
        id,
        name: "endpoint".into(),
        base_url: "https://example.test".into(),
        model: "model".into(),
        context_tokens: 100,
        max_output_tokens: 10,
        temperature: 0.1,
        allow_http: false,
        credential: None,
    }
}
fn profile(id: ProfileId, endpoint_id: EndpointId) -> Profile {
    Profile {
        mode: crate::model::core::ProfileMode::Agent,
        id,
        name: "profile".into(),
        endpoint_id,
        server_ids: vec![],
        instructions: String::new(),
        limits: RunLimits::default(),
    }
}
fn setup(repository: &Repository) -> ProfileId {
    let endpoint_id = EndpointId::new();
    repository
        .save_endpoint(endpoint(endpoint_id))
        .expect("save endpoint");
    let profile_id = ProfileId::new();
    repository
        .save_profile(profile(profile_id, endpoint_id))
        .expect("save profile");
    profile_id
}

#[test]
fn reopen_preserves_separate_conversations() {
    let dir = tempdir().expect("temporary directory");
    let path = dir.path().join("core.db");
    let repository = Repository::open(&path).expect("open database");
    let profile_id = setup(&repository);
    let first = repository
        .create_conversation(profile_id)
        .expect("first conversation");
    let second = repository
        .create_conversation(profile_id)
        .expect("second conversation");
    drop(repository);
    let reopened = Repository::open(&path).expect("reopen database");
    assert_ne!(first, second);
    assert_eq!(
        reopened.conversation(first).expect("first snapshot").id,
        first
    );
    assert_eq!(
        reopened.conversation(second).expect("second snapshot").id,
        second
    );
}
#[test]
fn migration_failure_rolls_back_via_apply() {
    let dir = tempdir().expect("temporary directory");
    let path = dir.path().join("core.db");
    let mut raw = Connection::open(&path).expect("open raw db");
    raw.execute_batch("CREATE TABLE endpoints (id TEXT PRIMARY KEY); CREATE TABLE marker (value TEXT); INSERT INTO marker VALUES ('kept');").expect("seed collision");
    assert!(super::migrations::apply(&mut raw).is_err());
    let version: i64 = raw
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("version");
    assert_eq!(version, 0);
    let count: i64 = raw
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='servers'",
            [],
            |row| row.get(0),
        )
        .expect("schema");
    assert_eq!(count, 0);
    let marker: String = raw
        .query_row("SELECT value FROM marker", [], |row| row.get(0))
        .expect("old data");
    assert_eq!(marker, "kept");
}

#[test]
fn future_version_is_rejected_without_mutation() {
    let dir = tempdir().expect("temporary directory");
    let path = dir.path().join("core.db");
    let raw = Connection::open(&path).expect("open raw db");
    raw.execute_batch("PRAGMA user_version=99; CREATE TABLE marker(value TEXT); INSERT INTO marker VALUES ('kept')").expect("seed future");
    drop(raw);
    assert!(Repository::open(&path).is_err());
    let raw = Connection::open(&path).expect("reopen raw");
    let version: i64 = raw
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("version");
    assert_eq!(version, 99);
    let marker: String = raw
        .query_row("SELECT value FROM marker", [], |row| row.get(0))
        .expect("old data");
    assert_eq!(marker, "kept");
}

#[test]
fn v1_upgrade_preserves_records_and_does_not_trust_legacy_rules() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("core.db");
    let mut raw = Connection::open(&path).unwrap();
    super::migrations::migrate_v1_for_test(&mut raw).unwrap();
    raw.execute(
        "INSERT INTO endpoints(id,payload) VALUES('old-endpoint','{}')",
        [],
    )
    .unwrap();
    raw.execute(
        "INSERT INTO profiles(id,payload,revision) VALUES('old-profile','{}',1)",
        [],
    )
    .unwrap();
    raw.execute("INSERT INTO permission_rules(id,profile_id,payload) VALUES('legacy','old-profile','{\"rule\":\"Allow\"}')", []).unwrap();
    drop(raw);
    let repo = Repository::open(&path).unwrap();
    assert_eq!(repo.schema_version_for_test().unwrap(), 6);
    let raw = Connection::open(&path).unwrap();
    let count: i64 = raw
        .query_row(
            "SELECT count(*) FROM permission_rules WHERE id='legacy'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    let trusted: i64 = raw
        .query_row("SELECT count(*) FROM tool_permission_rules", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(trusted, 0);
    let observed: i64 = raw
        .query_row("SELECT count(*) FROM observed_tool_definitions", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(observed, 0);
    let endpoint: String = raw
        .query_row(
            "SELECT id FROM endpoints WHERE id='old-endpoint'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(endpoint, "old-endpoint");
}

#[test]
fn v2_upgrade_failure_preserves_version_and_data() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("core.db");
    let mut raw = Connection::open(&path).unwrap();
    super::migrations::migrate_v1_for_test(&mut raw).unwrap();
    super::migrations::migrate_v2_for_test(&mut raw).unwrap();
    raw.execute_batch("CREATE TABLE observed_tool_definitions (collision TEXT); INSERT INTO endpoints(id,payload) VALUES('kept','{}')").unwrap();
    assert!(super::migrations::apply(&mut raw).is_err());
    let version: i64 = raw
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 2);
    let count: i64 = raw
        .query_row("SELECT count(*) FROM endpoints WHERE id='kept'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn v4_context_upgrade_preserves_transcript_and_defaults_without_rewriting() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("core.db");
    let mut raw = Connection::open(&path).unwrap();
    super::migrations::migrate_v4_for_test(&mut raw).unwrap();
    raw.execute_batch("INSERT INTO conversations VALUES('conversation',1); INSERT INTO conversation_snapshots VALUES('conversation',1,'{}'); INSERT INTO runs(id,conversation_id,snapshot_revision,state,input_message_id) VALUES('run','conversation',1,'Paused','input'); INSERT INTO messages VALUES('input','conversation',1,0,'original input');").unwrap();
    super::migrations::apply(&mut raw).unwrap();
    let context: String = raw
        .query_row("SELECT context_json FROM runs WHERE id='run'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let default: crate::agent::context::RunContext = serde_json::from_str(&context).unwrap();
    assert!(default.selection.is_none());
    assert!(default.usage.is_none());
    let text: String = raw
        .query_row("SELECT payload FROM messages WHERE id='input'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(text, "original input");
    let summaries: i64 = raw
        .query_row("SELECT count(*) FROM derived_context", [], |r| r.get(0))
        .unwrap();
    assert_eq!(summaries, 0);
    super::migrations::apply(&mut raw).unwrap();
}

#[test]
fn current_migration_is_noop() {
    let dir = tempdir().expect("temporary directory");
    let path = dir.path().join("core.db");
    let repository = Repository::open(&path).expect("open database");
    let profile = setup(&repository);
    let conversation = repository
        .create_conversation(profile)
        .expect("conversation");
    assert_eq!(
        repository
            .schema_version_for_test()
            .expect("schema version"),
        6
    );
    drop(repository);
    let reopened = Repository::open(&path).expect("current version opens");
    assert_eq!(
        reopened
            .conversation(conversation)
            .expect("unchanged data")
            .id,
        conversation
    );
}
#[test]
fn profile_fk_failure_preserves_prior_revision_and_links() {
    let dir = tempdir().expect("temporary directory");
    let path = dir.path().join("core.db");
    let repository = Repository::open(&path).expect("open db");
    let endpoint_id = EndpointId::new();
    repository
        .save_endpoint(endpoint(endpoint_id))
        .expect("endpoint");
    let server = crate::model::core::ServerConfig {
        id: crate::model::core::ServerId::new(),
        name: "server".into(),
        url: "https://example.test".into(),
        allow_http: false,
        secret_headers: vec![],
    };
    repository.save_server(server.clone()).expect("server");
    let profile_id = ProfileId::new();
    let mut original = profile(profile_id, endpoint_id);
    original.server_ids.push(server.id);
    repository.save_profile(original.clone()).expect("original");
    let conversation = repository
        .create_conversation(profile_id)
        .expect("conversation");
    let mut invalid = original.clone();
    invalid.instructions = "must rollback".into();
    invalid.server_ids.push(crate::model::core::ServerId::new());
    assert!(repository.save_profile(invalid).is_err());
    drop(repository);
    let reopened = Repository::open(&path).expect("reopen");
    reopened
        .apply_profile(conversation, profile_id)
        .expect("apply original");
    let snapshot = reopened.conversation(conversation).expect("snapshot");
    assert_eq!(snapshot.profile.instructions, original.instructions);
    assert_eq!(snapshot.profile.servers, vec![server]);
    let raw = Connection::open(&path).expect("raw db");
    let revision: i64 = raw
        .query_row(
            "SELECT revision FROM profiles WHERE id=?1",
            [profile_id.0.to_string()],
            |row| row.get(0),
        )
        .expect("revision");
    assert_eq!(revision, 1);
    let links: i64 = raw
        .query_row(
            "SELECT count(*) FROM profile_servers WHERE profile_id=?1",
            [profile_id.0.to_string()],
            |row| row.get(0),
        )
        .expect("links");
    assert_eq!(links, 1);
}

#[test]
fn dispatching_recovers_unknown() {
    let dir = tempdir().expect("temporary directory");
    let repository = Repository::open(&dir.path().join("core.db")).expect("open database");
    let conversation = repository
        .create_conversation(setup(&repository))
        .expect("conversation");
    let run = RunId::new();
    let attempt = AttemptId::new();
    repository
        .insert_run_for_test(run, conversation, RunState::Running)
        .expect("running run");
    repository
        .insert_attempt_for_test(attempt, run, AttemptState::Dispatching)
        .expect("dispatching attempt");
    assert_eq!(
        repository.recover_interrupted().expect("recover"),
        vec![run]
    );
    let state = repository.conversation(conversation).expect("conversation");
    assert_eq!(state.runs[0].state, RunState::Interrupted);
    assert_eq!(
        state.runs[0].attempts[0].state,
        AttemptState::OutcomeUnknown
    );
}

#[test]
fn recovery_after_reopen_is_idempotent_and_preserves_terminal_rows() {
    let dir = tempdir().expect("temporary directory");
    let path = dir.path().join("core.db");
    let repository = Repository::open(&path).expect("open db");
    let conversation = repository
        .create_conversation(setup(&repository))
        .expect("conversation");
    let states = [
        RunState::Running,
        RunState::WaitingApproval,
        RunState::Paused,
        RunState::Completed,
        RunState::Failed,
        RunState::Cancelled,
    ];
    let ids: Vec<_> = states
        .iter()
        .map(|state| {
            let run = RunId::new();
            repository
                .insert_run_for_test(run, conversation, *state)
                .expect("run");
            repository
                .insert_attempt_for_test(
                    AttemptId::new(),
                    run,
                    if *state == RunState::Completed {
                        AttemptState::Succeeded
                    } else {
                        AttemptState::Dispatching
                    },
                )
                .expect("attempt");
            run
        })
        .collect();
    drop(repository);
    let reopened = Repository::open(&path).expect("reopen");
    let recovered = reopened.recover_interrupted().expect("recover");
    assert_eq!(recovered.len(), 3);
    assert!(ids[..3].iter().all(|id| recovered.contains(id)));
    assert!(reopened.recover_interrupted().expect("again").is_empty());
    let snapshot = reopened.conversation(conversation).expect("snapshot");
    assert_eq!(
        snapshot
            .runs
            .iter()
            .map(|run| run.state)
            .collect::<Vec<_>>(),
        [
            RunState::Interrupted,
            RunState::Interrupted,
            RunState::Interrupted,
            RunState::Completed,
            RunState::Failed,
            RunState::Cancelled
        ]
    );
    assert_eq!(snapshot.runs[3].attempts[0].state, AttemptState::Succeeded);
    assert_eq!(
        snapshot.runs[4].attempts[0].state,
        AttemptState::OutcomeUnknown
    );
    assert_eq!(
        snapshot.runs[5].attempts[0].state,
        AttemptState::OutcomeUnknown
    );
    assert_eq!(
        snapshot.runs[0].attempts[0].state,
        AttemptState::OutcomeUnknown
    );
}

#[test]
fn worker_shutdown_survives_clones_and_reopen() {
    let dir = tempdir().expect("temporary directory");
    let path = dir.path().join("core.db");
    let repository = Repository::open(&path).expect("open db");
    let clone = repository.clone();
    let conversation = clone
        .create_conversation(setup(&repository))
        .expect("conversation");
    drop(repository);
    assert_eq!(
        clone.conversation(conversation).expect("clone works").id,
        conversation
    );
    drop(clone);
    assert_eq!(
        Repository::open(&path)
            .expect("worker released")
            .conversation(conversation)
            .expect("snapshot")
            .id,
        conversation
    );
}
