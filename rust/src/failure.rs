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
                && w.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.=+/".contains(c));
            if tokenish {
                "•••".to_string()
            } else {
                w.to_string()
            }
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
}
