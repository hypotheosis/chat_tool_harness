//! Diagnosable failure context recorded where a request fails.
//!
//! `CoreError` stays a small kind enum. Boundaries that want to explain a
//! failure run their work inside [`scope`]; lower layers call [`note`] with a
//! sanitized [`FailureDetail`]. Outside a scope, `note` is a no-op.
//!
//! Rules:
//! - Notes made in `tokio::spawn` / `spawn_blocking` tasks are not captured.
//! - Concurrent `select!` / `join!` branches share one note and step; the
//!   last writer wins. Set the step per request.
//! - Nested scopes capture their own notes and start with no step.
//! - Read the detail only when the work returned `Err`.
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::future::Future;

pub const MAX_REASON_CHARS: usize = 160;
const MAX_BODY_BYTES: usize = 8192;

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

/// Redacts every `secrets` occurrence (on the raw text, so secrets containing
/// control characters still match; secrets shorter than 4 chars are not
/// redacted), replaces control and invisible format characters with spaces,
/// redacts every run of 32+ token characters, and caps the length.
pub fn sanitize_reason(raw: &str, secrets: &[&str]) -> Option<String> {
    let mut known: Vec<&str> = secrets
        .iter()
        .copied()
        .filter(|s| s.chars().count() >= 4)
        .collect();
    known.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    known.dedup();
    let mut text = raw.to_string();
    for secret in known {
        text = text.replace(secret, "•••");
    }
    let cleaned: String = text
        .chars()
        .map(|c| {
            if c.is_control() || is_invisible(c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    let mut out = String::with_capacity(cleaned.len());
    let mut run = String::new();
    for c in cleaned.chars().chain(std::iter::once('\n')) {
        if is_token_char(c) {
            run.push(c);
            continue;
        }
        if run.len() >= 32 {
            out.push_str("•••");
        } else {
            out.push_str(&run);
        }
        run.clear();
        out.push(c);
    }
    out.pop();
    let joined = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let capped: String = joined.chars().take(MAX_REASON_CHARS).collect();
    let trimmed = capped.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "-_.=+/".contains(c)
}

fn is_invisible(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
}

/// Extracts a human reason from the first 8 KiB of an error body: JSON
/// `error.message`, `error` (string), `message` or `detail` (string); falls
/// back to short plain text. HTML bodies (starting with `<`) yield `None`.
pub fn reason_from_body(body: &[u8], secrets: &[&str]) -> Option<String> {
    let body = &body[..body.len().min(MAX_BODY_BYTES)];
    let value: Option<serde_json::Value> = serde_json::from_slice(body).ok();
    let picked = value.as_ref().and_then(|v| {
        v.get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .or_else(|| v.get("error").and_then(|e| e.as_str()))
            .or_else(|| v.get("message").and_then(|m| m.as_str()))
            .or_else(|| v.get("detail").and_then(|m| m.as_str()))
            .map(str::to_owned)
    });
    let text = match picked {
        Some(text) => text,
        None if value.is_none() => {
            let plain = String::from_utf8_lossy(body).into_owned();
            if plain.trim_start().starts_with('<') {
                return None;
            }
            plain
        }
        None => return None,
    };
    sanitize_reason(&text, secrets)
}

/// Classifies a transport error without exposing its message. Only the
/// source chain is inspected: the top-level error carries the request URL,
/// whose text must not influence the classification.
pub fn transport_reason(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        return "timeout";
    }
    let mut chain = String::new();
    let mut refused = false;
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            refused |= io.kind() == std::io::ErrorKind::ConnectionRefused;
        }
        chain.push_str(&cause.to_string().to_ascii_lowercase());
        chain.push(' ');
        source = cause.source();
    }
    if error.is_connect() {
        if refused {
            "connection refused"
        } else if chain.contains("certificate") || chain.contains("tls") {
            "tls handshake failed"
        } else if chain.contains("dns") || chain.contains("resolve") || chain.contains("lookup") {
            "dns lookup failed"
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
            target: FailureTarget::McpServer {
                name: "tools".into(),
            },
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
        assert_eq!(
            reason_from_body(br#"{"error":"missing API key"}"#, &[]).as_deref(),
            Some("missing API key")
        );
        assert_eq!(
            reason_from_body(br#"{"message":"nope"}"#, &[]).as_deref(),
            Some("nope")
        );
        assert_eq!(reason_from_body(br#"{"other":1}"#, &[]), None);
        assert_eq!(
            reason_from_body(b"Unauthorized", &[]).as_deref(),
            Some("Unauthorized")
        );
        assert_eq!(reason_from_body(b"", &[]), None);
    }

    #[test]
    fn failure_display_url_drops_userinfo_query_fragment() {
        let url = reqwest::Url::parse("https://u:p@example.com/v1/models?k=1#f").unwrap();
        assert_eq!(display_url(&url), "https://example.com/v1/models");
    }

    #[test]
    fn failure_reason_redacts_wrapped_tokens() {
        let out = sanitize_reason(
            "invalid token \"abcdefghijklmnopqrstuvwxyz0123456789XYZ\", rejected",
            &[],
        )
        .unwrap();
        assert_eq!(out, "invalid token \"•••\", rejected");
        let out = sanitize_reason("key=abcdefghijklmnopqrstuvwxyz0123456789XYZ;", &[]).unwrap();
        assert_eq!(out, "•••;");
    }

    #[test]
    fn failure_reason_overlapping_secrets_fully_redacted() {
        assert_eq!(
            sanitize_reason("got abcdefgh", &["abcd", "abcdefgh"]).as_deref(),
            Some("got •••")
        );
    }

    #[test]
    fn failure_reason_secret_with_control_chars_matches() {
        assert_eq!(
            sanitize_reason("bad ab\tcd here", &["ab\tcd"]).as_deref(),
            Some("bad ••• here")
        );
    }

    #[test]
    fn failure_reason_strips_invisible_format_chars() {
        let out = sanitize_reason("a\u{202E}b\u{200B}c\u{FEFF}d", &[]).unwrap();
        assert_eq!(out, "a b c d");
    }

    #[test]
    fn failure_reason_from_body_detail_html_and_bound() {
        assert_eq!(
            reason_from_body(br#"{"detail":"bad thing"}"#, &[]).as_deref(),
            Some("bad thing")
        );
        assert_eq!(reason_from_body(b"  <html>oops</html>", &[]), None);
        let big = vec![b'a'; 100_000];
        let out = reason_from_body(&big, &[]).unwrap();
        assert!(out.chars().count() <= MAX_REASON_CHARS);
    }

    #[tokio::test]
    async fn failure_note_in_spawned_task_is_not_captured() {
        let ((), got) = scope(async {
            let _ = tokio::spawn(async { note(detail("elsewhere")) }).await;
        })
        .await;
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn failure_transport_reason_ignores_url_text() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let err = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{port}/dns/tls/certificate"))
            .send()
            .await
            .unwrap_err();
        let reason = transport_reason(&err);
        assert!(reason == "connection refused", "{reason}");
    }
}
