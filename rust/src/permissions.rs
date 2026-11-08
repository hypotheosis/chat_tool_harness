use crate::error::{CoreError, CoreResult};
use crate::model::core::{
    ApprovalBinding, AttemptId, Authorization, PermissionRule, ProfileId, ServerId, ToolCall,
    ToolDefinition, ToolKey, ToolServer,
};
use crate::storage::Repository;
use rusqlite::{params, OptionalExtension};
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

/// Maximum raw JSON bytes accepted at this boundary (both arguments and schema).
pub const MAX_TOOL_JSON_BYTES: usize = 256 * 1024;

struct Strict(Value);
impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StrictVisitor;
        impl<'de> Visitor<'de> for StrictVisitor {
            type Value = Strict;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("valid JSON without duplicate object keys")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(Strict(Value::Bool(v)))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(Strict(Value::from(v)))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(Strict(Value::from(v)))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                Ok(Strict(Value::from(v)))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(Strict(Value::String(v.into())))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(Strict(Value::String(v)))
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(Strict(Value::Null))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(Strict(Value::Null))
            }
            fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
                Strict::deserialize(d)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some(v) = a.next_element::<Strict>()? {
                    out.push(v.0);
                }
                Ok(Strict(Value::Array(out)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut out = Map::new();
                let mut seen = HashSet::new();
                while let Some((k, v)) = a.next_entry::<String, Strict>()? {
                    if !seen.insert(k.clone()) {
                        return Err(serde::de::Error::custom("duplicate key"));
                    }
                    out.insert(k, v.0);
                }
                Ok(Strict(Value::Object(out)))
            }
        }
        deserializer.deserialize_any(StrictVisitor)
    }
}
pub(crate) fn parse_json(raw: &str) -> CoreResult<Value> {
    if raw.len() > MAX_TOOL_JSON_BYTES {
        return Err(CoreError::Limit);
    }
    let mut d = serde_json::Deserializer::from_str(raw);
    let parsed = Strict::deserialize(&mut d)
        .map_err(|_| CoreError::InvalidInput)?
        .0;
    d.end().map_err(|_| CoreError::InvalidInput)?;
    Ok(parsed)
}
fn digest(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
fn value_digest(value: &Value) -> CoreResult<String> {
    Ok(digest(
        &serde_json::to_vec(value).map_err(|_| CoreError::InvalidInput)?,
    ))
}
fn key_parts(key: &ToolKey) -> (String, String, String) {
    let server = match key.server {
        ToolServer::Mcp(id) => format!("mcp:{}", id.0),
        ToolServer::LocalFiles => "local-files".into(),
    };
    (key.profile_id.0.to_string(), server, key.name.clone())
}
/// Why a discovered tool's input schema cannot be used to check its arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SchemaRejection {
    /// Not a JSON object, or not parseable within the size limit.
    NotAnObject,
    /// A `$ref` names a document other than the schema itself; nothing is fetched.
    ExternalReference,
    /// The validator cannot compile it as JSON Schema 2020-12.
    Invalid,
}
impl SchemaRejection {
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::NotAnObject => "input schema is not a JSON object",
            Self::ExternalReference => {
                "input schema refers to another document; only references within the schema are supported"
            }
            Self::Invalid => "input schema is not valid JSON Schema 2020-12",
        }
    }
}
pub(crate) fn validate_discovery_schema(raw: &str) -> Result<(), SchemaRejection> {
    let schema = parse_json(raw).map_err(|_| SchemaRejection::NotAnObject)?;
    compile_schema(&schema).map(|_| ())
}
/// The validator is the gatekeeper: whatever it compiles, it enforces.
///
/// Unknown keywords and unknown `format` values are annotations under 2020-12
/// and do not constrain arguments. The crate is built without remote or file
/// resolvers, so only references within the schema itself can resolve.
fn compile_schema(schema: &Value) -> Result<jsonschema::Validator, SchemaRejection> {
    if !schema.is_object() {
        return Err(SchemaRejection::NotAnObject);
    }
    jsonschema::draft202012::options()
        .should_validate_formats(true)
        .should_ignore_unknown_formats(true)
        .build(schema)
        .map_err(|error| match error.kind {
            jsonschema::error::ValidationErrorKind::Referencing(_) => {
                SchemaRejection::ExternalReference
            }
            _ => SchemaRejection::Invalid,
        })
}
fn validated_schema(raw: &str) -> CoreResult<Value> {
    let schema = parse_json(raw)?;
    compile_schema(&schema).map_err(|_| CoreError::InvalidInput)?;
    Ok(schema)
}
fn mcp_destination_digest(config: &crate::model::core::ServerConfig) -> CoreResult<String> {
    let url = reqwest::Url::parse(&config.url).map_err(|_| CoreError::InvalidInput)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.query().is_some()
        || url.fragment().is_some()
        || (url.scheme() == "http" && !config.allow_http)
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(CoreError::InvalidInput);
    }
    let mut headers = Vec::new();
    let mut seen = HashSet::new();
    for (name, reference) in &config.secret_headers {
        let name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| CoreError::InvalidInput)?;
        if !seen.insert(name.clone()) {
            return Err(CoreError::InvalidInput);
        }
        headers.push((name.as_str().to_owned(), reference.0.to_string()));
    }
    headers.sort();
    value_digest(&serde_json::json!([
        config.id.0.to_string(),
        url.as_str(),
        config.allow_http,
        headers
    ]))
}
fn destination_on_connection(conn: &rusqlite::Connection, key: &ToolKey) -> CoreResult<String> {
    match key.server {
        ToolServer::LocalFiles => Ok(digest(b"local-files:v1")),
        ToolServer::Mcp(id) => {
            let payload: String = conn
                .query_row(
                    "SELECT payload FROM servers WHERE id=?1",
                    [id.0.to_string()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|_| CoreError::Storage)?
                .ok_or(CoreError::Configuration)?;
            let config: crate::model::core::ServerConfig =
                serde_json::from_str(&payload).map_err(|_| CoreError::Storage)?;
            mcp_destination_digest(&config)
        }
    }
}
fn observed_on_connection(
    conn: &rusqlite::Connection,
    key: &ToolKey,
) -> CoreResult<Option<String>> {
    let (profile, server, name) = key_parts(key);
    conn.query_row("SELECT definition_digest FROM observed_tool_definitions WHERE profile_id=?1 AND server_key=?2 AND tool_name=?3", params![profile, server, name], |r| r.get(0)).optional().map_err(|_| CoreError::Storage)
}
fn require_observed(conn: &rusqlite::Connection, key: &ToolKey, expected: &str) -> CoreResult<()> {
    if observed_on_connection(conn, key)?.as_deref() != Some(expected) {
        return Err(CoreError::Conflict);
    }
    Ok(())
}
fn destination(repo: &Repository, key: &ToolKey) -> CoreResult<String> {
    let (profile, _, _) = key_parts(key);
    let key = key.clone();
    repo.with_connection(move |conn| {
        let payload: String = conn
            .query_row(
                "SELECT payload FROM profiles WHERE id=?1",
                [&profile],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| CoreError::Storage)?
            .ok_or(CoreError::Configuration)?;
        let configured: crate::model::core::Profile =
            serde_json::from_str(&payload).map_err(|_| CoreError::Storage)?;
        if let ToolServer::Mcp(id) = key.server {
            if !configured.server_ids.contains(&id) && observed_on_connection(conn, &key)?.is_none()
            {
                return Err(CoreError::Configuration);
            }
        }
        destination_on_connection(conn, &key)
    })
}
impl ToolDefinition {
    pub fn from_discovery(repo: &Repository, key: ToolKey, schema_json: &str) -> CoreResult<Self> {
        if key.name.is_empty() {
            return Err(CoreError::InvalidInput);
        }
        let schema = validated_schema(schema_json)?;
        let schema_digest = value_digest(&schema)?;
        let destination_digest = destination(repo, &key)?;
        let definition_digest = digest(format!("{schema_digest}:{destination_digest}").as_bytes());
        let (profile, server, name) = key_parts(&key);
        let canonical = serde_json::to_string(&schema).map_err(|_| CoreError::InvalidInput)?;
        let candidate = Self {
            key: key.clone(),
            schema_json: canonical.clone(),
            schema_digest: schema_digest.clone(),
            destination_digest: destination_digest.clone(),
        };
        repo.with_connection(move |conn| {
            let transaction = conn.transaction().map_err(|_| CoreError::Storage)?;
            if destination_on_connection(&transaction, &key)? != destination_digest { return Err(CoreError::Conflict); }
            if let ToolServer::Mcp(id) = key.server {
                let linked:Option<String>=transaction.query_row("SELECT server_id FROM profile_servers WHERE profile_id=?1 AND server_id=?2",params![profile,id.0.to_string()],|r|r.get(0)).optional().map_err(|_|CoreError::Storage)?;
                if linked.is_none() && observed_on_connection(&transaction, &key)?.is_none() { return Err(CoreError::Configuration); }
            }
            transaction.execute("INSERT INTO observed_tool_definitions(profile_id,server_key,tool_name,schema_json,schema_digest,destination_digest,definition_digest) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(profile_id,server_key,tool_name) DO UPDATE SET schema_json=excluded.schema_json,schema_digest=excluded.schema_digest,destination_digest=excluded.destination_digest,definition_digest=excluded.definition_digest", params![profile,server,name,canonical,schema_digest,destination_digest,definition_digest]).map_err(|_| CoreError::Storage)?;
            transaction.commit().map_err(|_| CoreError::Storage)
        })?;
        Ok(candidate)
    }
}
/// Publish a complete, validated observation for one profile/server in one transaction.
/// Caller must finish every page before invoking this synchronous repository operation.
pub fn replace_mcp_discovery(
    repo: &Repository,
    profile_id: ProfileId,
    server_id: ServerId,
    observed_config: &crate::model::core::ServerConfig,
    tools: Vec<(ToolKey, String)>,
) -> CoreResult<Vec<ToolDefinition>> {
    replace_mcp_discovery_if_current(
        repo,
        profile_id,
        server_id,
        observed_config,
        tools,
        &std::sync::Arc::new(AtomicU64::new(0)),
        0,
    )
}

pub fn replace_mcp_discovery_if_current(
    repo: &Repository,
    profile_id: ProfileId,
    server_id: ServerId,
    observed_config: &crate::model::core::ServerConfig,
    tools: Vec<(ToolKey, String)>,
    generation: &std::sync::Arc<AtomicU64>,
    expected_generation: u64,
) -> CoreResult<Vec<ToolDefinition>> {
    let expected_destination = mcp_destination_digest(observed_config)?;
    let generation = generation.clone();
    let server = format!("mcp:{}", server_id.0);
    let profile = profile_id.0.to_string();
    let mut seen = HashSet::new();
    let mut prepared = Vec::with_capacity(tools.len());
    for (key, raw) in tools {
        if key.profile_id != profile_id
            || key.server != ToolServer::Mcp(server_id)
            || key.name.is_empty()
            || !seen.insert(key.name.clone())
        {
            return Err(CoreError::InvalidInput);
        }
        let schema = validated_schema(&raw)?;
        let schema_json = serde_json::to_string(&schema).map_err(|_| CoreError::InvalidInput)?;
        let schema_digest = value_digest(&schema)?;
        prepared.push((key, schema_json, schema_digest));
    }
    repo.with_connection(move |conn| {
        let tx = conn.transaction().map_err(|_| CoreError::Storage)?;
        let linked: Option<String> = tx.query_row(
            "SELECT server_id FROM profile_servers WHERE profile_id=?1 AND server_id=?2",
            params![profile, server_id.0.to_string()], |row| row.get(0),
        ).optional().map_err(|_| CoreError::Storage)?;
        if linked.is_none() { return Err(CoreError::Configuration); }
        let route = ToolKey { profile_id, server: ToolServer::Mcp(server_id), name: String::new() };
        if destination_on_connection(&tx, &route)? != expected_destination {
            return Err(CoreError::Conflict);
        }
        let mut definitions = Vec::with_capacity(prepared.len());
        let mut allowed = Vec::with_capacity(prepared.len());
        for (key, schema_json, schema_digest) in prepared {
            let destination_digest = destination_on_connection(&tx, &key)?;
            let definition_digest = digest(format!("{schema_digest}:{destination_digest}").as_bytes());
            allowed.push(key.name.clone());
            let (profile_key, server_key, name) = key_parts(&key);
            tx.execute("INSERT INTO observed_tool_definitions(profile_id,server_key,tool_name,schema_json,schema_digest,destination_digest,definition_digest) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(profile_id,server_key,tool_name) DO UPDATE SET schema_json=excluded.schema_json,schema_digest=excluded.schema_digest,destination_digest=excluded.destination_digest,definition_digest=excluded.definition_digest", params![profile_key,server_key,name,schema_json,schema_digest,destination_digest,definition_digest]).map_err(|_| CoreError::Storage)?;
            definitions.push(ToolDefinition { key, schema_json, schema_digest, destination_digest });
        }
        // Remove absent names only after all incoming observations have been accepted.
        let mut stmt = tx.prepare("SELECT tool_name FROM observed_tool_definitions WHERE profile_id=?1 AND server_key=?2").map_err(|_| CoreError::Storage)?;
        let existing = stmt.query_map(params![profile, server], |row| row.get::<_, String>(0)).map_err(|_| CoreError::Storage)?
            .collect::<Result<Vec<_>, _>>().map_err(|_| CoreError::Storage)?;
        drop(stmt);
        for name in existing {
            if !allowed.contains(&name) {
                tx.execute("DELETE FROM observed_tool_definitions WHERE profile_id=?1 AND server_key=?2 AND tool_name=?3", params![profile, server, name]).map_err(|_| CoreError::Storage)?;
            }
        }
        if generation.load(Ordering::Acquire) != expected_generation { return Err(CoreError::Conflict); }
        tx.commit().map_err(|_| CoreError::Storage)?;
        Ok(definitions)
    })
}

/// Check the live configured destination and linked profile on the SQLite worker.
pub fn require_mcp_destination(
    repo: &Repository,
    profile_id: ProfileId,
    server_id: ServerId,
    observed_config: &crate::model::core::ServerConfig,
) -> CoreResult<()> {
    let expected_destination = mcp_destination_digest(observed_config)?;
    repo.with_connection(move |conn| {
        let linked: Option<String> = conn
            .query_row(
                "SELECT server_id FROM profile_servers WHERE profile_id=?1 AND server_id=?2",
                params![profile_id.0.to_string(), server_id.0.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| CoreError::Storage)?;
        if linked.is_none() {
            return Err(CoreError::Conflict);
        }
        let key = ToolKey {
            profile_id,
            server: ToolServer::Mcp(server_id),
            name: String::new(),
        };
        if destination_on_connection(conn, &key)? != expected_destination {
            return Err(CoreError::Conflict);
        }
        Ok(())
    })
}

/// Revoke all observed identities immediately on known list changes or session loss.
pub fn invalidate_mcp_discovery(
    repo: &Repository,
    profile_id: ProfileId,
    server_id: ServerId,
) -> CoreResult<()> {
    repo.with_connection(move |conn| {
        conn.execute(
            "DELETE FROM observed_tool_definitions WHERE profile_id=?1 AND server_key=?2",
            params![profile_id.0.to_string(), format!("mcp:{}", server_id.0)],
        )
        .map_err(|_| CoreError::Storage)?;
        Ok(())
    })
}

fn trust(repo: &Repository, definition: &ToolDefinition) -> CoreResult<String> {
    let schema = validated_schema(&definition.schema_json)?;
    if value_digest(&schema)? != definition.schema_digest
        || destination(repo, &definition.key)? != definition.destination_digest
    {
        return Err(CoreError::Conflict);
    }
    Ok(digest(
        format!(
            "{}:{}",
            definition.schema_digest, definition.destination_digest
        )
        .as_bytes(),
    ))
}
pub fn set_rule(
    repo: &Repository,
    definition: &ToolDefinition,
    rule: PermissionRule,
) -> CoreResult<()> {
    let bound = trust(repo, definition)?;
    let (profile, server, name) = key_parts(&definition.key);
    let key = definition.key.clone();
    let destination_digest = definition.destination_digest.clone();
    repo.with_connection(move |conn| {
        let transaction = conn.transaction().map_err(|_| CoreError::Storage)?;
        require_observed(&transaction, &key, &bound)?;
        if destination_on_connection(&transaction, &key)? != destination_digest { return Err(CoreError::Conflict); }
        transaction.execute("INSERT INTO tool_permission_rules(profile_id,server_key,tool_name,rule,definition_digest) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(profile_id,server_key,tool_name) DO UPDATE SET rule=excluded.rule,definition_digest=excluded.definition_digest",params![profile,server,name,format!("{rule:?}"),bound]).map_err(|_|CoreError::Storage)?;
        transaction.commit().map_err(|_| CoreError::Storage)
    })
}
fn binding(
    attempt_id: AttemptId,
    call: &ToolCall,
    definition: &ToolDefinition,
    definition_digest: String,
    arguments: &Value,
) -> CoreResult<ApprovalBinding> {
    Ok(ApprovalBinding {
        attempt_id,
        key: call.key.clone(),
        provider_call_id: call.provider_call_id.clone(),
        definition_digest,
        arguments_digest: value_digest(arguments)?,
        arguments_json: serde_json::to_string(arguments).map_err(|_| CoreError::InvalidInput)?,
        schema_json: serde_json::to_string(&parse_json(&definition.schema_json)?)
            .map_err(|_| CoreError::InvalidInput)?,
        destination_digest: definition.destination_digest.clone(),
    })
}
pub(crate) fn validate_call(call: &ToolCall, definition: &ToolDefinition) -> CoreResult<()> {
    if call.key != definition.key || call.provider_call_id.is_empty() {
        return Err(CoreError::InvalidInput);
    }
    let args = parse_json(&call.arguments_json)?;
    if !args.is_object() {
        return Err(CoreError::InvalidInput);
    }
    let schema = parse_json(&definition.schema_json)?;
    if !compile_schema(&schema)
        .map_err(|_| CoreError::InvalidInput)?
        .is_valid(&args)
    {
        return Err(CoreError::InvalidInput);
    }
    Ok(())
}
/// The live policy for a call: a Block rule wins; an Allow rule bound to the
/// current definition, or a trusted MCP server, allows; anything else asks.
fn policy_on(
    conn: &rusqlite::Connection,
    key: &ToolKey,
    expected: &str,
    current_destination: &str,
) -> CoreResult<PermissionRule> {
    let (profile, server, name) = key_parts(key);
    let rule: Option<(String, String)> = conn
        .query_row(
            "SELECT rule,definition_digest FROM tool_permission_rules WHERE profile_id=?1 AND server_key=?2 AND tool_name=?3",
            params![profile, server, name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|_| CoreError::Storage)?;
    Ok(match rule {
        Some((value, _)) if value == "Block" => PermissionRule::Block,
        Some((value, bound)) if value == "Allow" && bound == expected => PermissionRule::Allow,
        _ => match key.server {
            ToolServer::Mcp(id) if trusted_on(conn, id, current_destination)? => {
                PermissionRule::Allow
            }
            _ => PermissionRule::Ask,
        },
    })
}
/// A server is trusted only while its stored digest matches its current destination.
fn trusted_on(
    conn: &rusqlite::Connection,
    server: ServerId,
    current_destination: &str,
) -> CoreResult<bool> {
    let stored: Option<String> = conn
        .query_row(
            "SELECT destination_digest FROM trusted_servers WHERE server_id=?1",
            [server.0.to_string()],
            |r| r.get(0),
        )
        .optional()
        .map_err(|_| CoreError::Storage)?;
    Ok(stored.as_deref() == Some(current_destination))
}
fn server_destination_on(conn: &rusqlite::Connection, server: ServerId) -> CoreResult<String> {
    destination_on_connection(
        conn,
        &ToolKey {
            profile_id: ProfileId(uuid::Uuid::nil()),
            server: ToolServer::Mcp(server),
            name: String::new(),
        },
    )
}
/// Trust (or stop trusting) every tool of one MCP server, app-wide, for its
/// current destination. Needs no network.
pub fn set_server_trust(repo: &Repository, server: ServerId, trusted: bool) -> CoreResult<()> {
    repo.with_connection(move |conn| {
        let tx = conn.transaction().map_err(|_| CoreError::Storage)?;
        let destination = server_destination_on(&tx, server)?;
        if trusted {
            tx.execute(
                "INSERT INTO trusted_servers(server_id,destination_digest) VALUES(?1,?2) ON CONFLICT(server_id) DO UPDATE SET destination_digest=excluded.destination_digest",
                params![server.0.to_string(), destination],
            )
        } else {
            tx.execute(
                "DELETE FROM trusted_servers WHERE server_id=?1",
                [server.0.to_string()],
            )
        }
        .map_err(|_| CoreError::Storage)?;
        tx.commit().map_err(|_| CoreError::Storage)
    })
}
/// Whether a saved server is currently trusted (its destination is unchanged).
pub fn server_trusted(repo: &Repository, server: ServerId) -> CoreResult<bool> {
    repo.with_connection(move |conn| server_trusted_on(conn, server))
}
pub(crate) fn server_trusted_on(conn: &rusqlite::Connection, server: ServerId) -> CoreResult<bool> {
    let destination = server_destination_on(conn, server)?;
    trusted_on(conn, server, &destination)
}
/// Store an approval binding for a pending call that will need a decision.
///
/// Only a `Proposed` attempt of a running run, whose live policy is Ask and that
/// has no binding yet, gets one. It never changes an attempt's state and never
/// dispatches; calls that cannot be bound now (Allow, Block, invalid or stale)
/// are left for the run loop, which reports them when it reaches them.
pub fn prepare_approval(
    repo: &Repository,
    attempt_id: AttemptId,
    call: &ToolCall,
    definition: &ToolDefinition,
) -> CoreResult<()> {
    fn skip(error: CoreError) -> CoreResult<()> {
        match error {
            CoreError::Storage => Err(CoreError::Storage),
            _ => Ok(()),
        }
    }
    if let Err(error) = validate_call(call, definition) {
        return skip(error);
    }
    let prepared = parse_json(&call.arguments_json).and_then(|args| {
        let expected = trust(repo, definition)?;
        let exact = binding(attempt_id, call, definition, expected.clone(), &args)?;
        Ok((expected, exact))
    });
    let (expected, exact) = match prepared {
        Ok(value) => value,
        Err(error) => return skip(error),
    };
    let call_key = call.key.clone();
    let destination_digest = definition.destination_digest.clone();
    let stored = repo.with_connection(move |conn| {
        let tx = conn.transaction().map_err(|_| CoreError::Storage)?;
        let context = attempt_context(&tx, attempt_id, &call_key)?;
        if context.run_state != "Running"
            || context.state != "Proposed"
            || context.stored.is_some()
            || context.current_destination != destination_digest
            || observed_on_connection(&tx, &call_key)?.as_deref() != Some(&expected)
            || policy_on(&tx, &call_key, &expected, &context.current_destination)?
                != PermissionRule::Ask
        {
            return Ok(());
        }
        tx.execute(
            "UPDATE attempts SET approval_binding=?1 WHERE id=?2 AND state='Proposed' AND approval_binding IS NULL",
            params![
                serde_json::to_string(&exact).map_err(|_| CoreError::Storage)?,
                attempt_id.0.to_string()
            ],
        )
        .map_err(|_| CoreError::Storage)?;
        tx.commit().map_err(|_| CoreError::Storage)
    });
    match stored {
        Ok(()) => Ok(()),
        Err(error) => skip(error),
    }
}
struct AttemptContext {
    state: String,
    stored: Option<String>,
    run_state: String,
    current_destination: String,
}
/// Load an attempt with its run, and check the call belongs to the run's
/// profile snapshot; returns the server's current destination digest.
fn attempt_context(
    conn: &rusqlite::Connection,
    attempt_id: AttemptId,
    call_key: &ToolKey,
) -> CoreResult<AttemptContext> {
    let (state,stored,run_profile,run_state):(String,Option<String>,String,String)=conn.query_row("SELECT a.state,a.approval_binding,s.payload,r.state FROM attempts a JOIN runs r ON r.id=a.run_id JOIN conversation_snapshots s ON s.conversation_id=r.conversation_id AND s.revision=r.snapshot_revision WHERE a.id=?1",[attempt_id.0.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(|_|CoreError::Storage)?.ok_or(CoreError::Conflict)?;
    let snapshot: crate::model::core::ProfileSnapshot =
        serde_json::from_str(&run_profile).map_err(|_| CoreError::Storage)?;
    if snapshot.profile_id != call_key.profile_id {
        return Err(CoreError::Conflict);
    }
    if let ToolServer::Mcp(id) = call_key.server {
        if !snapshot.servers.iter().any(|config| config.id == id) {
            return Err(CoreError::Conflict);
        }
    }
    Ok(AttemptContext {
        state,
        stored,
        run_state,
        current_destination: destination_on_connection(conn, call_key)?,
    })
}
pub fn authorize(
    repo: &Repository,
    attempt_id: AttemptId,
    call: &ToolCall,
    definition: &ToolDefinition,
    supplied: Option<&ApprovalBinding>,
) -> CoreResult<Authorization> {
    validate_call(call, definition)?;
    let args = parse_json(&call.arguments_json)?;
    let expected = trust(repo, definition)?;
    let exact = binding(attempt_id, call, definition, expected.clone(), &args)?;
    let supplied = supplied.cloned();
    let destination_digest = definition.destination_digest.clone();
    let call_key = call.key.clone();
    repo.with_connection(move |conn| {
        let transaction = conn.transaction().map_err(|_| CoreError::Storage)?;
        let AttemptContext {
            state,
            stored,
            run_state,
            current_destination,
        } = attempt_context(&transaction, attempt_id, &call_key)?;
        if run_state != "Running" {
            return Err(CoreError::Conflict);
        }
        let observed_current =
            observed_on_connection(&transaction, &call_key)?.as_deref() == Some(&expected);
        if current_destination != destination_digest {
            return Err(CoreError::Conflict);
        }
        if !matches!(state.as_str(), "Proposed" | "Approved") {
            return Err(CoreError::Conflict);
        }
        let policy = policy_on(&transaction, &call_key, &expected, &current_destination)?;
        if policy != PermissionRule::Block && !observed_current {
            return Err(CoreError::Conflict);
        }
        let old = stored
            .map(|raw| serde_json::from_str::<ApprovalBinding>(&raw))
            .transpose()
            .map_err(|_| CoreError::Storage)?;
        if state == "Approved" && old.is_none() {
            return Err(CoreError::Storage);
        }
        // The caller must hold the binding it last saw for this attempt, and
        // the call itself can never change; only its definition may.
        if policy != PermissionRule::Block
            && old.is_some()
            && (supplied != old || old.as_ref().is_some_and(|old| !same_call(old, &exact)))
        {
            return Err(CoreError::Conflict);
        }
        let dispatch = |transaction: &rusqlite::Transaction<'_>| {
            transaction
                .execute(
                    "UPDATE attempts SET state='Dispatching' WHERE id=?1",
                    [attempt_id.0.to_string()],
                )
                .map_err(|_| CoreError::Storage)
        };
        let result = if policy == PermissionRule::Block {
            Authorization::Block
        } else if state == "Approved" && old.as_ref() == Some(&exact) {
            dispatch(&transaction)?;
            Authorization::Allow
        } else if policy == PermissionRule::Allow {
            // Allow (rule or trust) wins over a binding stored while it asked.
            dispatch(&transaction)?;
            Authorization::Allow
        } else if state == "Proposed" && old.as_ref() == Some(&exact) {
            Authorization::Ask(Box::new(exact))
        } else {
            // No binding yet, or the definition changed since it was bound
            // (or approved): bind the current call and ask again.
            transaction
                .execute(
                    "UPDATE attempts SET state='Proposed',approval_binding=?1 WHERE id=?2",
                    params![
                        serde_json::to_string(&exact).map_err(|_| CoreError::Storage)?,
                        attempt_id.0.to_string()
                    ],
                )
                .map_err(|_| CoreError::Storage)?;
            Authorization::Ask(Box::new(exact))
        };
        transaction.commit().map_err(|_| CoreError::Storage)?;
        Ok(result)
    })
}
/// Whether two bindings are for the same call (attempt, tool, provider id and
/// arguments), whatever definition they were bound to.
fn same_call(a: &ApprovalBinding, b: &ApprovalBinding) -> bool {
    a.attempt_id == b.attempt_id
        && a.key == b.key
        && a.provider_call_id == b.provider_call_id
        && a.arguments_digest == b.arguments_digest
        && a.arguments_json == b.arguments_json
}
fn call_server(server: &str) -> Option<&str> {
    server.strip_prefix("mcp:")
}
fn supplied_schema_digest(binding: &ApprovalBinding) -> CoreResult<String> {
    value_digest(&validated_schema(&binding.schema_json)?)
}
pub fn approve(
    repo: &Repository,
    attempt_id: AttemptId,
    binding: &ApprovalBinding,
) -> CoreResult<()> {
    if binding.attempt_id != attempt_id {
        return Err(CoreError::Conflict);
    }
    let supplied = binding.clone();
    repo.with_connection(move |conn| {
        let transaction = conn.transaction().map_err(|_| CoreError::Storage)?;
        if approve_on(&transaction, &supplied)? {
            transaction.commit().map_err(|_| CoreError::Storage)?;
        }
        Ok(())
    })
}
/// Whether an attempt was already approved with exactly this binding.
pub fn already_approved(repo: &Repository, binding: &ApprovalBinding) -> CoreResult<bool> {
    let binding = binding.clone();
    repo.with_connection(move |conn| already_approved_on(conn, &binding))
}
/// Whether an attempt is already approved with exactly this binding.
pub(crate) fn already_approved_on(
    conn: &rusqlite::Connection,
    binding: &ApprovalBinding,
) -> CoreResult<bool> {
    let row: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT state,approval_binding FROM attempts WHERE id=?1",
            [binding.attempt_id.0.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|_| CoreError::Storage)?;
    // Once approved, the attempt may already be dispatching or finished.
    Ok(matches!(row, Some((state, Some(stored)))
        if matches!(state.as_str(), "Approved" | "Dispatching" | "Succeeded" | "Failed" | "OutcomeUnknown")
            && serde_json::from_str::<ApprovalBinding>(&stored).ok().as_ref() == Some(binding)))
}
/// Approve one attempt inside the caller's transaction. Returns whether it
/// changed anything: repeating an approval with the same binding is a no-op,
/// even after the approval's MCP session (and its observations) closed.
pub(crate) fn approve_on(
    transaction: &rusqlite::Connection,
    supplied: &ApprovalBinding,
) -> CoreResult<bool> {
    let attempt_id = supplied.attempt_id;
    if already_approved_on(transaction, supplied)? {
        return Ok(false);
    }
    {
        let (state,stored,profile,run_state):(String,Option<String>,String,String)=transaction.query_row("SELECT a.state,a.approval_binding,s.payload,r.state FROM attempts a JOIN runs r ON r.id=a.run_id JOIN conversation_snapshots s ON s.conversation_id=r.conversation_id AND s.revision=r.snapshot_revision WHERE a.id=?1",[attempt_id.0.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(|_|CoreError::Storage)?.ok_or(CoreError::Conflict)?;
        if !matches!(run_state.as_str(), "Running" | "WaitingApproval") {
            return Err(CoreError::Conflict);
        }
        require_observed(transaction, &supplied.key, &supplied.definition_digest)?;
        if state != "Proposed"
            || stored
                .as_deref()
                .and_then(|s| serde_json::from_str::<ApprovalBinding>(s).ok())
                .as_ref()
                != Some(supplied)
        {
            return Err(CoreError::Conflict);
        }
        let snapshot: crate::model::core::ProfileSnapshot =
            serde_json::from_str(&profile).map_err(|_| CoreError::Storage)?;
        let (profile_id, server, name) = key_parts(&supplied.key);
        if snapshot.profile_id.0.to_string() != profile_id {
            return Err(CoreError::Conflict);
        }
        if let ToolServer::Mcp(id) = supplied.key.server {
            if !snapshot.servers.iter().any(|config| config.id == id) {
                return Err(CoreError::Conflict);
            }
        }
        let current_destination = if let Some(id) = call_server(&server) {
            let payload: String = transaction
                .query_row("SELECT payload FROM servers WHERE id=?1", [id], |r| {
                    r.get(0)
                })
                .optional()
                .map_err(|_| CoreError::Storage)?
                .ok_or(CoreError::Configuration)?;
            let config: crate::model::core::ServerConfig =
                serde_json::from_str(&payload).map_err(|_| CoreError::Storage)?;
            mcp_destination_digest(&config)?
        } else {
            digest(b"local-files:v1")
        };
        if current_destination != supplied.destination_digest
            || value_digest(&parse_json(&supplied.schema_json)?)?
                != supplied_schema_digest(supplied)?
            || digest(
                format!(
                    "{}:{}",
                    supplied_schema_digest(supplied)?,
                    current_destination
                )
                .as_bytes(),
            ) != supplied.definition_digest
            || value_digest(&parse_json(&supplied.arguments_json)?)? != supplied.arguments_digest
        {
            return Err(CoreError::Conflict);
        }
        let rule:Option<String>=transaction.query_row("SELECT rule FROM tool_permission_rules WHERE profile_id=?1 AND server_key=?2 AND tool_name=?3",params![profile_id,server,name],|r|r.get(0)).optional().map_err(|_|CoreError::Storage)?;
        if rule.as_deref() == Some("Block") {
            return Err(CoreError::Conflict);
        }
        transaction
            .execute(
                "UPDATE attempts SET state='Approved' WHERE id=?1",
                [attempt_id.0.to_string()],
            )
            .map_err(|_| CoreError::Storage)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CoreError;
    use crate::model::core::*;
    use crate::storage::Repository;
    use tempfile::tempdir;

    const SCHEMA: &str = r#"{"type":"object","properties":{"path":{"type":"string"},"count":{"type":"integer"}},"required":["path"],"additionalProperties":false}"#;
    struct Fixture {
        repo: Repository,
        _dir: tempfile::TempDir,
        profile: ProfileId,
        server: ServerId,
        attempt: AttemptId,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = tempdir().unwrap();
            let repo = Repository::open(&dir.path().join("core.db")).unwrap();
            let endpoint = EndpointId::new();
            repo.save_endpoint(EndpointConfig {
                id: endpoint,
                name: "model".into(),
                base_url: "https://model.test".into(),
                model: "model".into(),
                context_tokens: 100,
                max_output_tokens: 10,
                temperature: 0.1,
                allow_http: false,
                credential: None,
            })
            .unwrap();
            let server = ServerId::new();
            repo.save_server(ServerConfig {
                id: server,
                name: "server".into(),
                url: "https://server.test/mcp".into(),
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
            let run = RunId::new();
            repo.insert_run_for_test(run, conversation, RunState::Running)
                .unwrap();
            let attempt = AttemptId::new();
            repo.insert_attempt_for_test(attempt, run, AttemptState::Proposed)
                .unwrap();
            Self {
                repo,
                _dir: dir,
                profile,
                server,
                attempt,
            }
        }
        fn definition(&self) -> ToolDefinition {
            ToolDefinition::from_discovery(
                &self.repo,
                ToolKey {
                    profile_id: self.profile,
                    server: ToolServer::Mcp(self.server),
                    name: "read".into(),
                },
                SCHEMA,
            )
            .unwrap()
        }
        fn call(&self) -> ToolCall {
            ToolCall {
                key: ToolKey {
                    profile_id: self.profile,
                    server: ToolServer::Mcp(self.server),
                    name: "read".into(),
                },
                provider_call_id: "call-1".into(),
                arguments_json: r#"{"path":"a"}"#.into(),
            }
        }
        fn next_attempt(&mut self) {
            let next = AttemptId::new();
            self.repo
                .insert_attempt_for_test(next, self.run(), AttemptState::Proposed)
                .unwrap();
            self.attempt = next;
        }
        fn run(&self) -> RunId {
            self.repo.run_for_attempt_for_test(self.attempt).unwrap()
        }
    }
    fn dispatch(counter: &mut usize, decision: Authorization) {
        if decision == Authorization::Allow {
            *counter += 1;
        }
    }
    fn ask(value: Authorization) -> ApprovalBinding {
        match value {
            Authorization::Ask(binding) => *binding,
            other => panic!("expected Ask, got {other:?}"),
        }
    }

    #[test]
    fn mcp_batch_replaces_removed_and_changed_observations_atomically() {
        let mut f = Fixture::new();
        let old = f.definition();
        set_rule(&f.repo, &old, PermissionRule::Allow).unwrap();
        let other = ToolKey {
            name: "other".into(),
            ..old.key.clone()
        };
        replace_mcp_discovery(
            &f.repo,
            f.profile,
            f.server,
            &ServerConfig {
                id: f.server,
                name: "server".into(),
                url: "https://server.test/mcp".into(),
                allow_http: false,
                secret_headers: vec![],
            },
            vec![(other.clone(), SCHEMA.into())],
        )
        .unwrap();
        f.next_attempt();
        assert!(authorize(&f.repo, f.attempt, &f.call(), &old, None).is_err());
        let new_def = ToolDefinition::from_discovery(&f.repo, other, SCHEMA).unwrap();
        assert!(matches!(
            authorize(
                &f.repo,
                f.attempt,
                &ToolCall {
                    key: new_def.key.clone(),
                    ..f.call()
                },
                &new_def,
                None
            )
            .unwrap(),
            Authorization::Ask(_)
        ));
        invalidate_mcp_discovery(&f.repo, f.profile, f.server).unwrap();
        assert!(authorize(
            &f.repo,
            f.attempt,
            &ToolCall {
                key: new_def.key.clone(),
                ..f.call()
            },
            &new_def,
            None
        )
        .is_err());
    }
    #[test]
    fn mcp_batch_rejects_observations_from_old_url() {
        let f = Fixture::new();
        let key = ToolKey {
            profile_id: f.profile,
            server: ToolServer::Mcp(f.server),
            name: "alpha".into(),
        };
        let result = replace_mcp_discovery(
            &f.repo,
            f.profile,
            f.server,
            &ServerConfig {
                id: f.server,
                name: "server".into(),
                url: "http://old.test/mcp".into(),
                allow_http: true,
                secret_headers: vec![],
            },
            vec![(key.clone(), SCHEMA.into())],
        );
        assert!(matches!(result, Err(CoreError::Conflict)));
        assert!(f
            .repo
            .with_connection(move |conn| observed_on_connection(conn, &key))
            .unwrap()
            .is_none());
    }
    #[test]
    fn mcp_batch_invalid_schema_keeps_previous_catalog() {
        let f = Fixture::new();
        let old = f.definition();
        assert!(replace_mcp_discovery(
            &f.repo,
            f.profile,
            f.server,
            &ServerConfig {
                id: f.server,
                name: "server".into(),
                url: "https://server.test/mcp".into(),
                allow_http: false,
                secret_headers: vec![]
            },
            vec![
                (old.key.clone(), SCHEMA.into()),
                (
                    ToolKey {
                        name: "invalid".into(),
                        ..old.key.clone()
                    },
                    r#"{"$ref":"https://example.test/schema"}"#.into()
                ),
            ]
        )
        .is_err());
        let found = f
            .repo
            .with_connection({
                let key = old.key.clone();
                move |conn| observed_on_connection(conn, &key)
            })
            .unwrap();
        assert!(found.is_some());
    }
    #[test]
    fn new_tool_asks() {
        let f = Fixture::new();
        let b = ask(authorize(&f.repo, f.attempt, &f.call(), &f.definition(), None).unwrap());
        assert_eq!(b.attempt_id, f.attempt);
    }
    #[test]
    fn block_wins_at_dispatch() {
        let f = Fixture::new();
        let d = f.definition();
        set_rule(&f.repo, &d, PermissionRule::Allow).unwrap();
        set_rule(&f.repo, &d, PermissionRule::Block).unwrap();
        let mut n = 0;
        dispatch(
            &mut n,
            authorize(&f.repo, f.attempt, &f.call(), &d, None).unwrap(),
        );
        assert_eq!(n, 0);
    }
    #[test]
    fn approval_cannot_change_arguments() {
        let f = Fixture::new();
        let d = f.definition();
        let b = ask(authorize(&f.repo, f.attempt, &f.call(), &d, None).unwrap());
        approve(&f.repo, f.attempt, &b).unwrap();
        let mut changed = f.call();
        changed.arguments_json = r#"{"path":"b"}"#.into();
        let mut n = 0;
        assert!(authorize(&f.repo, f.attempt, &changed, &d, Some(&b)).is_err());
        dispatch(
            &mut n,
            authorize(&f.repo, f.attempt, &f.call(), &d, Some(&b)).unwrap(),
        );
        assert_eq!(n, 1);
        assert!(authorize(&f.repo, f.attempt, &f.call(), &d, Some(&b)).is_err());
    }
    #[test]
    fn schema_change_revokes_allow() {
        let mut f = Fixture::new();
        let d = f.definition();
        set_rule(&f.repo, &d, PermissionRule::Allow).unwrap();
        let changed = ToolDefinition::from_discovery(
            &f.repo,
            d.key.clone(),
            r#"{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}"#,
        )
        .unwrap();
        f.next_attempt();
        let mut counter = 0;
        if let Ok(decision) = authorize(&f.repo, f.attempt, &f.call(), &changed, None) {
            dispatch(&mut counter, decision);
        }
        assert_eq!(counter, 0);
        assert!(authorize(&f.repo, f.attempt, &f.call(), &d, None).is_err());
    }
    #[test]
    fn destination_change_revokes_allow() {
        let mut f = Fixture::new();
        let d = f.definition();
        set_rule(&f.repo, &d, PermissionRule::Allow).unwrap();
        f.repo
            .save_server(ServerConfig {
                id: f.server,
                name: "server".into(),
                url: "https://other.test/mcp".into(),
                allow_http: false,
                secret_headers: vec![],
            })
            .unwrap();
        f.next_attempt();
        let changed = ToolDefinition::from_discovery(&f.repo, d.key.clone(), SCHEMA).unwrap();
        assert!(matches!(
            authorize(&f.repo, f.attempt, &f.call(), &changed, None).unwrap(),
            Authorization::Ask(_)
        ));
        assert!(authorize(&f.repo, f.attempt, &f.call(), &d, None).is_err());
    }
    #[test]
    fn pending_approval_rechecks_live_policy() {
        let f = Fixture::new();
        let d = f.definition();
        let b = ask(authorize(&f.repo, f.attempt, &f.call(), &d, None).unwrap());
        set_rule(&f.repo, &d, PermissionRule::Block).unwrap();
        assert!(approve(&f.repo, f.attempt, &b).is_err());
        let mut n = 0;
        dispatch(
            &mut n,
            authorize(&f.repo, f.attempt, &f.call(), &d, Some(&b)).unwrap(),
        );
        assert_eq!(n, 0);
    }
    #[test]
    fn server_name_collision_cannot_route_to_other_server() {
        let mut f = Fixture::new();
        let d = f.definition();
        set_rule(&f.repo, &d, PermissionRule::Allow).unwrap();
        let other = ServerId::new();
        f.repo
            .save_server(ServerConfig {
                id: other,
                name: "server".into(),
                url: "https://server.test/mcp".into(),
                allow_http: false,
                secret_headers: vec![],
            })
            .unwrap();
        let endpoint = EndpointId::new();
        f.repo
            .save_endpoint(EndpointConfig {
                id: endpoint,
                name: "model".into(),
                base_url: "https://model.test".into(),
                model: "model".into(),
                context_tokens: 100,
                max_output_tokens: 10,
                temperature: 0.1,
                allow_http: false,
                credential: None,
            })
            .unwrap();
        f.repo
            .save_profile(Profile {
                mode: ProfileMode::Agent,
                id: f.profile,
                name: "profile".into(),
                endpoint_id: endpoint,
                server_ids: vec![f.server, other],
                instructions: String::new(),
                limits: RunLimits::default(),
            })
            .unwrap();
        f.next_attempt();
        let other_def = ToolDefinition::from_discovery(
            &f.repo,
            ToolKey {
                profile_id: f.profile,
                server: ToolServer::Mcp(other),
                name: "read".into(),
            },
            SCHEMA,
        )
        .unwrap();
        let mut call = f.call();
        call.key = other_def.key.clone();
        assert!(authorize(&f.repo, f.attempt, &call, &other_def, None).is_err());
        assert!(authorize(&f.repo, f.attempt, &call, &d, None).is_err());
    }
    #[test]
    fn local_tools_follow_same_policy() {
        let f = Fixture::new();
        let d = ToolDefinition::from_discovery(
            &f.repo,
            ToolKey {
                profile_id: f.profile,
                server: ToolServer::LocalFiles,
                name: "read".into(),
            },
            SCHEMA,
        )
        .unwrap();
        let call = ToolCall {
            key: d.key.clone(),
            ..f.call()
        };
        assert!(matches!(
            authorize(&f.repo, f.attempt, &call, &d, None).unwrap(),
            Authorization::Ask(_)
        ));
        set_rule(&f.repo, &d, PermissionRule::Block).unwrap();
        assert_eq!(
            authorize(&f.repo, f.attempt, &call, &d, None).unwrap(),
            Authorization::Block
        );
    }
    #[test]
    fn strict_json_and_schema_validation_precede_decision() {
        let mut f = Fixture::new();
        let d = f.definition();
        set_rule(&f.repo, &d, PermissionRule::Allow).unwrap();
        for raw in [
            r#"{"path":"a","path":"b"}"#,
            r#"{"path":"a","nested":{"x":1,"x":2}}"#,
            "{",
            r#"{"path":3}"#,
            r#"{"path":"a","count":"bad"}"#,
        ] {
            let mut call = f.call();
            call.arguments_json = raw.into();
            assert_eq!(
                authorize(&f.repo, f.attempt, &call, &d, None),
                Err(CoreError::InvalidInput)
            );
        }
        let mut n = 0;
        dispatch(
            &mut n,
            authorize(&f.repo, f.attempt, &f.call(), &d, None).unwrap(),
        );
        assert_eq!(n, 1);
        f.next_attempt();
        let mut wrong = f.call();
        wrong.key.name = "other".into();
        assert!(authorize(&f.repo, f.attempt, &wrong, &d, None).is_err());
    }
    #[test]
    fn object_key_order_preserves_approval_but_array_order_does_not() {
        let f = Fixture::new();
        let d=ToolDefinition::from_discovery(&f.repo,f.definition().key,r#"{"type":"object","properties":{"a":{"type":"integer"},"b":{"type":"array"}},"required":["a","b"]}"#).unwrap();
        let mut c = f.call();
        c.arguments_json = r#"{"a":1,"b":[1,2]}"#.into();
        let b = ask(authorize(&f.repo, f.attempt, &c, &d, None).unwrap());
        approve(&f.repo, f.attempt, &b).unwrap();
        c.arguments_json = r#"{ "b": [1,2], "a": 1 }"#.into();
        assert_eq!(
            authorize(&f.repo, f.attempt, &c, &d, Some(&b)).unwrap(),
            Authorization::Allow
        );
    }
    #[test]
    fn allow_to_block_before_claim_and_once_only_auto_allow() {
        let mut f = Fixture::new();
        let d = f.definition();
        set_rule(&f.repo, &d, PermissionRule::Allow).unwrap();
        set_rule(&f.repo, &d, PermissionRule::Block).unwrap();
        let mut n = 0;
        dispatch(
            &mut n,
            authorize(&f.repo, f.attempt, &f.call(), &d, None).unwrap(),
        );
        assert_eq!(n, 0);
        f.next_attempt();
        set_rule(&f.repo, &d, PermissionRule::Allow).unwrap();
        dispatch(
            &mut n,
            authorize(&f.repo, f.attempt, &f.call(), &d, None).unwrap(),
        );
        assert_eq!(n, 1);
        assert!(authorize(&f.repo, f.attempt, &f.call(), &d, None).is_err());
    }
    #[test]
    fn schema_and_destination_changes_revoke_pending_approval() {
        let f = Fixture::new();
        let d = f.definition();
        let b = ask(authorize(&f.repo, f.attempt, &f.call(), &d, None).unwrap());
        let changed =
            ToolDefinition::from_discovery(&f.repo, d.key.clone(), r#"{"type":"object"}"#).unwrap();
        // The old binding is replaced: the call asks again for the new definition.
        let fresh = ask(authorize(&f.repo, f.attempt, &f.call(), &changed, Some(&b)).unwrap());
        assert_ne!(fresh, b);
        assert!(approve(&f.repo, f.attempt, &b).is_err());
        assert!(authorize(&f.repo, f.attempt, &f.call(), &changed, Some(&b)).is_err());
    }
    #[test]
    fn block_survives_schema_change() {
        let f = Fixture::new();
        let d = f.definition();
        set_rule(&f.repo, &d, PermissionRule::Block).unwrap();
        let changed =
            ToolDefinition::from_discovery(&f.repo, d.key.clone(), r#"{"type":"object"}"#).unwrap();
        assert_eq!(
            authorize(&f.repo, f.attempt, &f.call(), &changed, None).unwrap(),
            Authorization::Block
        );
    }
    #[test]
    fn persisted_rule_and_pending_approval_survive_reopen() {
        let f = Fixture::new();
        let d = f.definition();
        let b = ask(authorize(&f.repo, f.attempt, &f.call(), &d, None).unwrap());
        let reopened = Repository::open(&f._dir.path().join("core.db")).unwrap();
        approve(&reopened, f.attempt, &b).unwrap();
        assert_eq!(
            authorize(&reopened, f.attempt, &f.call(), &d, Some(&b)).unwrap(),
            Authorization::Allow
        );
    }
    #[test]
    fn approval_binds_provider_call_id_and_blocks_destination_mutation() {
        let f = Fixture::new();
        let d = f.definition();
        let b = ask(authorize(&f.repo, f.attempt, &f.call(), &d, None).unwrap());
        approve(&f.repo, f.attempt, &b).unwrap();
        let mut call = f.call();
        call.provider_call_id = "other".into();
        assert!(authorize(&f.repo, f.attempt, &call, &d, Some(&b)).is_err());
        f.repo
            .save_server(ServerConfig {
                id: f.server,
                name: "server".into(),
                url: "https://changed.test/mcp".into(),
                allow_http: false,
                secret_headers: vec![],
            })
            .unwrap();
        assert!(authorize(&f.repo, f.attempt, &f.call(), &d, Some(&b)).is_err());
    }
    #[test]
    fn nested_object_keys_canonicalize_but_array_order_is_distinct() {
        let f = Fixture::new();
        let d = ToolDefinition::from_discovery(
            &f.repo,
            f.definition().key,
            r#"{"type":"object","properties":{"data":{"type":"array","items":{"type":"object"}}}}"#,
        )
        .unwrap();
        let mut call = f.call();
        call.arguments_json = r#"{"data":[{"a":1,"b":2},{"a":3}]}"#.into();
        let b = ask(authorize(&f.repo, f.attempt, &call, &d, None).unwrap());
        approve(&f.repo, f.attempt, &b).unwrap();
        call.arguments_json = r#"{"data":[{"b":2,"a":1},{"a":3}]}"#.into();
        assert_eq!(
            authorize(&f.repo, f.attempt, &call, &d, Some(&b)).unwrap(),
            Authorization::Allow
        );
    }
    #[test]
    fn recursive_duplicate_schema_keys_and_limits_are_rejected() {
        let f = Fixture::new();
        assert_eq!(
            ToolDefinition::from_discovery(
                &f.repo,
                f.definition().key,
                r#"{"properties":{"a":{"type":"string","type":"integer"}}}"#
            ),
            Err(CoreError::InvalidInput)
        );
        let d = f.definition();
        let mut call = f.call();
        call.arguments_json = " ".repeat(MAX_TOOL_JSON_BYTES + 1);
        assert_eq!(
            authorize(&f.repo, f.attempt, &call, &d, None),
            Err(CoreError::Limit)
        );
    }
    #[test]
    fn observed_schema_change_revokes_old_pending_approval() {
        let f = Fixture::new();
        let old = f.definition();
        let binding = ask(authorize(&f.repo, f.attempt, &f.call(), &old, None).unwrap());
        let _new = ToolDefinition::from_discovery(&f.repo, old.key.clone(), r#"{"type":"object"}"#)
            .unwrap();
        let mut calls = 0;
        assert!(approve(&f.repo, f.attempt, &binding).is_err());
        if let Ok(decision) = authorize(&f.repo, f.attempt, &f.call(), &old, Some(&binding)) {
            dispatch(&mut calls, decision);
        }
        assert_eq!(calls, 0);
    }
    #[test]
    fn observed_change_remains_revoking_after_reopen() {
        let mut f = Fixture::new();
        let old = f.definition();
        set_rule(&f.repo, &old, PermissionRule::Allow).unwrap();
        let changed =
            ToolDefinition::from_discovery(&f.repo, old.key.clone(), r#"{"type":"object"}"#)
                .unwrap();
        f.next_attempt();
        let reopened = Repository::open(&f._dir.path().join("core.db")).unwrap();
        assert!(authorize(&reopened, f.attempt, &f.call(), &old, None).is_err());
        assert!(matches!(
            authorize(&reopened, f.attempt, &f.call(), &changed, None),
            Ok(Authorization::Ask(_))
        ));
    }
    #[test]
    fn repeated_identical_discovery_keeps_allow_but_changed_observation_revokes_it() {
        let mut f = Fixture::new();
        let definition = f.definition();
        set_rule(&f.repo, &definition, PermissionRule::Allow).unwrap();
        let same = ToolDefinition::from_discovery(
            &f.repo,
            definition.key.clone(),
            &definition.schema_json,
        )
        .unwrap();
        f.next_attempt();
        assert_eq!(
            authorize(&f.repo, f.attempt, &f.call(), &same, None).unwrap(),
            Authorization::Allow
        );
        f.next_attempt();
        let changed =
            ToolDefinition::from_discovery(&f.repo, definition.key.clone(), r#"{"type":"object"}"#)
                .unwrap();
        assert!(authorize(&f.repo, f.attempt, &f.call(), &definition, None).is_err());
        assert!(matches!(
            authorize(&f.repo, f.attempt, &f.call(), &changed, None),
            Ok(Authorization::Ask(_))
        ));
    }
    #[test]
    fn destination_change_during_registration_does_not_install_stale_definition() {
        let f = Fixture::new();
        let old = f.definition();
        f.repo
            .save_server(ServerConfig {
                id: f.server,
                name: "server".into(),
                url: "https://changed.test/mcp".into(),
                allow_http: false,
                secret_headers: vec![],
            })
            .unwrap();
        assert!(set_rule(&f.repo, &old, PermissionRule::Allow).is_err());
    }
    #[test]
    fn stale_allow_rule_cannot_be_rebound_from_old_definition() {
        let f = Fixture::new();
        let old = f.definition();
        set_rule(&f.repo, &old, PermissionRule::Allow).unwrap();
        let _new = ToolDefinition::from_discovery(&f.repo, old.key.clone(), r#"{"type":"object"}"#)
            .unwrap();
        assert!(set_rule(&f.repo, &old, PermissionRule::Allow).is_err());
    }
    #[test]
    fn forged_same_key_permissive_schema_cannot_consume_rule() {
        let mut f = Fixture::new();
        let observed = f.definition();
        set_rule(&f.repo, &observed, PermissionRule::Allow).unwrap();
        let schema = validated_schema(r#"{"type":"object"}"#).unwrap();
        let forged = ToolDefinition {
            key: observed.key.clone(),
            schema_json: r#"{"type":"object"}"#.into(),
            schema_digest: value_digest(&schema).unwrap(),
            destination_digest: observed.destination_digest.clone(),
        };
        f.next_attempt();
        let mut calls = 0;
        if let Ok(decision) = authorize(&f.repo, f.attempt, &f.call(), &forged, None) {
            dispatch(&mut calls, decision);
        }
        assert_eq!(calls, 0);
        assert_eq!(
            authorize(&f.repo, f.attempt, &f.call(), &observed, None).unwrap(),
            Authorization::Allow
        );
    }
    #[test]
    fn old_run_cannot_gain_server_added_to_profile() {
        let mut f = Fixture::new();
        let other = ServerId::new();
        f.repo
            .save_server(ServerConfig {
                id: other,
                name: "other".into(),
                url: "https://other.test/mcp".into(),
                allow_http: false,
                secret_headers: vec![],
            })
            .unwrap();
        let updated = Profile {
            mode: crate::model::core::ProfileMode::Agent,
            id: f.profile,
            name: "profile".into(),
            endpoint_id: f
                .repo
                .conversation_for_run_for_test(f.run())
                .unwrap()
                .profile
                .endpoint
                .id,
            server_ids: vec![f.server, other],
            instructions: String::new(),
            limits: RunLimits::default(),
        };
        f.repo.save_profile(updated).unwrap();
        let definition = ToolDefinition::from_discovery(
            &f.repo,
            ToolKey {
                profile_id: f.profile,
                server: ToolServer::Mcp(other),
                name: "read".into(),
            },
            SCHEMA,
        )
        .unwrap();
        let mut call = f.call();
        call.key = definition.key.clone();
        f.next_attempt();
        assert!(authorize(&f.repo, f.attempt, &call, &definition, None).is_err());
    }
    #[test]
    fn removed_profile_server_still_allows_old_snapshot_approval() {
        let f = Fixture::new();
        let definition = f.definition();
        let call = f.call();
        let binding = ask(authorize(&f.repo, f.attempt, &call, &definition, None).unwrap());
        let profile = Profile {
            mode: crate::model::core::ProfileMode::Agent,
            id: f.profile,
            name: "profile".into(),
            endpoint_id: f
                .repo
                .conversation_for_run_for_test(f.run())
                .unwrap()
                .profile
                .endpoint
                .id,
            server_ids: vec![],
            instructions: String::new(),
            limits: RunLimits::default(),
        };
        f.repo.save_profile(profile).unwrap();
        approve(&f.repo, f.attempt, &binding).unwrap();
        let mut calls = 0;
        dispatch(
            &mut calls,
            authorize(&f.repo, f.attempt, &call, &definition, Some(&binding)).unwrap(),
        );
        assert_eq!(calls, 1);
    }
    #[test]
    fn old_run_retains_removed_snapshot_server() {
        let f = Fixture::new();
        let definition = f.definition();
        let call = f.call();
        let profile = Profile {
            mode: crate::model::core::ProfileMode::Agent,
            id: f.profile,
            name: "profile".into(),
            endpoint_id: f
                .repo
                .conversation_for_run_for_test(f.run())
                .unwrap()
                .profile
                .endpoint
                .id,
            server_ids: vec![],
            instructions: String::new(),
            limits: RunLimits::default(),
        };
        f.repo.save_profile(profile).unwrap();
        assert!(matches!(
            authorize(&f.repo, f.attempt, &call, &definition, None),
            Ok(Authorization::Ask(_))
        ));
    }
    #[test]
    fn interrupted_run_cannot_dispatch() {
        let f = Fixture::new();
        let definition = f.definition();
        set_rule(&f.repo, &definition, PermissionRule::Allow).unwrap();
        f.repo.recover_interrupted().unwrap();
        let mut calls = 0;
        if let Ok(decision) = authorize(&f.repo, f.attempt, &f.call(), &definition, None) {
            dispatch(&mut calls, decision);
        }
        assert_eq!(calls, 0);
    }
    #[test]
    fn uri_format_asserts_and_unknown_format_is_an_annotation() {
        let f = Fixture::new();
        let schema = r#"{"type":"object","properties":{"path":{"type":"string","format":"uri"}},"required":["path"]}"#;
        let definition =
            ToolDefinition::from_discovery(&f.repo, f.definition().key, schema).unwrap();
        let mut call = f.call();
        call.arguments_json = r#"{"path":"not a URI"}"#.into();
        assert_eq!(
            authorize(&f.repo, f.attempt, &call, &definition, None),
            Err(CoreError::InvalidInput)
        );
        let unknown = ToolDefinition::from_discovery(
            &f.repo,
            definition.key.clone(),
            r#"{"type":"object","properties":{"n":{"type":"integer","format":"uint32","minimum":0}}}"#,
        )
        .unwrap();
        call.arguments_json = r#"{"n":-1}"#.into();
        assert_eq!(
            authorize(&f.repo, f.attempt, &call, &unknown, None),
            Err(CoreError::InvalidInput),
            "the bounds next to an unknown format still apply"
        );
    }
    #[test]
    fn observed_schema_rejects_bad_asserted_uri_before_rule() {
        let f = Fixture::new();
        let definition=ToolDefinition::from_discovery(&f.repo,f.definition().key,r#"{"type":"object","properties":{"path":{"type":"string","format":"uri"}},"required":["path"]}"#).unwrap();
        set_rule(&f.repo, &definition, PermissionRule::Allow).unwrap();
        let mut call = f.call();
        call.arguments_json = r#"{"path":"bad uri"}"#.into();
        assert_eq!(
            authorize(&f.repo, f.attempt, &call, &definition, None),
            Err(CoreError::InvalidInput)
        );
    }
    #[test]
    fn tool_arguments_must_be_objects_even_with_empty_schema() {
        let f = Fixture::new();
        let definition = ToolDefinition::from_discovery(&f.repo, f.definition().key, "{}").unwrap();
        for arguments in ["null", "[]", "42", "true"] {
            let mut call = f.call();
            call.arguments_json = arguments.into();
            assert_eq!(
                authorize(&f.repo, f.attempt, &call, &definition, None),
                Err(CoreError::InvalidInput)
            );
        }
        let mut call = f.call();
        call.arguments_json = "{}".into();
        assert!(matches!(
            authorize(&f.repo, f.attempt, &call, &definition, None),
            Ok(Authorization::Ask(_))
        ));
    }
    #[test]
    fn concurrent_approved_claimants_get_one_usable_allow() {
        let f = Fixture::new();
        let definition = f.definition();
        let binding = ask(authorize(&f.repo, f.attempt, &f.call(), &definition, None).unwrap());
        approve(&f.repo, f.attempt, &binding).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut workers = vec![];
        for _ in 0..2 {
            let (repo, barrier, call, definition, binding) = (
                f.repo.clone(),
                barrier.clone(),
                f.call(),
                definition.clone(),
                binding.clone(),
            );
            let attempt = f.attempt;
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                authorize(&repo, attempt, &call, &definition, Some(&binding))
            }));
        }
        barrier.wait();
        let mut calls = 0;
        for worker in workers {
            if let Ok(decision) = worker.join().unwrap() {
                dispatch(&mut calls, decision);
            }
        }
        assert_eq!(calls, 1);
    }
    #[test]
    fn concurrent_claimants_get_one_usable_allow() {
        let f = Fixture::new();
        let definition = f.definition();
        set_rule(&f.repo, &definition, PermissionRule::Allow).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut workers = vec![];
        for _ in 0..2 {
            let (repo, barrier, call, definition) = (
                f.repo.clone(),
                barrier.clone(),
                f.call(),
                definition.clone(),
            );
            let attempt = f.attempt;
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                authorize(&repo, attempt, &call, &definition, None)
            }));
        }
        barrier.wait();
        let mut calls = 0;
        for worker in workers {
            if let Ok(decision) = worker.join().unwrap() {
                dispatch(&mut calls, decision);
            }
        }
        assert_eq!(calls, 1);
    }
    #[test]
    fn approved_attempt_after_reopen_can_claim_once_only() {
        let f = Fixture::new();
        let definition = f.definition();
        let binding = ask(authorize(&f.repo, f.attempt, &f.call(), &definition, None).unwrap());
        approve(&f.repo, f.attempt, &binding).unwrap();
        let reopened = Repository::open(&f._dir.path().join("core.db")).unwrap();
        let mut calls = 0;
        dispatch(
            &mut calls,
            authorize(&reopened, f.attempt, &f.call(), &definition, Some(&binding)).unwrap(),
        );
        assert!(authorize(&reopened, f.attempt, &f.call(), &definition, Some(&binding)).is_err());
        assert_eq!(calls, 1);
    }
    #[test]
    fn stale_approval_after_reopen_and_recovery_has_zero_dispatch() {
        let f = Fixture::new();
        let definition = f.definition();
        let binding = ask(authorize(&f.repo, f.attempt, &f.call(), &definition, None).unwrap());
        let reopened = Repository::open(&f._dir.path().join("core.db")).unwrap();
        reopened.recover_interrupted().unwrap();
        let mut calls = 0;
        if approve(&reopened, f.attempt, &binding).is_ok() {
            if let Ok(decision) =
                authorize(&reopened, f.attempt, &f.call(), &definition, Some(&binding))
            {
                dispatch(&mut calls, decision);
            }
        }
        assert_eq!(calls, 0);
    }
    #[test]
    fn array_reorder_cannot_use_approved_attempt() {
        let f = Fixture::new();
        let definition = ToolDefinition::from_discovery(
            &f.repo,
            f.definition().key,
            r#"{"type":"object","properties":{"items":{"type":"array"}}}"#,
        )
        .unwrap();
        let mut call = f.call();
        call.arguments_json = r#"{"items":[1,2]}"#.into();
        let binding = ask(authorize(&f.repo, f.attempt, &call, &definition, None).unwrap());
        approve(&f.repo, f.attempt, &binding).unwrap();
        call.arguments_json = r#"{"items":[2,1]}"#.into();
        let mut calls = 0;
        if let Ok(decision) = authorize(&f.repo, f.attempt, &call, &definition, Some(&binding)) {
            dispatch(&mut calls, decision);
        }
        assert_eq!(calls, 0);
    }
    /// Regression: schemas as rmcp/schemars emit them (`$schema`, local `$defs`
    /// references, integer `format`s, nullable type arrays) were all rejected
    /// by a keyword allow-list, hiding nearly every tool of a real server.
    #[test]
    fn generated_schemas_are_accepted_and_still_enforced() {
        let f = Fixture::new();
        let schema = r##"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "StampParams",
            "type": "object",
            "required": ["path", "field"],
            "properties": {
                "path": {"type": "string", "description": "The concept."},
                "field": {"$ref": "#/$defs/StampField"},
                "limit": {"type": ["integer", "null"], "format": "uint32", "minimum": 0},
                "note": {"type": ["string", "null"], "x-extension": true}
            },
            "$defs": {
                "StampField": {"type": "string", "enum": ["generated", "reviewed"]}
            }
        }"##;
        let definition =
            ToolDefinition::from_discovery(&f.repo, f.definition().key, schema).unwrap();
        let mut call = f.call();
        for (arguments, allowed) in [
            (r#"{"path":"/a","field":"generated","limit":3}"#, true),
            (r#"{"path":"/a","field":"reviewed","limit":null}"#, true),
            (r#"{"path":"/a","field":"other"}"#, false),
            (r#"{"path":"/a","field":"generated","limit":-1}"#, false),
            (r#"{"path":"/a"}"#, false),
        ] {
            call.arguments_json = arguments.into();
            let result = authorize(&f.repo, f.attempt, &call, &definition, None);
            assert_eq!(
                result != Err(CoreError::InvalidInput),
                allowed,
                "{arguments}: {result:?}"
            );
        }
    }
    #[test]
    fn rejects_unsupported_schema_and_forged_definition() {
        let f = Fixture::new();
        for schema in [
            r#"{"$ref":"https://example.test/schema"}"#,
            r##"{"$ref":"#/$defs/missing"}"##,
            r#"{"type":"object","properties":{"a":{"type":"no-such-type"}}}"#,
            "true",
            "[]",
        ] {
            assert!(
                ToolDefinition::from_discovery(&f.repo, f.definition().key, schema).is_err(),
                "{schema}"
            );
        }
        assert_eq!(
            validate_discovery_schema(r#"{"$ref":"https://example.test/schema"}"#),
            Err(SchemaRejection::ExternalReference)
        );
        assert_eq!(
            validate_discovery_schema("[]"),
            Err(SchemaRejection::NotAnObject)
        );
        let mut d = f.definition();
        d.schema_digest = "forged".into();
        assert!(authorize(&f.repo, f.attempt, &f.call(), &d, None).is_err());
    }

    fn attempt_row(repo: &Repository, attempt: AttemptId) -> (String, Option<String>) {
        repo.with_connection(move |conn| {
            conn.query_row(
                "SELECT state,approval_binding FROM attempts WHERE id=?1",
                [attempt.0.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|_| CoreError::Storage)
        })
        .unwrap()
    }
    fn server_config(
        f: &Fixture,
        url: &str,
        headers: Vec<(String, CredentialRef)>,
    ) -> ServerConfig {
        ServerConfig {
            id: f.server,
            name: "server".into(),
            url: url.into(),
            allow_http: false,
            secret_headers: headers,
        }
    }
    #[test]
    fn trusted_server_allows_new_tool_without_asking() {
        let f = Fixture::new();
        let d = f.definition();
        set_server_trust(&f.repo, f.server, true).unwrap();
        assert!(server_trusted(&f.repo, f.server).unwrap());
        assert_eq!(
            authorize(&f.repo, f.attempt, &f.call(), &d, None).unwrap(),
            Authorization::Allow
        );
        assert_eq!(attempt_row(&f.repo, f.attempt).0, "Dispatching");
    }
    #[test]
    fn trusted_server_allows_changed_schema_and_overrides_ask_rule() {
        let f = Fixture::new();
        let d = f.definition();
        set_rule(&f.repo, &d, PermissionRule::Ask).unwrap();
        set_server_trust(&f.repo, f.server, true).unwrap();
        let changed = ToolDefinition::from_discovery(
            &f.repo,
            d.key.clone(),
            r#"{"type":"object","properties":{"path":{"type":"string"}}}"#,
        )
        .unwrap();
        assert_eq!(
            authorize(&f.repo, f.attempt, &f.call(), &changed, None).unwrap(),
            Authorization::Allow
        );
    }
    #[test]
    fn trust_still_requires_current_observation_and_valid_arguments() {
        let f = Fixture::new();
        let d = f.definition();
        set_server_trust(&f.repo, f.server, true).unwrap();
        let mut bad = f.call();
        bad.arguments_json = r#"{"path":1}"#.into();
        assert!(authorize(&f.repo, f.attempt, &bad, &d, None).is_err());
        invalidate_mcp_discovery(&f.repo, f.profile, f.server).unwrap();
        assert!(authorize(&f.repo, f.attempt, &f.call(), &d, None).is_err());
        assert_eq!(attempt_row(&f.repo, f.attempt).0, "Proposed");
    }
    #[test]
    fn trust_ends_when_url_changes() {
        let f = Fixture::new();
        set_server_trust(&f.repo, f.server, true).unwrap();
        f.repo
            .save_server(server_config(&f, "https://other.example.test/mcp", vec![]))
            .unwrap();
        assert!(!server_trusted(&f.repo, f.server).unwrap());
        let d = f.definition();
        assert!(matches!(
            authorize(&f.repo, f.attempt, &f.call(), &d, None).unwrap(),
            Authorization::Ask(_)
        ));
    }
    #[test]
    fn trust_ends_when_a_credential_reference_changes() {
        let f = Fixture::new();
        let header = vec![("authorization".to_owned(), CredentialRef::new())];
        f.repo
            .save_server(server_config(&f, "https://server.test/mcp", header))
            .unwrap();
        set_server_trust(&f.repo, f.server, true).unwrap();
        assert!(server_trusted(&f.repo, f.server).unwrap());
        let replaced = vec![("authorization".to_owned(), CredentialRef::new())];
        f.repo
            .save_server(server_config(&f, "https://server.test/mcp", replaced))
            .unwrap();
        assert!(!server_trusted(&f.repo, f.server).unwrap());
        let d = f.definition();
        assert!(matches!(
            authorize(&f.repo, f.attempt, &f.call(), &d, None).unwrap(),
            Authorization::Ask(_)
        ));
        // Saving trust again binds it to the new destination.
        set_server_trust(&f.repo, f.server, true).unwrap();
        assert!(server_trusted(&f.repo, f.server).unwrap());
    }
    #[test]
    fn untrusting_removes_trust_and_unknown_server_is_rejected() {
        let f = Fixture::new();
        set_server_trust(&f.repo, f.server, true).unwrap();
        set_server_trust(&f.repo, f.server, false).unwrap();
        assert!(!server_trusted(&f.repo, f.server).unwrap());
        set_server_trust(&f.repo, f.server, false).unwrap();
        assert!(matches!(
            set_server_trust(&f.repo, ServerId::new(), true),
            Err(CoreError::Configuration)
        ));
    }
    #[test]
    fn block_beats_trust() {
        let f = Fixture::new();
        let d = f.definition();
        set_rule(&f.repo, &d, PermissionRule::Block).unwrap();
        set_server_trust(&f.repo, f.server, true).unwrap();
        assert_eq!(
            authorize(&f.repo, f.attempt, &f.call(), &d, None).unwrap(),
            Authorization::Block
        );
    }
    #[test]
    fn trust_never_covers_local_files() {
        let f = Fixture::new();
        set_server_trust(&f.repo, f.server, true).unwrap();
        let d = ToolDefinition::from_discovery(
            &f.repo,
            ToolKey {
                profile_id: f.profile,
                server: ToolServer::LocalFiles,
                name: "read".into(),
            },
            SCHEMA,
        )
        .unwrap();
        let call = ToolCall {
            key: d.key.clone(),
            ..f.call()
        };
        assert!(matches!(
            authorize(&f.repo, f.attempt, &call, &d, None).unwrap(),
            Authorization::Ask(_)
        ));
    }
    #[test]
    fn allow_dispatches_proposed_attempt_despite_stored_binding() {
        let mut f = Fixture::new();
        let d = f.definition();
        let b = ask(authorize(&f.repo, f.attempt, &f.call(), &d, None).unwrap());
        set_server_trust(&f.repo, f.server, true).unwrap();
        assert_eq!(
            authorize(&f.repo, f.attempt, &f.call(), &d, Some(&b)).unwrap(),
            Authorization::Allow
        );
        set_server_trust(&f.repo, f.server, false).unwrap();
        f.next_attempt();
        let b = ask(authorize(&f.repo, f.attempt, &f.call(), &d, None).unwrap());
        set_rule(&f.repo, &d, PermissionRule::Allow).unwrap();
        assert_eq!(
            authorize(&f.repo, f.attempt, &f.call(), &d, Some(&b)).unwrap(),
            Authorization::Allow
        );
    }
    #[test]
    fn repeated_approval_is_idempotent_even_after_session_closes() {
        let f = Fixture::new();
        let d = f.definition();
        let b = ask(authorize(&f.repo, f.attempt, &f.call(), &d, None).unwrap());
        approve(&f.repo, f.attempt, &b).unwrap();
        invalidate_mcp_discovery(&f.repo, f.profile, f.server).unwrap();
        approve(&f.repo, f.attempt, &b).unwrap();
        assert_eq!(attempt_row(&f.repo, f.attempt).0, "Approved");
        let mut other = b.clone();
        other.arguments_json = r#"{"path":"b"}"#.into();
        assert!(approve(&f.repo, f.attempt, &other).is_err());
    }
    #[test]
    fn prepare_stores_binding_without_changing_state() {
        let f = Fixture::new();
        let d = f.definition();
        prepare_approval(&f.repo, f.attempt, &f.call(), &d).unwrap();
        let (state, stored) = attempt_row(&f.repo, f.attempt);
        assert_eq!(state, "Proposed");
        let stored: ApprovalBinding = serde_json::from_str(&stored.unwrap()).unwrap();
        assert_eq!(stored.attempt_id, f.attempt);
        // Preparing again keeps the same binding; authorize asks with it.
        prepare_approval(&f.repo, f.attempt, &f.call(), &d).unwrap();
        assert_eq!(
            ask(authorize(&f.repo, f.attempt, &f.call(), &d, Some(&stored)).unwrap()),
            stored
        );
        assert_eq!(attempt_row(&f.repo, f.attempt).0, "Proposed");
    }
    #[test]
    fn prepare_leaves_allowed_blocked_and_invalid_calls_unbound() {
        let mut f = Fixture::new();
        let d = f.definition();
        set_rule(&f.repo, &d, PermissionRule::Allow).unwrap();
        prepare_approval(&f.repo, f.attempt, &f.call(), &d).unwrap();
        assert_eq!(attempt_row(&f.repo, f.attempt), ("Proposed".into(), None));
        set_rule(&f.repo, &d, PermissionRule::Block).unwrap();
        prepare_approval(&f.repo, f.attempt, &f.call(), &d).unwrap();
        assert_eq!(attempt_row(&f.repo, f.attempt), ("Proposed".into(), None));
        f.next_attempt();
        set_rule(&f.repo, &d, PermissionRule::Ask).unwrap();
        let mut bad = f.call();
        bad.arguments_json = r#"{"path":1}"#.into();
        prepare_approval(&f.repo, f.attempt, &bad, &d).unwrap();
        assert_eq!(attempt_row(&f.repo, f.attempt), ("Proposed".into(), None));
    }
    fn set_run_state(f: &Fixture, state: &'static str) {
        let run = f.run();
        f.repo
            .with_connection(move |conn| {
                conn.execute(
                    "UPDATE runs SET state=?1 WHERE id=?2",
                    params![state, run.0.to_string()],
                )
                .map_err(|_| CoreError::Storage)
            })
            .unwrap();
    }
    const CHANGED: &str = r#"{"type":"object","properties":{"path":{"type":"string"}}}"#;
    #[test]
    fn changed_definition_replaces_prepared_binding_and_asks_again() {
        let f = Fixture::new();
        let d = f.definition();
        prepare_approval(&f.repo, f.attempt, &f.call(), &d).unwrap();
        let old: ApprovalBinding =
            serde_json::from_str(&attempt_row(&f.repo, f.attempt).1.unwrap()).unwrap();
        let changed = ToolDefinition::from_discovery(&f.repo, d.key.clone(), CHANGED).unwrap();
        let fresh = ask(authorize(&f.repo, f.attempt, &f.call(), &changed, Some(&old)).unwrap());
        assert_ne!(fresh, old);
        let (state, stored) = attempt_row(&f.repo, f.attempt);
        assert_eq!(state, "Proposed");
        assert_eq!(
            serde_json::from_str::<ApprovalBinding>(&stored.unwrap()).unwrap(),
            fresh
        );
        // The fresh binding is the one to approve; it then dispatches once.
        approve(&f.repo, f.attempt, &fresh).unwrap();
        assert_eq!(
            authorize(&f.repo, f.attempt, &f.call(), &changed, Some(&fresh)).unwrap(),
            Authorization::Allow
        );
    }
    #[test]
    fn changed_definition_after_approval_asks_again_without_dispatch() {
        let f = Fixture::new();
        let d = f.definition();
        let b = ask(authorize(&f.repo, f.attempt, &f.call(), &d, None).unwrap());
        approve(&f.repo, f.attempt, &b).unwrap();
        let changed = ToolDefinition::from_discovery(&f.repo, d.key.clone(), CHANGED).unwrap();
        let fresh = ask(authorize(&f.repo, f.attempt, &f.call(), &changed, Some(&b)).unwrap());
        assert_ne!(fresh, b);
        assert_eq!(attempt_row(&f.repo, f.attempt).0, "Proposed");
    }
    #[test]
    fn prepare_skips_runs_that_are_not_running() {
        let f = Fixture::new();
        let d = f.definition();
        set_run_state(&f, "WaitingApproval");
        prepare_approval(&f.repo, f.attempt, &f.call(), &d).unwrap();
        assert_eq!(attempt_row(&f.repo, f.attempt), ("Proposed".into(), None));
    }
    #[test]
    fn prepare_skips_stale_observation_and_destination() {
        let f = Fixture::new();
        let d = f.definition();
        invalidate_mcp_discovery(&f.repo, f.profile, f.server).unwrap();
        prepare_approval(&f.repo, f.attempt, &f.call(), &d).unwrap();
        assert_eq!(attempt_row(&f.repo, f.attempt), ("Proposed".into(), None));
        let d = f.definition();
        f.repo
            .save_server(server_config(&f, "https://other.example.test/mcp", vec![]))
            .unwrap();
        prepare_approval(&f.repo, f.attempt, &f.call(), &d).unwrap();
        assert_eq!(attempt_row(&f.repo, f.attempt), ("Proposed".into(), None));
    }
    #[test]
    fn trust_ends_when_allow_http_changes() {
        let f = Fixture::new();
        set_server_trust(&f.repo, f.server, true).unwrap();
        let mut config = server_config(&f, "https://server.test/mcp", vec![]);
        config.allow_http = true;
        f.repo.save_server(config).unwrap();
        assert!(!server_trusted(&f.repo, f.server).unwrap());
    }
    #[test]
    fn repeated_approval_after_dispatch_is_already_decided() {
        let f = Fixture::new();
        let d = f.definition();
        let b = ask(authorize(&f.repo, f.attempt, &f.call(), &d, None).unwrap());
        approve(&f.repo, f.attempt, &b).unwrap();
        assert_eq!(
            authorize(&f.repo, f.attempt, &f.call(), &d, Some(&b)).unwrap(),
            Authorization::Allow
        );
        assert!(already_approved(&f.repo, &b).unwrap());
        approve(&f.repo, f.attempt, &b).unwrap();
        assert_eq!(attempt_row(&f.repo, f.attempt).0, "Dispatching");
        let mut other = b.clone();
        other.arguments_json = r#"{"path":"b"}"#.into();
        assert!(!already_approved(&f.repo, &other).unwrap());
    }
}
