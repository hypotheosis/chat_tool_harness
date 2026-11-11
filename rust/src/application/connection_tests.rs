//! The last result of each connection test, kept until the next test of the
//! same target replaces it: one per endpoint and test kind, one per MCP server.
//!
//! Results hold counts, the tool-support verdict, skipped tools and the
//! failure kind with its sanitized [`FailureDetail`]; never credentials,
//! headers or response bodies.
use super::*;
use crate::failure::FailureDetail;
use serde::{Deserialize, Serialize};

/// What was tested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Target {
    Endpoint(core::EndpointId, ConnectionTestKind),
    Server(core::ServerId, core::ProfileId),
}
impl Target {
    /// The target of a connection-test diagnostic; none for other diagnostics
    /// and for endpoint ids that do not parse (the test fails before running).
    pub(super) fn of(command: &Diagnostic) -> Option<Self> {
        let endpoint = |id: &str, kind| {
            uuid(id)
                .ok()
                .map(|id| Self::Endpoint(core::EndpointId(id), kind))
        };
        match command {
            Diagnostic::Models(id) => endpoint(id, ConnectionTestKind::DiscoverModels),
            Diagnostic::Probe(id) => endpoint(id, ConnectionTestKind::ProbeTools),
            Diagnostic::Server(profile, server) => Some(Self::Server(*server, *profile)),
            _ => None,
        }
    }
}

/// What a test found, as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Outcome {
    Models {
        count: u32,
    },
    ToolSupport {
        support: ToolSupport,
    },
    Tools {
        available: u32,
        skipped: Vec<(String, String)>,
    },
    Failed {
        code: CoreError,
        detail: Option<FailureDetail>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct StoredTest {
    tested_at_ms: i64,
    outcome: Outcome,
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The record of a finished test, or none when the test did not finish
/// (cancelled by the caller or by closing the app), so the previous result
/// stays in place.
pub(super) fn record(
    target: &Target,
    result: &Result<AgentCommandReply, (CoreError, Option<FailureDetail>)>,
    tested_at_ms: i64,
) -> Option<StoredTest> {
    let outcome = match result {
        Err((CoreError::Cancelled, _)) => return None,
        Err((code, detail)) => Outcome::Failed {
            code: code.clone(),
            detail: detail.clone(),
        },
        Ok(reply) => match target {
            Target::Endpoint(_, ConnectionTestKind::DiscoverModels) => Outcome::Models {
                // Counted as the settings screen lists them: once per name.
                count: count(
                    reply
                        .models
                        .iter()
                        .collect::<std::collections::HashSet<_>>()
                        .len(),
                ),
            },
            Target::Endpoint(_, _) => Outcome::ToolSupport {
                support: reply.tool_support?,
            },
            Target::Server(..) => Outcome::Tools {
                available: count(reply.discovered_tools.len()),
                skipped: reply
                    .skipped_tools
                    .iter()
                    .map(|t| (t.name.clone(), t.reason.clone()))
                    .collect(),
            },
        },
    };
    Some(StoredTest {
        tested_at_ms,
        outcome,
    })
}

fn kind_key(kind: ConnectionTestKind) -> &'static str {
    match kind {
        ConnectionTestKind::DiscoverModels => "discoverModels",
        ConnectionTestKind::ProbeTools => "probeTools",
        ConnectionTestKind::DiscoverServer => "discoverServer",
    }
}

fn kind_from_key(key: &str) -> CoreResult<ConnectionTestKind> {
    match key {
        "discoverModels" => Ok(ConnectionTestKind::DiscoverModels),
        "probeTools" => Ok(ConnectionTestKind::ProbeTools),
        _ => Err(CoreError::Storage),
    }
}

/// Replaces the stored result for `target` with `record`. A target that is
/// no longer saved stores nothing.
pub(super) fn store(repo: &Repository, target: Target, record: StoredTest) -> CoreResult<()> {
    let payload = match &target {
        Target::Endpoint(..) => serde_json::to_string(&record),
        Target::Server(_, profile) => serde_json::to_string(&ServerRecord {
            profile_id: profile.0.to_string(),
            record,
        }),
    }
    .map_err(|_| CoreError::Storage)?;
    repo.with_connection(move |conn| {
        match target {
            Target::Endpoint(id, kind) => conn.execute(
                "INSERT OR REPLACE INTO endpoint_tests(endpoint_id,kind,payload) SELECT ?1,?2,?3 WHERE EXISTS(SELECT 1 FROM endpoints WHERE id=?1)",
                rusqlite::params![id.0.to_string(), kind_key(kind), payload],
            ),
            Target::Server(id, _) => conn.execute(
                "INSERT OR REPLACE INTO server_tests(server_id,payload) SELECT ?1,?2 WHERE EXISTS(SELECT 1 FROM servers WHERE id=?1)",
                rusqlite::params![id.0.to_string(), payload],
            ),
        }
        .map(|_| ())
        .map_err(|_| CoreError::Storage)
    })
}

/// A server result also names the profile it was tested through.
#[derive(Serialize, Deserialize)]
struct ServerRecord {
    profile_id: String,
    #[serde(flatten)]
    record: StoredTest,
}

fn to_dto(
    kind: ConnectionTestKind,
    endpoint_id: Option<String>,
    server_id: Option<String>,
    profile_id: Option<String>,
    record: StoredTest,
) -> AgentConnectionTest {
    let mut test = AgentConnectionTest {
        kind,
        endpoint_id,
        server_id,
        profile_id,
        tested_at_ms: record.tested_at_ms,
        failure_code: None,
        failure_detail: None,
        model_count: None,
        tool_support: None,
        tool_count: None,
        skipped_tools: Vec::new(),
    };
    match record.outcome {
        Outcome::Models { count } => test.model_count = Some(count),
        Outcome::ToolSupport { support } => test.tool_support = Some(support),
        Outcome::Tools { available, skipped } => {
            test.tool_count = Some(available);
            test.skipped_tools = skipped
                .into_iter()
                .map(|(name, reason)| AgentSkippedTool { name, reason })
                .collect();
        }
        Outcome::Failed { code, detail } => {
            test.failure_code = Some(code);
            test.failure_detail = detail.map(Into::into);
        }
    }
    test
}

/// Every stored result: endpoints first, then servers, each in save order.
pub(super) fn all(repo: &Repository) -> CoreResult<Vec<AgentConnectionTest>> {
    repo.with_connection(|conn| {
        let storage = |_| CoreError::Storage;
        let mut tests = Vec::new();
        let mut stmt = conn
            .prepare("SELECT endpoint_id,kind,payload FROM endpoint_tests ORDER BY rowid")
            .map_err(storage)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(storage)?;
        for row in rows {
            let (endpoint, kind, payload) = row.map_err(storage)?;
            let record: StoredTest =
                serde_json::from_str(&payload).map_err(|_| CoreError::Storage)?;
            tests.push(to_dto(
                kind_from_key(&kind)?,
                Some(endpoint),
                None,
                None,
                record,
            ));
        }
        let mut stmt = conn
            .prepare("SELECT server_id,payload FROM server_tests ORDER BY rowid")
            .map_err(storage)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(storage)?;
        for row in rows {
            let (server, payload) = row.map_err(storage)?;
            let stored: ServerRecord =
                serde_json::from_str(&payload).map_err(|_| CoreError::Storage)?;
            tests.push(to_dto(
                ConnectionTestKind::DiscoverServer,
                None,
                Some(server),
                Some(stored.profile_id),
                stored.record,
            ));
        }
        Ok(tests)
    })
}

/// Milliseconds since the Unix epoch, negative for a clock set before it.
pub(super) fn unix_ms(time: std::time::SystemTime) -> i64 {
    match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(since) => i64::try_from(since.as_millis()).unwrap_or(i64::MAX),
        Err(before) => i64::try_from(before.duration().as_millis()).map_or(i64::MIN, |ms| -ms),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply() -> AgentCommandReply {
        AgentCommandReply::empty()
    }
    fn endpoint(kind: ConnectionTestKind) -> Target {
        Target::Endpoint(core::EndpointId::new(), kind)
    }

    #[test]
    fn cancelled_tests_leave_no_record() {
        for target in [
            endpoint(ConnectionTestKind::DiscoverModels),
            Target::Server(core::ServerId::new(), core::ProfileId::new()),
        ] {
            assert_eq!(record(&target, &Err((CoreError::Cancelled, None)), 1), None);
        }
    }

    #[test]
    fn model_count_counts_each_name_once() {
        let mut models = reply();
        models.models = vec!["a".into(), "b".into(), "a".into()];
        let stored = record(
            &endpoint(ConnectionTestKind::DiscoverModels),
            &Ok(models),
            7,
        )
        .unwrap();
        assert_eq!(stored.tested_at_ms, 7);
        assert_eq!(stored.outcome, Outcome::Models { count: 2 });
    }

    #[test]
    fn probe_without_a_verdict_records_nothing() {
        let target = endpoint(ConnectionTestKind::ProbeTools);
        assert_eq!(record(&target, &Ok(reply()), 1), None);
        let mut probed = reply();
        probed.tool_support = Some(ToolSupport::Unsupported);
        assert_eq!(
            record(&target, &Ok(probed), 1).unwrap().outcome,
            Outcome::ToolSupport {
                support: ToolSupport::Unsupported
            }
        );
    }

    #[test]
    fn failures_keep_their_code_and_detail() {
        let detail = FailureDetail {
            target: crate::failure::FailureTarget::McpServer { name: "s".into() },
            step: crate::failure::FailureStep::Initialize,
            url: Some("https://example.test/mcp".into()),
            status: Some(401),
            reason: Some("unknown API key".into()),
        };
        let stored = record(
            &Target::Server(core::ServerId::new(), core::ProfileId::new()),
            &Err((CoreError::Authentication, Some(detail.clone()))),
            1,
        )
        .unwrap();
        let dto = to_dto(
            ConnectionTestKind::DiscoverServer,
            None,
            Some("s".into()),
            Some("p".into()),
            stored,
        );
        assert_eq!(dto.failure_code, Some(CoreError::Authentication));
        assert_eq!(dto.failure_detail, Some(detail.into()));
        assert_eq!(dto.tool_count, None);
    }

    #[test]
    fn unix_ms_handles_clocks_before_the_epoch() {
        let epoch = std::time::UNIX_EPOCH;
        let second = std::time::Duration::from_secs(1);
        assert_eq!(unix_ms(epoch + second), 1000);
        assert_eq!(unix_ms(epoch - second), -1000);
    }
}
