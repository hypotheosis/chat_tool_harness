# User-Testing Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every failure say what failed and what to do, make credentials predictable, make conversations follow current settings, and declutter the chat and settings screens.

**Architecture:** A task-local failure note (`rust/src/failure.rs`) lets the net/MCP/inference layers record a sanitized `FailureDetail` where a request fails without changing `CoreError` or `CoreResult`; run execution and diagnostics read it at their boundary, persist it in `runs.failure`, and carry it across the bridge in a new `AgentFailure` error. Conversations refresh their profile snapshot automatically before every turn, summary and resume. Disclosure moves to an app-wide per-origin table. Dart gains a shared `ResultBanner`, a rewritten `failureMessage`, and a decluttered chat screen.

**Tech Stack:** Rust (tokio, reqwest/rustls, rusqlite, serde), flutter_rust_bridge 2.13.0, Flutter/Dart, Kotlin (Android credential channel).

**Spec:** `docs/superpowers/specs/2026-11-08-user-testing-fixes-design.md`.

**Global rules for every task:**
- Follow `AGENTS.md`. No `unwrap`/`expect`/panics in production paths; tests may use them.
- Never hand-edit `rust/src/frb_generated.rs` or `lib/src/rust/**`; regenerate with `flutter_rust_bridge_codegen generate` (2.13.0) from the repo root.
- Do not put real hostnames, domains, IPs or personal names in code, tests, fixtures, docs or commit messages; use `example.com` / `example.internal`.
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Rust gates: `cargo fmt --manifest-path rust/Cargo.toml -- --check`, `cargo clippy --manifest-path rust/Cargo.toml --all-targets -- -D warnings`, `cargo test --manifest-path rust/Cargo.toml`.
- Dart gates: `dart format --output=none --set-exit-if-changed lib test integration_test`, `flutter analyze`, `flutter test`.

## File map

| File | Change |
|---|---|
| `rust/src/failure.rs` (new) | `FailureDetail`, `FailureTarget`, `FailureStep`, reason sanitizing, task-local `scope`/`note` |
| `rust/src/lib.rs` | `pub mod failure;` |
| `rust/src/error.rs` | add `CoreError::CredentialMissing` |
| `rust/src/net.rs` | note details on transport/status failures; credential scheme rule; redact secrets in reasons |
| `rust/src/mcp.rs` | note details for initialize/list/call status failures |
| `rust/src/inference.rs` | step tagging for discover/chat |
| `rust/src/application/mod.rs` | `CredentialMissing` from credential lookup; disclosure command by origin; describe credential plumbing (none in Rust) |
| `rust/src/application/diagnostics.rs` | wrap diagnostics in `failure::scope`, return detail |
| `rust/src/agent/mod.rs` | wrap run in `failure::scope`; refresh profile before start/summary/resume |
| `rust/src/agent/state.rs` | `settle` stores detail JSON; `load` parses it; origin disclosure; resume rebind |
| `rust/src/agent/context.rs` | `DisclosureAcknowledgment { origin }`, `endpoint_origin()` |
| `rust/src/storage/mod.rs` | `Repository::refresh_profile` |
| `rust/src/storage/migrations.rs` | v7: `acknowledged_origins` |
| `rust/src/application/dto.rs`, `mapping.rs` | `AgentFailure`, `AgentFailureDetail`, run `failure_detail`, profile `inference_origin`, conversation `origin_acknowledged` |
| `rust/src/api/agent.rs` | `agent_command` returns `Result<_, AgentFailure>` |
| `android/.../CredentialChannel.kt`, `CredentialStore.kt` | `describe` method |
| `lib/backend/models.dart`, `frb_backend.dart`, `platform_services.dart`, `backend.dart` | detail types, mapping, `describeCredential` |
| `lib/features/result_banner.dart` (new) | shared result/error banner |
| `lib/features/screen_controller.dart` | `failureMessage(BackendFailure)`, `FailureAction`, `success` field |
| `lib/features/settings/*.dart`, `lib/features/profiles/*.dart` | banner, safe area, credential summary, defaults |
| `lib/features/chat/chat_screen.dart`, `chat_controller.dart` | header row, failed-run card, origin disclosure card |
| `test/**`, `integration_test/agent_bridge_test.dart` | tests listed per task |

---

### Task 1: Failure detail module

**Files:**
- Create: `rust/src/failure.rs`
- Modify: `rust/src/lib.rs` (add `pub mod failure;` next to the other `pub mod` lines)

- [ ] **Step 1: Write the module with tests first (tests at the bottom fail to compile until the functions exist; write the whole file, then run).**

```rust
//! Diagnosable failure context recorded where a request fails.
//!
//! `CoreError` stays a small kind enum. Boundaries that want to explain a
//! failure run their work inside [`scope`]; lower layers call [`note`] with a
//! sanitized [`FailureDetail`]. Outside a scope, `note` is a no-op.
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::future::Future;

pub const MAX_REASON_CHARS: usize = 160;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureTarget {
    Inference { name: String },
    McpServer { name: String },
    Profile { name: String },
    Credential,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureStep {
    DiscoverModels,
    ChatRequest,
    Initialize,
    ListTools,
    CallTool,
    ResolveCredential,
    RefreshProfile,
    Request,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureDetail {
    pub target: FailureTarget,
    pub step: FailureStep,
    pub url: Option<String>,
    pub status: Option<u16>,
    pub reason: Option<String>,
}

tokio::task_local! {
    static NOTE: RefCell<Option<FailureDetail>>;
    static STEP: RefCell<Option<FailureStep>>;
}

/// Runs `work` with a fresh failure note; returns its output and the last note.
pub async fn scope<F: Future>(work: F) -> (F::Output, Option<FailureDetail>) {
    NOTE.scope(RefCell::new(None), async move {
        let output = STEP.scope(RefCell::new(None), work).await;
        let detail = NOTE.try_with(|n| n.borrow_mut().take()).ok().flatten();
        (output, detail)
    })
    .await
}

/// Records `detail` as the most recent failure in the current scope.
pub fn note(mut detail: FailureDetail) {
    if detail.step == FailureStep::Request {
        if let Ok(Some(step)) = STEP.try_with(|s| *s.borrow()) {
            detail.step = step;
        }
    }
    let _ = NOTE.try_with(|n| *n.borrow_mut() = Some(detail));
}

/// Sets the logical step for notes recorded with `FailureStep::Request`.
pub fn set_step(step: FailureStep) {
    let _ = STEP.try_with(|s| *s.borrow_mut() = Some(step));
}

/// Strips control characters, redacts every `secrets` occurrence and long
/// token-like words, and caps the length on a char boundary.
pub fn sanitize_reason(raw: &str, secrets: &[&str]) -> Option<String> {
    let mut text: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    for secret in secrets.iter().filter(|s| s.len() >= 4) {
        text = text.replace(secret, "•••");
    }
    let words: Vec<String> = text
        .split_whitespace()
        .map(|w| {
            let tokenish = w.len() >= 32
                && w.chars().all(|c| c.is_ascii_alphanumeric() || "-_.=+/".contains(c));
            if tokenish { "•••".to_string() } else { w.to_string() }
        })
        .collect();
    let joined = words.join(" ");
    let capped: String = joined.chars().take(MAX_REASON_CHARS).collect();
    let trimmed = capped.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Extracts a human reason from a JSON error body: `error.message`,
/// `error` (string) or `message`; falls back to short plain text.
pub fn reason_from_body(body: &[u8], secrets: &[&str]) -> Option<String> {
    let value: Option<serde_json::Value> = serde_json::from_slice(body).ok();
    let picked = value.as_ref().and_then(|v| {
        v.get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .or_else(|| v.get("error").and_then(|e| e.as_str()))
            .or_else(|| v.get("message").and_then(|m| m.as_str()))
            .map(str::to_owned)
    });
    let text = match picked {
        Some(text) => text,
        None if value.is_none() => String::from_utf8_lossy(body).into_owned(),
        None => return None,
    };
    sanitize_reason(&text, secrets)
}

/// Classifies a transport error without exposing its message.
pub fn transport_reason(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        let text = format!("{error:?}").to_ascii_lowercase();
        if text.contains("dns") || text.contains("resolve") {
            "dns lookup failed"
        } else if text.contains("certificate") || text.contains("tls") {
            "tls handshake failed"
        } else {
            "connection failed"
        }
    } else if error.is_body() || error.is_decode() {
        "response body error"
    } else {
        "request failed"
    }
}

/// URL for display: scheme, host, port and path only.
pub fn display_url(url: &reqwest::Url) -> String {
    let mut shown = url.clone();
    let _ = shown.set_username("");
    let _ = shown.set_password(None);
    shown.set_query(None);
    shown.set_fragment(None);
    shown.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detail(reason: &str) -> FailureDetail {
        FailureDetail {
            target: FailureTarget::McpServer { name: "tools".into() },
            step: FailureStep::Request,
            url: None,
            status: Some(401),
            reason: Some(reason.into()),
        }
    }

    #[tokio::test]
    async fn failure_scope_returns_last_note() {
        let ((), got) = scope(async {
            note(detail("first"));
            note(detail("second"));
        })
        .await;
        assert_eq!(got.unwrap().reason.as_deref(), Some("second"));
    }

    #[tokio::test]
    async fn failure_note_outside_scope_is_ignored() {
        note(detail("nobody listens"));
        let ((), got) = scope(async {}).await;
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn failure_request_step_takes_current_step() {
        let ((), got) = scope(async {
            set_step(FailureStep::Initialize);
            note(detail("x"));
        })
        .await;
        assert_eq!(got.unwrap().step, FailureStep::Initialize);
    }

    #[test]
    fn failure_reason_strips_controls_and_caps_length() {
        let raw = format!("bad\u{0}\nkey {}", "é".repeat(400));
        let out = sanitize_reason(&raw, &[]).unwrap();
        assert!(!out.contains('\u{0}') && !out.contains('\n'));
        assert_eq!(out.chars().count(), MAX_REASON_CHARS);
    }

    #[test]
    fn failure_reason_redacts_known_secret_and_long_tokens() {
        let out = sanitize_reason(
            "rejected Bearer s3cr3t-value and abcdefghijklmnopqrstuvwxyz0123456789XYZ",
            &["s3cr3t-value"],
        )
        .unwrap();
        assert_eq!(out, "rejected Bearer ••• and •••");
    }

    #[test]
    fn failure_reason_from_json_shapes() {
        assert_eq!(
            reason_from_body(br#"{"error":{"message":"unknown API key"}}"#, &[]).as_deref(),
            Some("unknown API key")
        );
        assert_eq!(reason_from_body(br#"{"error":"missing API key"}"#, &[]).as_deref(), Some("missing API key"));
        assert_eq!(reason_from_body(br#"{"message":"nope"}"#, &[]).as_deref(), Some("nope"));
        assert_eq!(reason_from_body(br#"{"other":1}"#, &[]), None);
        assert_eq!(reason_from_body(b"Unauthorized", &[]).as_deref(), Some("Unauthorized"));
        assert_eq!(reason_from_body(b"", &[]), None);
    }

    #[test]
    fn failure_display_url_drops_userinfo_query_fragment() {
        let url = reqwest::Url::parse("https://u:p@example.com/v1/models?k=1#f").unwrap();
        assert_eq!(display_url(&url), "https://example.com/v1/models");
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test --manifest-path rust/Cargo.toml failure::`
Expected: 7 tests pass. If `tokio::task_local!` is unavailable, check `rust/Cargo.toml` tokio features include `rt` (task_local needs only the `rt` feature); do not add new crates.

- [ ] **Step 3: Gates and commit**

Run fmt and clippy (see Global rules). Commit:
```bash
git add rust/src/failure.rs rust/src/lib.rs
git commit -m "Add task-local failure detail notes with sanitized reasons"
```

---

### Task 2: Record failure details in the net layer, and the credential scheme rule

**Files:**
- Modify: `rust/src/net.rs` (`bearer_header` at ~line 62, MCP request builder ~lines 110-175, endpoint `send` ~lines 200-275)
- Test: `rust/src/net.rs` `mod tests`

Background: `send` (endpoint requests) maps 401/403 → `Authentication`, other non-2xx → `Protocol`, transport errors → `UnavailableTransport`, discarding detail. The MCP request builder (`mcp_request`) attaches custom secret headers verbatim. Both must now (a) note a `FailureDetail` and (b) apply one Authorization rule.

- [ ] **Step 1: Write failing tests** in `net.rs` `mod tests` (reuse the existing `endpoint()` helper and the `TcpListener` pattern from `net_oversized_http_chunk_rejected_before_owned_copy`):

```rust
    struct FixedKey(&'static str);
    impl CredentialProvider for FixedKey {
        fn resolve(
            &self,
            _: CredentialRef,
        ) -> Pin<Box<dyn Future<Output = CoreResult<SecretValue>> + Send + '_>> {
            let value = self.0.to_string();
            Box::pin(async move { Ok(SecretValue::new(value)) })
        }
    }

    async fn one_shot(status: &'static str, body: &'static str) -> (std::net::SocketAddr, tokio::task::JoinHandle<String>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 8192];
            let n = socket.read(&mut request).await.unwrap();
            socket
                .write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes())
                .await
                .unwrap();
            String::from_utf8_lossy(&request[..n]).into_owned()
        });
        (addr, task)
    }

    #[test]
    fn net_authorization_value_rule() {
        assert_eq!(authorization_value("abc"), "Bearer abc");
        assert_eq!(authorization_value("Bearer abc"), "Bearer abc");
        assert_eq!(authorization_value("bearer abc"), "bearer abc");
        assert_eq!(authorization_value("Basic dXNlcg=="), "Basic dXNlcg==");
        assert_eq!(authorization_value("okf_abc-def"), "Bearer okf_abc-def");
    }

    #[tokio::test]
    async fn net_endpoint_401_notes_reason_and_redacts_key() {
        let (addr, server) = one_shot("401 Unauthorized", r#"{"error":{"message":"unknown API key"}}"#).await;
        let mut endpoint = endpoint(&format!("http://{addr}/v1"));
        endpoint.allow_http = true;
        endpoint.credential = Some(CredentialRef::new());
        let (result, detail) = crate::failure::scope(async {
            NetClient::new(FixedKey("sk-test-key"))
                .unwrap()
                .get(&endpoint, "models", Instant::now() + Duration::from_secs(5), Cancellation::new())
                .await
        })
        .await;
        assert!(matches!(result, Err(CoreError::Authentication)));
        let request = server.await.unwrap().to_ascii_lowercase();
        assert!(request.contains("authorization: bearer sk-test-key"));
        let detail = detail.expect("detail noted");
        assert_eq!(detail.status, Some(401));
        assert_eq!(detail.reason.as_deref(), Some("unknown API key"));
        assert_eq!(detail.target, crate::failure::FailureTarget::Inference { name: "test".into() });
        assert_eq!(detail.url.as_deref(), Some(format!("http://{addr}/v1/models").as_str()));
    }

    #[tokio::test]
    async fn net_endpoint_credential_with_scheme_is_not_double_prefixed() {
        let (addr, server) = one_shot("200 OK", r#"{"data":[]}"#).await;
        let mut endpoint = endpoint(&format!("http://{addr}/v1"));
        endpoint.allow_http = true;
        endpoint.credential = Some(CredentialRef::new());
        let _ = NetClient::new(FixedKey("Bearer sk-test-key"))
            .unwrap()
            .get(&endpoint, "models", Instant::now() + Duration::from_secs(5), Cancellation::new())
            .await
            .unwrap();
        let request = server.await.unwrap().to_ascii_lowercase();
        assert!(request.contains("authorization: bearer sk-test-key\r\n"));
        assert!(!request.contains("bearer bearer"));
    }

    #[tokio::test]
    async fn net_connect_failure_notes_transport_reason() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let mut endpoint = endpoint(&format!("http://{addr}/v1"));
        endpoint.allow_http = true;
        let (result, detail) = crate::failure::scope(async {
            NetClient::new(FixedKey("unused"))
                .unwrap()
                .get(&endpoint, "models", Instant::now() + Duration::from_secs(5), Cancellation::new())
                .await
        })
        .await;
        assert!(matches!(result, Err(CoreError::UnavailableTransport)));
        let detail = detail.unwrap();
        assert_eq!(detail.status, None);
        assert_eq!(detail.reason.as_deref(), Some("connection refused"));
    }
```

Also add an MCP-header test next to the existing MCP request tests in `net.rs` (search for `secret_headers: vec![("Authorization".into(), CredentialRef::new())]` near line 406 and copy that test's server setup), asserting that a stored value `okf_abc` is sent as `authorization: bearer okf_abc`, a stored `Bearer okf_abc` is sent once, and a header named `X-Api-Key` with value `k1` is sent verbatim as `x-api-key: k1`.

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test --manifest-path rust/Cargo.toml net_`
Expected: compile error `cannot find function authorization_value`.

- [ ] **Step 3: Implement**

Replace `bearer_header` with:
```rust
/// `Authorization` value for a stored credential: verbatim when it already
/// starts with an auth scheme (`<token> <rest>`), otherwise `Bearer <value>`.
pub(crate) fn authorization_value(stored: &str) -> String {
    let has_scheme = stored.split_once(' ').is_some_and(|(scheme, rest)| {
        !rest.is_empty()
            && scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+.^_`|~-".contains(c))
    });
    if has_scheme { stored.to_string() } else { format!("Bearer {stored}") }
}
fn authorization_header(secret: &SecretValue) -> CoreResult<header::HeaderValue> {
    let mut value = header::HeaderValue::from_str(&authorization_value(secret.as_str()))
        .map_err(|_| CoreError::Configuration)?;
    value.set_sensitive(true);
    Ok(value)
}
```
Update the existing test `net_bearer_header_debug_redacts_while_preserving_wire_value` to call `authorization_header`. In the endpoint `send`, use `authorization_header(&secret)?`. In the MCP builder's custom header loop, when `name.eq_ignore_ascii_case("authorization")` build the value with `authorization_value(secret.as_str())`; otherwise keep `secret.as_str()` verbatim.

Add a private helper and call it at every failure exit of `send` and of the MCP request function. Keep the resolved secret strings in a local `Vec<String>` named `secrets` so they can be redacted:
```rust
fn note_failure(
    target: crate::failure::FailureTarget,
    url: &reqwest::Url,
    status: Option<u16>,
    reason: Option<String>,
) {
    crate::failure::note(crate::failure::FailureDetail {
        target,
        step: crate::failure::FailureStep::Request,
        url: Some(crate::failure::display_url(url)),
        status,
        reason,
    });
}
```
- Transport error (`Ok(Err(e))` from `send()`): `note_failure(target, &url, None, Some(transport_reason(&e).into()))` before returning `UnavailableTransport`. Keep the existing `Ok(Ok(Err(_)))` arm the same way.
- Non-2xx status in endpoint `send`: before mapping, read at most 4096 body bytes with the existing `next_chunk` loop (as the 400/422 branch already does), compute `crate::failure::reason_from_body(&body, &secret_refs)`, call `note_failure(target, &url, Some(status), reason)`, then return the same `CoreError` as today. For 400/422 on `chat/completions`, reuse the body already read.
- Endpoint target: `FailureTarget::Inference { name: endpoint.name.clone() }`; MCP target: `FailureTarget::McpServer { name: server.name.clone() }`.
- The MCP request function returns the `Response` to `mcp.rs` for status handling; it only notes transport failures. Status notes for MCP are added in Task 3.

- [ ] **Step 4: Run tests**

Run: `cargo test --manifest-path rust/Cargo.toml net`
Expected: all net tests pass, including the four new ones and the renamed header test.

- [ ] **Step 5: Gates and commit**
```bash
git add rust/src/net.rs
git commit -m "Note sanitized request failures and apply one Authorization scheme rule"
```

---

### Task 3: MCP and inference failure steps

**Files:**
- Modify: `rust/src/mcp.rs` (`initialize` ~line 179, `classify_status` ~line 671, list/call paths that check status ~lines 268, 450-460, 542, 593, 656)
- Modify: `rust/src/inference.rs` (`discover_models_with` ~line 243, chat request ~line 103 and ~line 318)
- Test: `rust/src/mcp.rs` and `rust/src/inference.rs` test modules

- [ ] **Step 1: Write failing tests.** In `mcp.rs` tests, copy the setup of an existing test that spins a fake MCP server (search `let server_task = tokio::spawn(async move {` near line 1756) and make the server answer the `initialize` POST with `HTTP/1.1 401 Unauthorized` and body `{"error":"unknown API key"}`. Wrap the discovery in `crate::failure::scope` and assert:

```rust
    assert!(matches!(result, Err(McpError::Authentication)));
    let detail = detail.expect("detail");
    assert_eq!(detail.step, crate::failure::FailureStep::Initialize);
    assert_eq!(detail.status, Some(401));
    assert_eq!(detail.reason.as_deref(), Some("unknown API key"));
```
Add a second test where `initialize` gets `404` and assert `McpError::UnsupportedTransport` with `detail.status == Some(404)` and `detail.step == Initialize`.

In `inference.rs` tests, add a discovery test against a one-shot server returning `401` with `{"error":{"message":"bad key"}}` and assert `detail.step == FailureStep::DiscoverModels`, and a chat test returning `500` asserting `detail.step == FailureStep::ChatRequest` and `status == Some(500)`.

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test --manifest-path rust/Cargo.toml mcp:: inference::`
Expected: new tests fail (`detail` is `None` or step is `Request`).

- [ ] **Step 3: Implement**
- At the start of `initialize`: `crate::failure::set_step(crate::failure::FailureStep::Initialize);`. At the start of tool listing: `set_step(FailureStep::ListTools)`; tool calls: `set_step(FailureStep::CallTool)`.
- Add to `mcp.rs`:
```rust
async fn note_status(server: &ServerConfig, response: Response, secrets: &[&str]) -> u16 {
    let status = response.status().as_u16();
    let url = crate::failure::display_url(response.url());
    let body = response.bytes().await.unwrap_or_default();
    let limited = &body[..body.len().min(4096)];
    crate::failure::note(crate::failure::FailureDetail {
        target: crate::failure::FailureTarget::McpServer { name: server.name.clone() },
        step: crate::failure::FailureStep::Request,
        url: Some(url),
        status: Some(status),
        reason: crate::failure::reason_from_body(limited, secrets),
    });
    status
}
```
  Call it on every non-success status path before converting to `McpError` (the 400/404/405 `UnsupportedTransport` branch in `initialize`, `classify_status` callers, and the 404-with-session `ExpiredSession` branches). Because `classify_status` takes `&Response`, restructure each caller as: `if !response.status().is_success() { let status = note_status(&self.server, response, &[]).await; return Err(classify_code(status, session)); }` with `classify_code(status: u16, session: bool) -> McpError` holding the existing match. Do not read bodies on success.
  Note: `note_status` passes no secrets; MCP secrets are never echoed by the server's error body in practice, and the long-token redaction in `sanitize_reason` covers accidental echoes.
- In `inference.rs`: `set_step(FailureStep::DiscoverModels)` at the start of `discover_models_with`; `set_step(FailureStep::ChatRequest)` before each `chat/completions` send.

- [ ] **Step 4: Run all Rust tests**

Run: `cargo test --manifest-path rust/Cargo.toml`
Expected: all pass.

- [ ] **Step 5: Gates and commit**
```bash
git add rust/src/mcp.rs rust/src/inference.rs
git commit -m "Tag MCP and inference failures with their step and response reason"
```

---

### Task 4: CredentialMissing and persisted run failure detail

**Files:**
- Modify: `rust/src/error.rs` (add variant + message)
- Modify: `rust/src/application/mod.rs:27-31` (`credential_value`)
- Modify: `rust/src/agent/mod.rs` (run spawn at ~line 383, `execute` at ~line 692, settle call ~line 763)
- Modify: `rust/src/agent/state.rs` (`settle` ~line 264, `RunData.failure` ~line 80, `load` ~line 165)
- Test: `rust/src/agent/tests.rs`, `rust/src/application/tests.rs`

- [ ] **Step 1: Write failing tests.**

In `rust/src/application/tests.rs` add:
```rust
#[test]
fn credential_missing_reply_maps_to_credential_missing() {
    assert_eq!(
        super::credential_value(super::CredentialReply::Missing).err(),
        Some(crate::error::CoreError::CredentialMissing)
    );
}
```
(If the enum variant for an absent credential is named differently, use the existing variant name from `CredentialReply` in `application/mod.rs`.)

In `rust/src/agent/tests.rs`, find an existing test that finishes a run with `request.finish(Err(CoreError::Authentication))` or similar fake-inference failure, copy it, and before finishing call `crate::failure::note(...)` from inside the fake inference future with `FailureDetail { target: Inference { name: "fake".into() }, step: ChatRequest, url: None, status: Some(401), reason: Some("unknown API key".into()) }`. Assert after settle:
```rust
    let data = state::load(&repo, run).unwrap();
    let detail: crate::failure::FailureDetail = serde_json::from_str(data.failure.as_deref().unwrap()).unwrap();
    assert_eq!(detail.reason.as_deref(), Some("unknown API key"));
```
Add a second test: a run that fails without any note still stores a `StoredFailure` whose detail has `target == Inference { name: <endpoint name> }`, `step == Request`, and `reason == Some("core authentication failure")` (the `CoreError` display text).

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test --manifest-path rust/Cargo.toml credential_missing agent::tests`
Expected: compile error on `CredentialMissing`.

- [ ] **Step 3: Implement**
- `error.rs`: add `CredentialMissing` to `CoreError` and `Self::CredentialMissing => "stored credential missing; re-enter it in settings"` to `Display`.
- `application/mod.rs` `credential_value`: map the missing/absent reply to `CredentialMissing` and keep `Failed(_)` → `Authentication`. Also note a detail there: `crate::failure::note(FailureDetail { target: FailureTarget::Credential, step: FailureStep::ResolveCredential, url: None, status: None, reason: None })`.
- `mcp.rs` `From<CoreError> for McpError` (line ~39): map `CoreError::CredentialMissing` to `McpError::Authentication`, and in `agent/mod.rs` `mcp_error` keep `Authentication`. To preserve the distinction for runs, `execute` uses the noted detail's target (`Credential`) when choosing the final kind: if the detail target is `FailureTarget::Credential` and the result is `Err(CoreError::Authentication)`, replace it with `Err(CoreError::CredentialMissing)`.
- `agent/mod.rs` spawn site: replace `execute(inner, worker_execution, child).await;` with `crate::failure::scope(execute(inner, worker_execution, child)).await;` — `execute` itself reads the note before settling: just before computing `let error = result.err().or(flush.err());`, add `let detail = crate::failure::take();` and add to `failure.rs`:
```rust
/// Takes the current note without leaving the scope.
pub fn take() -> Option<FailureDetail> {
    NOTE.try_with(|n| n.borrow_mut().take()).ok().flatten()
}
```
  Pass `detail` and the run's endpoint name into `state::settle`.
- `state::settle` signature becomes `(repo, run, status, error: Option<CoreError>, detail: Option<FailureDetail>, total)`. Persist:
```rust
let failure = error.map(|e| {
    let detail = detail.unwrap_or_else(|| crate::failure::FailureDetail {
        target: crate::failure::FailureTarget::Inference { name: endpoint_name.clone() },
        step: crate::failure::FailureStep::Request,
        url: None,
        status: None,
        reason: None,
    });
    let detail = crate::failure::FailureDetail { reason: detail.reason.or_else(|| Some(e.to_string())), ..detail };
    serde_json::to_string(&StoredFailure { code: e, detail }).unwrap_or_else(|_| e.to_string())
});
```
  where `StoredFailure { code: CoreError, detail: FailureDetail }` is a private serde struct in `state.rs` (derive `Serialize, Deserialize`; add `Serialize, Deserialize` derives to `CoreError`). `endpoint_name` is read in the same transaction: `SELECT s.payload FROM runs r JOIN conversation_snapshots s ON s.conversation_id=r.conversation_id AND s.revision=r.snapshot_revision WHERE r.id=?1`, parsed as `ProfileSnapshot`, `.endpoint.name`.
- `RunData.failure` becomes `Option<StoredFailure>`; `load` parses JSON and falls back for old plain strings to `StoredFailure { code: CoreError::Protocol, detail: FailureDetail { target: Inference { name: "" }, step: Request, url: None, status: None, reason: Some(old) } }`. Make `StoredFailure` `pub` and re-export from `agent::state` so mapping can use it. Update the test assertion to `data.failure.unwrap().detail.reason`.
- Fix every other `settle(` call site and test to the new signature (`grep -rn "settle(" rust/src`).

- [ ] **Step 4: Run all Rust tests**

Run: `cargo test --manifest-path rust/Cargo.toml`
Expected: all pass.

- [ ] **Step 5: Gates and commit**
```bash
git add rust/src
git commit -m "Persist run failures with target, step, status and reason"
```

---

### Task 5: Live profile refresh

**Files:**
- Modify: `rust/src/storage/mod.rs` (add `refresh_profile` next to `apply_profile` ~line 224)
- Modify: `rust/src/agent/mod.rs` (`start_turn` ~288, `summarize_context` ~312, `resume` ~488)
- Modify: `rust/src/agent/state.rs` (add `rebind_to_current`)
- Test: `rust/src/storage/tests.rs`, `rust/src/agent/tests.rs`

- [ ] **Step 1: Write failing tests** in `rust/src/storage/tests.rs` (use the existing helpers there that create an endpoint, server, profile and conversation; search for `apply_profile(` in that file for a template):

```rust
#[test]
fn refresh_profile_adds_revision_only_when_settings_change() {
    let (repo, conversation, profile, mut endpoint) = seeded_conversation(); // existing-helper style; build from the file's current fixtures
    assert!(!repo.refresh_profile(conversation).unwrap());
    assert_eq!(current_revision(&repo, conversation), 1);
    endpoint.base_url = "https://example.com/v1".into();
    repo.save_endpoint(endpoint.clone()).unwrap();
    assert!(repo.refresh_profile(conversation).unwrap());
    assert_eq!(current_revision(&repo, conversation), 2);
    assert!(!repo.refresh_profile(conversation).unwrap());
    let snapshot = repo.conversation_profile_revision(conversation, 2).unwrap();
    assert_eq!(snapshot.endpoint.base_url, "https://example.com/v1");
    let _ = profile;
}
```
If `seeded_conversation`/`current_revision` helpers do not exist, add them in the test file using the existing setup code and `SELECT current_revision FROM conversations WHERE id=?1`.

In `rust/src/agent/tests.rs`, copy an existing resume test (search `resume(`) and add: change the endpoint's `base_url` between pause and resume, call `resume`, then assert the run's `snapshot_revision` equals the conversation's new `current_revision` and the fake inference saw the new URL.

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test --manifest-path rust/Cargo.toml refresh_profile resume`
Expected: compile error `no method named refresh_profile`.

- [ ] **Step 3: Implement** in `storage/mod.rs`:
```rust
    /// Rebuilds the conversation's profile snapshot from current settings and
    /// records a new revision only if anything changed. Returns whether it did.
    pub fn refresh_profile(&self, conversation: ConversationId) -> CoreResult<bool> {
        self.with_connection(move |connection| {
            let transaction = connection.transaction().map_err(|_| CoreError::Storage)?;
            let (current, payload): (i64, String) = transaction
                .query_row(
                    "SELECT c.current_revision, s.payload FROM conversations c JOIN conversation_snapshots s ON s.conversation_id=c.id AND s.revision=c.current_revision WHERE c.id=?1",
                    params![id(conversation)],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|_| CoreError::Storage)?
                .ok_or(CoreError::Conflict)?;
            let existing: ProfileSnapshot = from_json(&payload)?;
            let mut fresh = profile_snapshot(&transaction, existing.profile_id).map_err(|e| {
                crate::failure::note(crate::failure::FailureDetail {
                    target: crate::failure::FailureTarget::Profile { name: existing.name.clone() },
                    step: crate::failure::FailureStep::RefreshProfile,
                    url: None,
                    status: None,
                    reason: Some("profile, endpoint or server no longer exists".into()),
                });
                if e == CoreError::Conflict { CoreError::Configuration } else { e }
            })?;
            fresh.revision = existing.revision;
            if fresh == existing {
                return Ok(false);
            }
            let revision = current.checked_add(1).ok_or(CoreError::Limit)?;
            fresh.revision = revision;
            transaction.execute("INSERT INTO conversation_snapshots(conversation_id, revision, payload) VALUES(?1, ?2, ?3)", params![id(conversation), revision, json(&fresh)?]).map_err(|_| CoreError::Storage)?;
            transaction.execute("UPDATE conversations SET current_revision=?1 WHERE id=?2", params![revision, id(conversation)]).map_err(|_| CoreError::Storage)?;
            transaction.commit().map_err(|_| CoreError::Storage)?;
            Ok(true)
        })
    }
```
In `agent/state.rs` add:
```rust
/// Points a resumable run at its conversation's current snapshot revision.
pub fn rebind_to_current(repo: &Repository, run: RunId) -> CoreResult<()> {
    repo.with_connection(move |conn| {
        conn.execute(
            "UPDATE runs SET snapshot_revision=(SELECT c.current_revision FROM conversations c WHERE c.id=runs.conversation_id) WHERE id=?1",
            [run.0.to_string()],
        )
        .map_err(sql_error)?;
        Ok(())
    })
}
```
In `agent/mod.rs`:
- `start_turn` prepare closure: `move |repo| { repo.refresh_profile(conversation)?; state::start(&repo, run, conversation, text) }`.
- `summarize_context`: same with `state::start_summary`.
- `resume`: after loading `data` and the state check, before `transition`: `repo.refresh_profile(data.conversation)?; state::rebind_to_current(&repo, run)?;` (use the field name `RunData` has for the conversation id).

- [ ] **Step 4: Run all Rust tests**

Run: `cargo test --manifest-path rust/Cargo.toml`
Expected: all pass. Existing tests that asserted a run keeps an old snapshot after `apply_profile` must be updated to the new behavior: a turn after a settings edit uses the edited settings.

- [ ] **Step 5: Gates and commit**
```bash
git add rust/src
git commit -m "Refresh conversation profile snapshots from current settings before each turn and resume"
```

---

### Task 6: App-wide per-origin disclosure

**Files:**
- Modify: `rust/src/storage/migrations.rs` (VERSION 7, `migrate_v7`)
- Modify: `rust/src/agent/context.rs` (`DisclosureAcknowledgment`, `endpoint_origin`, check at ~line 350)
- Modify: `rust/src/agent/state.rs` (`acknowledge_disclosure`, `disclosure` ~lines 524-556)
- Modify: `rust/src/application/dto.rs:320` (`AcknowledgeContextDisclosure { origin }`), `application/mod.rs:403-418`, `application/mapping.rs:131`
- Modify: `rust/src/files/artifacts.rs:211` only if conversation deletion lists tables (leave `disclosure_acknowledgments` in its list)
- Test: `rust/src/agent/context_tests.rs`, `rust/src/storage/tests.rs`

- [ ] **Step 1: Write failing tests**

In `context_tests.rs`:
```rust
#[test]
fn endpoint_origin_ignores_path_and_keeps_port() {
    let mut e = test_endpoint(); // existing helper in this file; otherwise construct EndpointConfig
    e.base_url = "https://example.com/v1".into();
    assert_eq!(endpoint_origin(&e).unwrap(), "https://example.com");
    e.base_url = "https://example.com:8443/api/v1/".into();
    assert_eq!(endpoint_origin(&e).unwrap(), "https://example.com:8443");
    e.allow_http = true;
    e.base_url = "http://example.internal/v1".into();
    assert_eq!(endpoint_origin(&e).unwrap(), "http://example.internal");
}
```
In `storage/tests.rs`:
```rust
#[test]
fn migration_v7_copies_acknowledgments_as_origins() {
    let mut connection = rusqlite::Connection::open_in_memory().unwrap();
    crate::storage::migrations::migrate_v6_for_test(&mut connection).unwrap();
    // minimal parent row required by the foreign key
    connection.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
    connection.execute(
        "INSERT INTO disclosure_acknowledgments(conversation_id,payload) VALUES('c','{\"snapshot_revision\":2,\"destination\":\"https://example.com/v1/chat/completions\"}')",
        [],
    ).unwrap();
    crate::storage::migrations::apply(&mut connection).unwrap();
    let origin: String = connection.query_row("SELECT origin FROM acknowledged_origins", [], |r| r.get(0)).unwrap();
    assert_eq!(origin, "https://example.com");
}
```
And an agent-level test: acknowledging origin `https://example.com` for one conversation means a second conversation using an endpoint `https://example.com/v2` is not asked (`ContextChoiceReason::DisclosureRequired` absent), while an endpoint `https://other.example.com/v1` is asked.

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test --manifest-path rust/Cargo.toml endpoint_origin migration_v7 disclosure`
Expected: compile errors.

- [ ] **Step 3: Implement**
- `context.rs`:
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisclosureAcknowledgment {
    pub origin: String,
}
pub fn endpoint_origin(endpoint: &EndpointConfig) -> CoreResult<String> {
    let url = crate::net::endpoint_url(endpoint, "chat/completions")?;
    Ok(url.origin().ascii_serialization())
}
```
  Replace the check at ~line 350 with `request.disclosure.as_ref() != Some(&DisclosureAcknowledgment { origin: endpoint_origin(&inference.endpoint)? })`. Remove `disclosure_destination` and replace its uses: `mapping.rs:131` becomes `inference_origin: crate::agent::context::endpoint_origin(&v.endpoint).ok()` (rename the DTO field `inference_destination` → `inference_origin`).
- `migrations.rs`: `const VERSION: i64 = 7;`, `if version < 7 { migrate_v7(connection)?; }`, a `#[cfg(test)] pub(crate) fn migrate_v6_for_test` that runs v1..v6, and:
```rust
fn migrate_v7(connection: &mut Connection) -> SqlResult<()> {
    let tx = connection.transaction()?;
    tx.execute_batch("CREATE TABLE acknowledged_origins (origin TEXT PRIMARY KEY);")?;
    let payloads: Vec<String> = {
        let mut statement = tx.prepare("SELECT payload FROM disclosure_acknowledgments")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        rows.collect::<SqlResult<_>>()?
    };
    for payload in payloads {
        let origin = serde_json::from_str::<serde_json::Value>(&payload)
            .ok()
            .and_then(|v| v.get("destination").and_then(|d| d.as_str()).map(str::to_owned))
            .and_then(|d| reqwest::Url::parse(&d).ok())
            .map(|u| u.origin().ascii_serialization());
        if let Some(origin) = origin.filter(|o| o != "null") {
            tx.execute("INSERT OR IGNORE INTO acknowledged_origins(origin) VALUES(?1)", [origin])?;
        }
    }
    tx.execute_batch("PRAGMA user_version = 7;")?;
    tx.commit()
}
```
- `state.rs`: `acknowledge_disclosure(repo, conversation, ack)` validates `ack.origin` equals the origin of the conversation's current snapshot endpoint (else `InvalidInput`), then `INSERT OR IGNORE INTO acknowledged_origins(origin) VALUES(?1)` and keeps the `context:disclosure-acknowledged` event. `disclosure(repo, conversation, profile)` returns `Some(DisclosureAcknowledgment { origin })` iff that origin is in `acknowledged_origins`.
- `dto.rs` `AcknowledgeContextDisclosure { conversation_id: String, origin: String }`; `application/mod.rs` passes `DisclosureAcknowledgment { origin }`.
- Add `origin_acknowledged: bool` to the conversation DTO returned by the context snapshot command (find the struct in `dto.rs` that has a `profile: AgentProfileSnapshot` field and is built in `application/mod.rs` for the chat screen) computed with `state::disclosure(...)?.is_some()`.

- [ ] **Step 4: Run all Rust tests**

Run: `cargo test --manifest-path rust/Cargo.toml`
Expected: all pass; update existing disclosure tests to the origin form.

- [ ] **Step 5: Gates and commit**
```bash
git add rust/src
git commit -m "Acknowledge inference disclosure once per origin across the app"
```

---

### Task 7: Bridge failure detail and regenerate bindings

**Files:**
- Modify: `rust/src/application/dto.rs`, `rust/src/application/mapping.rs`, `rust/src/application/diagnostics.rs` (~line 262), `rust/src/api/agent.rs`
- Regenerate: `rust/src/frb_generated.rs`, `lib/src/rust/**`
- Modify: `lib/backend/models.dart`, `lib/backend/frb_backend.dart`
- Test: `rust/src/application/diagnostic_tests.rs`, `test/backend_contract_test.dart`, `integration_test/agent_bridge_test.dart`

- [ ] **Step 1: Write failing Rust test** in `diagnostic_tests.rs`: copy a server-discovery diagnostic test that uses a fake MCP server, make it return 401 with `{"error":"unknown API key"}`, call `Application::command(AppCommand::DiscoverServer{..})` through the new failure-returning path `command_with_detail`, and assert the error is `AgentFailure { code: CoreError::Authentication, detail: Some(d) }` with `d.status == Some(401)` and `d.reason == Some("unknown API key")`.

- [ ] **Step 2: Implement Rust side**
- `dto.rs`:
```rust
#[derive(Debug, Clone)]
pub struct AgentFailureDetail {
    pub target_kind: String,   // "inference" | "mcpServer" | "profile" | "credential"
    pub target_name: Option<String>,
    pub step: String,          // FailureStep variant name in lowerCamelCase
    pub url: Option<String>,
    pub status: Option<u16>,
    pub reason: Option<String>,
}
#[derive(Debug, Clone)]
pub struct AgentFailure {
    pub code: crate::error::CoreError,
    pub detail: Option<AgentFailureDetail>,
}
```
  Add `pub failure_detail: Option<AgentFailureDetail>` and `pub failure_code: Option<CoreError>` to the run DTO next to `failure: Option<String>` (line ~153); keep `failure` as the one-line display string.
- `mapping.rs`: `impl From<crate::failure::FailureDetail> for AgentFailureDetail` mapping each `FailureTarget` to `(target_kind, target_name)` and `FailureStep` to its lowerCamelCase name; run mapping fills `failure_code`, `failure_detail`, and `failure` = `detail.reason` or the code's display text.
- `diagnostics.rs` spawn: `let (result, detail) = crate::failure::scope(operation.run(command)).await;` and send `(result, detail)` through the oneshot; `Application::diagnostic` returns `Result<_, (CoreError, Option<FailureDetail>)>`. Add `Application::command_with_detail(&self, command) -> Result<AgentCommandReply, AgentFailure>` that wraps `self.command(..)` in `crate::failure::scope` and, on error, prefers the diagnostic detail when present.
- `api/agent.rs`: `pub async fn agent_command(handle: &AgentHandle, command: AppCommand) -> Result<AgentCommandReply, AgentFailure> { handle.0.command_with_detail(command).await }`.

- [ ] **Step 3: Run Rust tests and regenerate**

Run: `cargo test --manifest-path rust/Cargo.toml` → all pass.
Run from repo root: `flutter_rust_bridge_codegen generate` → completes without errors. Review `git diff --stat lib/src/rust rust/src/frb_generated.rs` and confirm only the new types/fields changed.

- [ ] **Step 4: Dart mapping, tests first.** In `test/backend_contract_test.dart` add a test that `mapFailure(AgentFailure(code: CoreError.authentication, detail: AgentFailureDetail(targetKind: 'mcpServer', targetName: 'tools', step: 'initialize', status: 401, reason: 'unknown API key', url: null)))` returns `BackendFailure` with `code == FailureCode.authentication` and `detail.targetName == 'tools'`. Expose the existing private `_failure` as `@visibleForTesting BackendFailure mapFailure(Object error)`.

  In `lib/backend/models.dart`:
```dart
enum FailureCode {
  invalidInput, configuration, authentication, credentialMissing, rateLimited,
  toolUnsupported, discoveryUnavailable, contextLength, unavailableTransport,
  protocol, limit, storage, cancelled, conflict,
}

class FailureDetail {
  const FailureDetail({required this.targetKind, this.targetName, required this.step, this.url, this.status, this.reason});
  final String targetKind;
  final String? targetName;
  final String step;
  final String? url;
  final int? status;
  final String? reason;
}

class BackendFailure implements Exception {
  const BackendFailure(this.code, {this.detail});
  final FailureCode code;
  final FailureDetail? detail;
  factory BackendFailure.fromNative(Object _) => const BackendFailure(FailureCode.unavailableTransport);
  @override
  String toString() => "Backend failure: ${code.name}";
}
```
  Add `failureCode` (`FailureCode?`) and `failureDetail` (`FailureDetail?`) to the run model next to `failure` and map them in `frb_backend.dart` (~line 474). In `mapFailure`, handle the generated `AgentFailure` type first (map `code` with the existing switch, now including `credentialMissing`, and convert `detail`), then the existing `errors.CoreError` branch.

- [ ] **Step 5: Real-bridge test.** In `integration_test/agent_bridge_test.dart` add a case that saves an endpoint pointing at `http://127.0.0.1:9/v1` with `allowHttp: true` (port 9 is discard/closed) and runs `DiscoverModels`; expect a `BackendFailure` with `detail?.step == 'discoverModels'` and `detail?.reason == 'connection refused'`.

- [ ] **Step 6: Gates and commit**

Run Rust and Dart gates; `flutter test`. Commit:
```bash
git add rust/src lib test integration_test
git commit -m "Carry failure detail across the bridge for commands and runs"
```

---

### Task 8: Describe stored credentials

**Files:**
- Modify: `android/app/src/main/kotlin/com/example/chat_tool_harness/CredentialStore.kt`, `CredentialChannel.kt`
- Modify: `lib/backend/platform_services.dart`
- Test: `integration_test/credential_lifecycle_test.dart`

- [ ] **Step 1: Write failing integration test** in `credential_lifecycle_test.dart`:
```dart
  testWidgets('describe reports scheme and length without the secret', (tester) async {
    const ref = 'describe-test-ref';
    await PlatformServices.putCredential(ref, 'Bearer okf_example');
    final d = await PlatformServices.describeCredential(ref);
    expect(d, isNotNull);
    expect(d!.scheme, 'Bearer');
    expect(d.length, 'Bearer okf_example'.length);
    await PlatformServices.putCredential(ref, 'plainkey');
    final bare = await PlatformServices.describeCredential(ref);
    expect(bare!.scheme, isNull);
    expect(bare.length, 8);
    await PlatformServices.deleteCredential(ref);
    expect(await PlatformServices.describeCredential(ref), isNull);
  });
```

- [ ] **Step 2: Implement**
- `CredentialStore.kt`:
```kotlin
    /** Scheme token (if the value starts with "<scheme> ") and UTF-8 length; null if absent. */
    @Synchronized fun describe(reference: String): Map<String, Any?>? {
        if (!file(reference).baseFile.exists()) return null
        val value = resolve(reference)
        try {
            val text = String(value, Charsets.UTF_8)
            val space = text.indexOf(' ')
            val scheme = if (space > 0 && space < text.length - 1 &&
                text[0].isLetter() && text.substring(0, space).all { it.isLetterOrDigit() || "!#$%&'*+.^_`|~-".contains(it) })
                text.substring(0, space) else null
            return mapOf("scheme" to scheme, "length" to value.size)
        } finally {
            value.fill(0)
        }
    }
```
- `CredentialChannel.kt`: add `"describe"` to the allowed method set and `"describe" -> store.describe(reference)` in the `when`.
- `platform_services.dart`:
```dart
class CredentialSummary {
  const CredentialSummary({this.scheme, required this.length});
  final String? scheme;
  final int length;
  /// What will be sent for an Authorization header.
  String get authorizationPreview =>
      scheme != null ? '$scheme ••• ($length chars)' : 'Bearer ••• (${length + 7} chars, Bearer added)';
}

  static Future<CredentialSummary?> describeCredential(String reference) async {
    final reply = await credentials.invokeMapMethod<String, Object?>('describe', {'reference': reference});
    if (reply == null) return null;
    return CredentialSummary(scheme: reply['scheme'] as String?, length: reply['length']! as int);
  }
```

- [ ] **Step 3: Run on the emulator** (never on the user's phone: integration tests install over the same package and can wipe its data):
Run: `flutter test integration_test/credential_lifecycle_test.dart -d emulator-5554`
Expected: pass. If no emulator is running, report this step as not run.

- [ ] **Step 4: Commit**
```bash
git add android/app/src/main/kotlin lib/backend/platform_services.dart integration_test/credential_lifecycle_test.dart
git commit -m "Describe stored credentials by scheme and length without exposing them"
```

---

### Task 9: Failure messages, result banner, settings screens, safe area, defaults

**Files:**
- Create: `lib/features/result_banner.dart`
- Modify: `lib/features/screen_controller.dart` (`failureMessage`, add `success`, `FailureAction`)
- Modify: `lib/features/settings/endpoint_screen.dart`, `server_screen.dart`, `credential_editor.dart`, `lib/features/profiles/profile_screen.dart`, `tool_rules_screen.dart`
- Test: `test/settings/server_test.dart`, `test/settings/endpoint_test.dart`, `test/settings/safe_area_test.dart` (new), `test/task12_test.dart` (update stale expectations)

- [ ] **Step 1: Write failing widget tests.**

`test/settings/server_test.dart`:
```dart
  testWidgets('failed connection test shows server, status and reason', (tester) async {
    final backend = UiBackend(); // from test/support/ui_fixture.dart
    await pumpServerScreen(tester, backend); // existing helper pattern in this file; save a server and select a profile first
    backend.failure = const BackendFailure(
      FailureCode.authentication,
      detail: FailureDetail(targetKind: 'mcpServer', targetName: 'tools', step: 'initialize', status: 401, reason: 'unknown API key'),
    );
    await tester.tap(find.text('Test connection and discover tools'));
    await tester.pumpAndSettle();
    expect(find.textContaining('MCP server "tools"'), findsOneWidget);
    expect(find.textContaining('HTTP 401'), findsOneWidget);
    expect(find.textContaining('unknown API key'), findsOneWidget);
    expect(find.textContaining('Task 12'), findsNothing);
  });

  testWidgets('successful connection test shows tool count', (tester) async {
    final backend = UiBackend()..discoveredTools = ['a', 'b'];
    await pumpServerScreen(tester, backend);
    await tester.tap(find.text('Test connection and discover tools'));
    await tester.pumpAndSettle();
    expect(find.textContaining('2 tools available'), findsOneWidget);
  });
```
Add the same failure-visible test to `endpoint_test.dart` for "Discover models" with `targetKind: 'inference'`, `step: 'discoverModels'`. If `UiBackend` lacks `discoveredTools`, add the field in `test/support/ui_fixture.dart` and return it from `execute` for `DiscoverServer`.

`test/settings/safe_area_test.dart`:
```dart
  testWidgets('last settings control is above the bottom system inset', (tester) async {
    tester.view.physicalSize = const Size(1080, 2340);
    tester.view.devicePixelRatio = 3;
    tester.view.padding = const FakeViewPadding(bottom: 144); // 48 logical px nav bar
    addTearDown(tester.view.reset);
    await pumpServerScreen(tester, UiBackend());
    await tester.scrollUntilVisible(find.text('Test connection and discover tools'), 200);
    await tester.pumpAndSettle();
    final bottom = tester.getBottomLeft(find.text('Test connection and discover tools')).dy;
    expect(bottom, lessThanOrEqualTo(2340 / 3 - 48));
  });
```
Repeat for the endpoint and profile screens.

- [ ] **Step 2: Run to confirm failure**

Run: `flutter test test/settings`
Expected: failures (no banner text; `FailureDetail` undefined until Task 7 is merged — Task 7 must be done first).

- [ ] **Step 3: Implement**

`screen_controller.dart` — replace `failureMessage` and add actions:
```dart
enum FailureAction { openSettings, reenterCredential, chooseContext, retry }

String _targetLabel(FailureDetail d) => switch (d.targetKind) {
      'inference' => 'Inference endpoint "${d.targetName ?? ''}"',
      'mcpServer' => 'MCP server "${d.targetName ?? ''}"',
      'profile' => 'Profile "${d.targetName ?? ''}"',
      _ => 'Stored credential',
    };

String _stepLabel(String step) => switch (step) {
      'discoverModels' => 'model discovery',
      'chatRequest' => 'chat request',
      'initialize' => 'initialize',
      'listTools' => 'tool listing',
      'callTool' => 'tool call',
      'resolveCredential' => 'credential lookup',
      'refreshProfile' => 'profile refresh',
      _ => 'request',
    };

String _codeText(FailureCode code) => switch (code) {
      FailureCode.authentication => 'The server rejected the credential.',
      FailureCode.credentialMissing => 'The stored credential is missing. Re-enter it in settings.',
      FailureCode.invalidInput => 'Some input is invalid. Check required fields and numbers.',
      FailureCode.configuration => 'The configuration is incomplete. Check the endpoint, profile and servers.',
      FailureCode.toolUnsupported => 'This model does not support tool calling. Use Chat only mode in the profile.',
      FailureCode.discoveryUnavailable => 'Model discovery is not available here. Enter the model name manually.',
      FailureCode.contextLength => 'The conversation is too long for the model. Choose context or summarize.',
      FailureCode.rateLimited => 'The provider is rate limiting. Try again shortly.',
      FailureCode.cancelled => 'Stopped. Remote effects are not rolled back.',
      FailureCode.limit => 'A safety limit was reached.',
      FailureCode.storage => 'Local storage failed. Refresh and try again.',
      FailureCode.protocol => 'The server sent an unexpected response.',
      FailureCode.unavailableTransport => 'The server could not be reached.',
      FailureCode.conflict => 'Something changed or is busy. Refresh and try again.',
    };

String failureMessage(Object error, {bool approval = false}) {
  final failure = error is BackendFailure ? error : const BackendFailure(FailureCode.unavailableTransport);
  if (approval && failure.code == FailureCode.conflict) {
    return 'This approval is stale or the policy changed. Refresh and review the current arguments.';
  }
  final d = failure.detail;
  if (d == null) return _codeText(failure.code);
  final parts = <String>[
    if (d.status != null) 'HTTP ${d.status}',
    'during ${_stepLabel(d.step)}',
  ];
  final reason = d.reason == null ? '' : ': ${d.reason}';
  return '${_targetLabel(d)}: ${parts.join(' ')}$reason. ${_codeText(failure.code)}';
}

FailureAction failureAction(FailureCode code) => switch (code) {
      FailureCode.credentialMissing => FailureAction.reenterCredential,
      FailureCode.authentication || FailureCode.unavailableTransport || FailureCode.protocol ||
      FailureCode.configuration || FailureCode.toolUnsupported || FailureCode.discoveryUnavailable =>
        FailureAction.openSettings,
      FailureCode.contextLength => FailureAction.chooseContext,
      _ => FailureAction.retry,
    };
```
In `ScreenController`, add `BackendFailure? lastFailure;` and `String? success;`; in `perform`, set `lastFailure = e is BackendFailure ? e : null` in the catch and clear both `lastFailure` and `success` at start.

`lib/features/result_banner.dart`:
```dart
import 'package:flutter/material.dart';
import '../backend/models.dart';
import 'screen_controller.dart';

class ResultBanner extends StatefulWidget {
  const ResultBanner({super.key, required this.state});
  final ScreenController state;
  @override
  State<ResultBanner> createState() => _ResultBannerState();
}

class _ResultBannerState extends State<ResultBanner> {
  bool details = false;
  @override
  Widget build(BuildContext context) {
    final s = widget.state;
    final scheme = Theme.of(context).colorScheme;
    if (s.error != null) {
      final d = s.lastFailure?.detail;
      return Card(
        color: scheme.errorContainer,
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Row(children: [Icon(Icons.warning_amber, color: scheme.onErrorContainer), const SizedBox(width: 8), const Text('Failed')]),
            const SizedBox(height: 4),
            SelectableText(s.error!),
            if (d != null)
              TextButton(onPressed: () => setState(() => details = !details), child: Text(details ? 'Hide details' : 'Details')),
            if (d != null && details)
              SelectableText([
                'Target: ${d.targetKind}${d.targetName == null ? '' : ' "${d.targetName}"'}',
                'Step: ${d.step}',
                if (d.url != null) 'URL: ${d.url}',
                if (d.status != null) 'HTTP status: ${d.status}',
                if (d.reason != null) 'Reason: ${d.reason}',
              ].join('\n'), style: const TextStyle(fontFamily: 'monospace')),
          ]),
        ),
      );
    }
    if (s.success != null) {
      return Card(child: ListTile(leading: const Icon(Icons.check_circle_outline), title: Text(s.success!)));
    }
    return const SizedBox.shrink();
  }
}
```
Settings screens:
- Wrap each `body:` in `SafeArea(top: false, child: ListView(padding: EdgeInsets.fromLTRB(16, 16, 16, 16 + MediaQuery.viewPaddingOf(context).bottom), children: [...]))` (keep existing children; replace an existing `padding:` if present).
- Replace the screen's `result` text with `ResultBanner(state: state)` placed directly under the last action button; where the screen sets `result = '...'` for success, set `state.success = '...'` and call `state.changed()` instead. Server discover success text: `'Connected to ${name.text}: ${tools.length} tools available'`. Endpoint discover success: `'${models.length} models available'`.
- `endpoint_screen.dart:23-24` defaults: `?? 32768` and `?? 2048`.
- `credential_editor.dart`: below the field, show `FutureBuilder<CredentialSummary?>(future: PlatformServices.describeCredential(widget.reference.text), ...)` rendering `Authorization: ${summary.authorizationPreview}` or `No credential stored`; rebuild it after store/delete by bumping `operationRevision` (already done) and keying the FutureBuilder with `ValueKey(operationRevision)`. In widget tests `PlatformServices` channel calls fail; catch errors in the future and render nothing.
- Remove the Task-12 sentence everywhere (`grep -rn "Task 12" lib`), and update `test/task12_test.dart` expectations that assert the old wording.

- [ ] **Step 4: Run tests**

Run: `flutter test` → all pass.

- [ ] **Step 5: Gates and commit**
```bash
git add lib test
git commit -m "Show settings results and failures, respect system insets, raise endpoint defaults"
```

---

### Task 10: Chat screen cleanup

**Files:**
- Modify: `lib/features/chat/chat_screen.dart` (run tile ~lines 135-200, conversation body ~lines 260-400)
- Modify: `lib/features/chat/chat_controller.dart` (`acknowledgeInferenceDisclosure` → `acknowledgeOrigin`, remove `applyProfile` UI use)
- Modify: `lib/backend/models.dart` (`AcknowledgeContextDisclosure` command: `conversationId`, `origin`; profile `inferenceOrigin`; conversation `originAcknowledged`) and `frb_backend.dart` mapping
- Test: `test/chat/chat_test.dart`

- [ ] **Step 1: Write failing widget tests** in `test/chat/chat_test.dart` using the fixture that pumps a conversation (search the file for the current pump helper):
```dart
  testWidgets('failed run shows one card with cause and settings action', (tester) async {
    final backend = UiBackend()..runs = [failedRun(
      code: FailureCode.authentication,
      detail: const FailureDetail(targetKind: 'mcpServer', targetName: 'tools', step: 'initialize', status: 401, reason: 'unknown API key'),
    )];
    await pumpChat(tester, backend);
    expect(find.textContaining('MCP server "tools"'), findsOneWidget);
    expect(find.text('Open tools settings'), findsOneWidget);
    expect(find.textContaining('revision'), findsNothing);
    expect(find.text('Apply current profile'), findsNothing);
    expect(find.textContaining('immutable'), findsNothing);
  });

  testWidgets('disclosure card only for unacknowledged origin', (tester) async {
    final backend = UiBackend()..originAcknowledged = false;
    await pumpChat(tester, backend);
    expect(find.textContaining('Messages will be sent to example.com'), findsOneWidget);
    await tester.tap(find.text('Allow'));
    await tester.pumpAndSettle();
    expect(backend.commands.whereType<AcknowledgeContextDisclosure>().single.origin, 'https://example.com');
    backend.originAcknowledged = true;
    await pumpChat(tester, backend);
    expect(find.textContaining('Messages will be sent to'), findsNothing);
  });

  testWidgets('successful runs show no run row', (tester) async {
    final backend = UiBackend()..runs = [completedRun()];
    await pumpChat(tester, backend);
    expect(find.textContaining('Run '), findsNothing);
  });
```
Add `failedRun`, `completedRun`, `runs`, `originAcknowledged` and `commands` to `test/support/ui_fixture.dart` if missing (record every executed command in `commands`).

- [ ] **Step 2: Run to confirm failure**

Run: `flutter test test/chat`
Expected: failures on old texts.

- [ ] **Step 3: Implement**
- Header row (replaces lines ~278-312): `ListTile(dense: true, title: Text(c.profile.name), subtitle: Text(c.profile.endpoint.model), trailing: IconButton(icon: const Icon(Icons.tune), tooltip: 'Edit profile', onPressed: () => Navigator.push(... ProfileScreen for c.profile.profileId ...)))`. Delete the snapshot sentence, "Apply current profile", "Repair configuration" and both disclosure paragraphs.
- Run rendering: replace the run tile with `_RunStatus(run)`:
  - `RunState.running` / `waitingApproval`: `Row(children: [const SizedBox.square(dimension: 16, child: CircularProgressIndicator(strokeWidth: 2)), const SizedBox(width: 8), const Text('Working…'), const Spacer(), OutlinedButton(onPressed: confirmStop, child: const Text('Stop'))])`; `confirmStop` shows an `AlertDialog` with the text "Stop is local and best effort. Work already sent to a server is not rolled back." and Stop/Cancel.
  - `failed` / `paused` / `interrupted`: a `Card` with title by state ("Run failed" / "Paused" / "Interrupted"), `Text(failureMessage(BackendFailure(run.failureCode ?? FailureCode.protocol, detail: run.failureDetail)))`, one action button chosen by `failureAction(code)`:
    - `openSettings`: label `Open ${detail?.targetName ?? 'profile'} settings`, opens `ServerScreen`/`EndpointScreen` for the named target from `c.profile` (match by name), else `ProfileScreen`.
    - `reenterCredential`: label `Re-enter credential`, same navigation.
    - `chooseContext`: label `Choose context`, existing context dialog.
    - `retry`: label `Resume` for paused/interrupted (existing resume), otherwise `Retry` which resends the run's input message via `StartTurn`.
    Plus a `TextButton('Details')` toggling a monospace block: run id, target, step, URL, status, reason.
  - `completed` / `cancelled`: render nothing.
- Disclosure: if `!c.originAcknowledged && c.profile.inferenceOrigin != null`, show above the composer a `Card` with `Text('Messages will be sent to ${Uri.parse(c.profile.inferenceOrigin!).host}.')` and `FilledButton(onPressed: () => state.acknowledgeOrigin(c.profile.inferenceOrigin!), child: const Text('Allow'))`. Disable Send while unacknowledged.
- Remove "Choose context"/"Choose summary context" buttons from the always-visible area; "Summarize" stays reachable from the existing app bar menu if present, otherwise add an `IconButton(icon: Icon(Icons.compress), tooltip: 'Summarize context')` to the app bar that opens the existing summary context dialog.
- Composer: wrap in `SafeArea(top: false, ...)`.
- `chat_controller.dart`: `Future<void> acknowledgeOrigin(String origin) => perform(AcknowledgeContextDisclosure(conversationId, origin));`.

- [ ] **Step 4: Run tests**

Run: `flutter test` → all pass.

- [ ] **Step 5: Gates and commit**
```bash
git add lib test
git commit -m "Declutter chat: compact header, one failure card with an action, origin disclosure"
```

---

### Task 11: Spec amendment

**Files:**
- Modify: `docs/superpowers/specs/2026-11-08-user-testing-fixes-design.md` §3
- Modify: `docs/superpowers/specs/2026-11-07-android-mcp-agent-design.md:62`

- [ ] **Step 1:** In the new spec §3 replace the bullet "`CoreError` keeps its kinds. Fallible boundary operations return `Failure { kind, detail }` ..." with: "`CoreError` keeps its kinds and `CoreResult` is unchanged. Lower layers record the detail with a task-local note (`rust/src/failure.rs`); run execution and diagnostics run inside a failure scope and read the last note at their boundary. The bridge's `agent_command` returns `AgentFailure { code, detail }`." Replace "discarded if the existing secret scanner flags it" with "with the request's resolved secrets and any 32+ character token-like word replaced by •••".

- [ ] **Step 2:** In the original design line 62 append: "Superseded 2026-11-08 by `2026-11-08-user-testing-fixes-design.md` §1: conversations refresh their snapshot from current settings before each turn, summary and resume."

- [ ] **Step 3: Commit**
```bash
git add docs/superpowers/specs
git commit -m "Record failure-note mechanism and live-profile supersession in specs"
```

---

### Task 12: Full verification and device install

- [ ] **Step 1: All gates from the repo root**
```sh
cargo fmt --manifest-path rust/Cargo.toml -- --check
cargo clippy --manifest-path rust/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path rust/Cargo.toml
dart format --output=none --set-exit-if-changed lib test integration_test
flutter analyze
flutter test
```
Expected: all pass. Record counts.

- [ ] **Step 2: Real-bridge suite on the emulator** (not the user's phone: integration tests install over the same package and can wipe its data):
```sh
flutter test integration_test/simple_test.dart -d emulator-5554
flutter test integration_test/agent_bridge_test.dart -d emulator-5554
flutter test integration_test/credential_lifecycle_test.dart -d emulator-5554
```
If the emulator is not running, report these as not run; do not substitute the phone without the user's approval.

- [ ] **Step 3: Install the debug app as an upgrade** (same debug signing key, data preserved):
```sh
flutter build apk --debug
adb -s <device-id> install -r build/app/outputs/flutter-apk/app-debug.apk
```

- [ ] **Step 4: Manual check with the user:** open the MCP server settings, run "Test connection and discover tools" with the current credential and confirm the banner names the server, HTTP status and reason; confirm the credential line shows the scheme/length; send a message in the old conversation and confirm the failure card or reply appears with no revision/disclosure clutter.
