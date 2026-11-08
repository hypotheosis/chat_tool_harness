use crate::error::{CoreError, CoreResult};
use crate::model::core::{CredentialRef, EndpointConfig};
use reqwest::{header, Client, Method, Response, Url};
use std::collections::HashSet;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::time::Instant;

pub type Cancellation = tokio_util::sync::CancellationToken;
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(120);
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_REQUEST_BYTES: usize = 512 * 1024;

pub struct SecretValue(String);
impl SecretValue {
    pub fn new(value: String) -> Self {
        Self(value)
    }
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}
/// Secret header values sent with one MCP request, kept only to redact that
/// request's error body.
pub struct SentSecrets(Vec<SecretValue>);
impl SentSecrets {
    /// Every sent value, plus the part after a scheme such as `Bearer `.
    pub(crate) fn redactions(&self) -> Vec<&str> {
        let mut out = Vec::new();
        for secret in &self.0 {
            out.push(secret.as_str());
            if let Some((_, bare)) = secret.as_str().split_once(' ') {
                out.push(bare);
            }
        }
        out
    }
}
impl fmt::Debug for SentSecrets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SentSecrets([REDACTED])")
    }
}
impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue([REDACTED])")
    }
}

/// Resolve from platform storage just in time for network requests or explicit redacted export.
pub trait CredentialProvider: Send + Sync {
    fn resolve(
        &self,
        reference: CredentialRef,
    ) -> Pin<Box<dyn Future<Output = CoreResult<SecretValue>> + Send + '_>>;
}

pub fn endpoint_url(endpoint: &EndpointConfig, suffix: &str) -> CoreResult<Url> {
    if endpoint.base_url.trim() != endpoint.base_url || endpoint.base_url.is_empty() {
        return Err(CoreError::InvalidInput);
    }
    let mut url = Url::parse(&endpoint.base_url).map_err(|_| CoreError::InvalidInput)?;
    if !matches!(url.scheme(), "https" | "http")
        || (url.scheme() == "http" && !endpoint.allow_http)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(suffix, "chat/completions" | "models")
    {
        return Err(CoreError::InvalidInput);
    }
    let path = url.path().trim_end_matches('/');
    let path = if path.is_empty() { "" } else { path };
    url.set_path(&format!("{path}/{suffix}"));
    Ok(url)
}

/// `Authorization` value for a stored credential: verbatim when it already
/// starts with an auth scheme (`<token> <rest>`), otherwise `Bearer <value>`.
pub(crate) fn authorization_value(stored: &str) -> String {
    let has_scheme = stored.split_once(' ').is_some_and(|(scheme, rest)| {
        !rest.is_empty()
            && scheme
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+.^_`|~-".contains(c))
    });
    if has_scheme {
        stored.to_string()
    } else {
        format!("Bearer {stored}")
    }
}

fn authorization_header(secret: &SecretValue) -> CoreResult<header::HeaderValue> {
    let mut value = header::HeaderValue::from_str(&authorization_value(secret.as_str()))
        .map_err(|_| CoreError::Configuration)?;
    value.set_sensitive(true);
    Ok(value)
}

fn note_failure(
    target: crate::failure::FailureTarget,
    url: &Url,
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

pub(crate) const ERROR_BODY_READ_TIMEOUT: Duration = Duration::from_secs(2);

/// Reads at most 4096 body bytes of an error response, waiting at most
/// `bound` for it. The flag is true when the body was cut short by the size
/// limit. A stalled or failed read yields whatever arrived; only caller
/// cancellation or the caller's own deadline is an error.
pub(crate) async fn read_error_body(
    response: &mut Response,
    deadline: Instant,
    cancel: &Cancellation,
    bound: Duration,
) -> CoreResult<(Vec<u8>, bool)> {
    let local = deadline.min(Instant::now() + bound);
    let mut body = Vec::new();
    loop {
        match next_chunk(response, local, cancel, 4096 - body.len()).await {
            Err(CoreError::Limit) => return Ok((body, true)),
            Err(_) => {
                return if cancel.is_cancelled() || Instant::now() >= deadline {
                    Err(CoreError::Cancelled)
                } else {
                    Ok((body, false))
                };
            }
            Ok(None) => return Ok((body, false)),
            Ok(Some(chunk)) => body.extend_from_slice(&chunk),
        }
    }
}

pub struct NetClient<P> {
    client: Client,
    provider: P,
}
impl<P: CredentialProvider> NetClient<P> {
    pub fn new(provider: P) -> CoreResult<Self> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .map_err(|_| CoreError::Configuration)?;
        Ok(Self { client, provider })
    }
    /// Send one exact-path MCP request; credential references are resolved only after validation.
    #[allow(clippy::too_many_arguments)] // Exact configured request + independent protocol/session/deadline/cancellation.
    pub async fn mcp_request(
        &self,
        server: &crate::model::core::ServerConfig,
        method: Method,
        body: Option<Vec<u8>>,
        version: Option<&str>,
        session: Option<&str>,
        deadline: Instant,
        cancel: &Cancellation,
    ) -> CoreResult<Response> {
        self.mcp_exchange(server, method, body, version, session, deadline, cancel)
            .await
            .map(|(response, _)| response)
    }
    /// [`NetClient::mcp_request`] that also returns the secret header values it
    /// sent, so a caller reading an error body can redact them.
    #[allow(clippy::too_many_arguments)] // Same request contexts as `mcp_request`.
    pub async fn mcp_exchange(
        &self,
        server: &crate::model::core::ServerConfig,
        method: Method,
        body: Option<Vec<u8>>,
        version: Option<&str>,
        session: Option<&str>,
        deadline: Instant,
        cancel: &Cancellation,
    ) -> CoreResult<(Response, SentSecrets)> {
        let url = Url::parse(&server.url).map_err(|_| CoreError::InvalidInput)?;
        let shown_url = url.clone();
        if server.url.trim() != server.url
            || url.query().is_some()
            || !matches!(url.scheme(), "https" | "http")
            || (url.scheme() == "http" && !server.allow_http)
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || !matches!(method, Method::POST | Method::GET | Method::DELETE)
            || (method != Method::POST && body.is_some())
            || body.as_ref().is_some_and(|b| b.len() > MAX_REQUEST_BYTES)
            || version.is_some_and(|v| !matches!(v, "2025-11-25" | "2025-06-18" | "2025-03-26"))
            || session.is_some_and(|v| {
                v.is_empty() || v.bytes().any(|byte| !(0x21..=0x7e).contains(&byte))
            })
            || deadline <= Instant::now()
        {
            return Err(CoreError::InvalidInput);
        }
        let mut names = HashSet::new();
        let mut custom = Vec::new();
        for (name, reference) in &server.secret_headers {
            let name = header::HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| CoreError::InvalidInput)?;
            if !names.insert(name.clone())
                || matches!(
                    name.as_str(),
                    "host"
                        | "content-length"
                        | "transfer-encoding"
                        | "connection"
                        | "accept"
                        | "content-type"
                        | "mcp-session-id"
                        | "mcp-protocol-version"
                        | "origin"
                        | "last-event-id"
                )
            {
                return Err(CoreError::InvalidInput);
            }
            custom.push((name, *reference));
        }
        let mut builder = self
            .client
            .request(method, url)
            .header(header::ACCEPT, "application/json, text/event-stream");
        if let Some(version) = version {
            builder = builder.header("MCP-Protocol-Version", version);
        }
        if let Some(session) = session {
            let mut value =
                header::HeaderValue::from_str(session).map_err(|_| CoreError::InvalidInput)?;
            value.set_sensitive(true);
            builder = builder.header("MCP-Session-Id", value);
        }
        let mut sent = Vec::new();
        let holder = crate::failure::FailureTarget::McpServer {
            name: server.name.clone(),
        };
        for (name, reference) in custom {
            let secret = tokio::select! {
                _ = cancel.cancelled() => return Err(CoreError::Cancelled),
                result = tokio::time::timeout_at(deadline, self.provider.resolve(reference)) => result.map_err(|_| CoreError::Cancelled)?,
            }
            .inspect_err(|_| crate::failure::attribute_credential(&holder))?;
            let mut value = if name == header::AUTHORIZATION {
                header::HeaderValue::from_str(&authorization_value(secret.as_str()))
            } else {
                header::HeaderValue::from_str(secret.as_str())
            }
            .map_err(|_| CoreError::Configuration)?;
            value.set_sensitive(true);
            builder = builder.header(name, value);
            sent.push(secret);
        }
        if let Some(body) = body {
            builder = builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(body);
        }
        tokio::select! {
            _ = cancel.cancelled() => Err(CoreError::Cancelled),
            result = tokio::time::timeout_at(deadline, tokio::time::timeout(IDLE_TIMEOUT, builder.send())) => match result {
                Err(_) => Err(CoreError::Cancelled),
                Ok(Err(_)) => {
                    note_failure(
                        crate::failure::FailureTarget::McpServer { name: server.name.clone() },
                        &shown_url,
                        None,
                        Some("timeout".into()),
                    );
                    Err(CoreError::UnavailableTransport)
                }
                Ok(Ok(Err(error))) => {
                    note_failure(
                        crate::failure::FailureTarget::McpServer { name: server.name.clone() },
                        &shown_url,
                        None,
                        Some(crate::failure::transport_reason(&error).into()),
                    );
                    Err(CoreError::UnavailableTransport)
                }
                Ok(Ok(Ok(response))) => Ok((response, SentSecrets(sent))),
            },
        }
    }
    pub async fn get(
        &self,
        endpoint: &EndpointConfig,
        suffix: &str,
        deadline: Instant,
        cancel: Cancellation,
    ) -> CoreResult<Response> {
        self.send(endpoint, suffix, Method::GET, None, deadline, &cancel)
            .await
    }
    /// Sends a bounded JSON body; redirects are rejected for all requests.
    pub async fn post(
        &self,
        endpoint: &EndpointConfig,
        suffix: &str,
        body: Vec<u8>,
        deadline: Instant,
        cancel: &Cancellation,
    ) -> CoreResult<Response> {
        if body.len() > MAX_REQUEST_BYTES {
            return Err(CoreError::Limit);
        }
        self.send(endpoint, suffix, Method::POST, Some(body), deadline, cancel)
            .await
    }
    async fn send(
        &self,
        endpoint: &EndpointConfig,
        suffix: &str,
        method: Method,
        body: Option<Vec<u8>>,
        deadline: Instant,
        cancel: &Cancellation,
    ) -> CoreResult<Response> {
        let url = endpoint_url(endpoint, suffix)?;
        if deadline <= Instant::now() {
            return Err(CoreError::Cancelled);
        }
        let shown_url = url.clone();
        let target = crate::failure::FailureTarget::Inference {
            name: endpoint.name.clone(),
        };
        let mut secret: Option<SecretValue> = None;
        let mut builder = self
            .client
            .request(method, url)
            .header(header::ACCEPT, "application/json, text/event-stream");
        if let Some(reference) = endpoint.credential {
            let resolved = tokio::select! {
                _ = cancel.cancelled() => return Err(CoreError::Cancelled),
                result = tokio::time::timeout_at(deadline, self.provider.resolve(reference)) => result.map_err(|_| CoreError::Cancelled)?,
            }
            .inspect_err(|_| crate::failure::attribute_credential(&target))?;
            builder = builder.header(header::AUTHORIZATION, authorization_header(&resolved)?);
            secret = Some(resolved);
        }
        if let Some(body) = body {
            builder = builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(body);
        }
        let mut response = tokio::select! {
            _ = cancel.cancelled() => return Err(CoreError::Cancelled),
            result = tokio::time::timeout_at(deadline, tokio::time::timeout(IDLE_TIMEOUT, builder.send())) => {
                match result {
                    Err(_) => return Err(CoreError::Cancelled),
                    Ok(Err(_)) => {
                        note_failure(target, &shown_url, None, Some("timeout".into()));
                        return Err(CoreError::UnavailableTransport);
                    }
                    Ok(Ok(Err(error))) => {
                        note_failure(
                            target,
                            &shown_url,
                            None,
                            Some(crate::failure::transport_reason(&error).into()),
                        );
                        return Err(CoreError::UnavailableTransport);
                    }
                    Ok(Ok(Ok(response))) => response,
                }
            },
        };
        let status = response.status().as_u16();
        if (200..=299).contains(&status) {
            return Ok(response);
        }
        let (body, truncated) =
            read_error_body(&mut response, deadline, cancel, ERROR_BODY_READ_TIMEOUT).await?;
        let mut secret_refs: Vec<&str> = Vec::new();
        if let Some(secret) = &secret {
            secret_refs.push(secret.as_str());
            if let Some((_, bare)) = secret.as_str().split_once(' ') {
                secret_refs.push(bare);
            }
        }
        note_failure(
            target,
            &shown_url,
            Some(status),
            crate::failure::reason_from_body(&body, &secret_refs),
        );
        match status {
            401 | 403 => Err(CoreError::Authentication),
            429 => Err(CoreError::RateLimited),
            400 | 422 if suffix == "chat/completions" => {
                if truncated {
                    return Err(CoreError::Protocol);
                }
                let error = serde_json::from_slice::<serde_json::Value>(&body).ok();
                let error = error.as_ref().and_then(|v| v.get("error"));
                let code = error.and_then(|v| v.get("code")).and_then(|v| v.as_str());
                let param = error.and_then(|v| v.get("param")).and_then(|v| v.as_str());
                if matches!(
                    code,
                    Some("context_length_exceeded" | "context_window_exceeded")
                ) {
                    Err(CoreError::ContextLength)
                } else if matches!(code, Some("unsupported_parameter" | "unsupported_feature"))
                    && matches!(param, Some("tools" | "tool_choice"))
                {
                    Err(CoreError::ToolUnsupported)
                } else {
                    Err(CoreError::Protocol)
                }
            }
            _ => Err(CoreError::Protocol),
        }
    }
}

pub async fn next_chunk(
    response: &mut Response,
    deadline: Instant,
    cancel: &Cancellation,
    remaining: usize,
) -> CoreResult<Option<Vec<u8>>> {
    tokio::select! {
        _ = cancel.cancelled() => Err(CoreError::Cancelled),
        result = tokio::time::timeout_at(deadline, tokio::time::timeout(IDLE_TIMEOUT, response.chunk())) => {
            match result {
                Err(_) => Err(CoreError::Cancelled),
                Ok(Err(_)) => Err(CoreError::UnavailableTransport),
                Ok(Ok(Err(_))) => Err(CoreError::UnavailableTransport),
                Ok(Ok(Ok(chunk))) => match chunk {
                    Some(bytes) if bytes.len() > remaining => Err(CoreError::Limit),
                    Some(bytes) => Ok(Some(bytes.to_vec())),
                    None => Ok(None),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::core::{CredentialRef, EndpointConfig, EndpointId};
    fn endpoint(base_url: &str) -> EndpointConfig {
        EndpointConfig {
            id: EndpointId::new(),
            name: "test".into(),
            base_url: base_url.into(),
            model: "manual".into(),
            context_tokens: 1024,
            max_output_tokens: 128,
            temperature: 0.2,
            allow_http: false,
            credential: None,
        }
    }
    #[tokio::test]
    async fn net_oversized_http_chunk_rejected_before_owned_copy() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        struct NoKey;
        impl CredentialProvider for NoKey {
            fn resolve(
                &self,
                _: CredentialRef,
            ) -> Pin<Box<dyn Future<Output = CoreResult<SecretValue>> + Send + '_>> {
                Box::pin(async { panic!("unexpected credential") })
            }
        }
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            let _ = socket.read(&mut request).await.unwrap();
            let body = vec![b'x'; 64 * 1024];
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            socket.write_all(&body).await.unwrap();
        });
        let mut endpoint = endpoint(&format!("http://{addr}/v1"));
        endpoint.allow_http = true;
        let mut response = NetClient::new(NoKey)
            .unwrap()
            .get(
                &endpoint,
                "models",
                Instant::now() + Duration::from_secs(5),
                Cancellation::new(),
            )
            .await
            .unwrap();
        let result = next_chunk(
            &mut response,
            Instant::now() + Duration::from_secs(5),
            &Cancellation::new(),
            1,
        )
        .await;
        assert!(matches!(result, Err(CoreError::Limit)));
        server.await.unwrap();
    }
    #[tokio::test]
    async fn mcp_exact_path_headers_and_response_limit() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        struct Key;
        impl CredentialProvider for Key {
            fn resolve(
                &self,
                _: CredentialRef,
            ) -> Pin<Box<dyn Future<Output = CoreResult<SecretValue>> + Send + '_>> {
                Box::pin(async { Ok(SecretValue::new("synthetic-key".into())) })
            }
        }
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            let n = socket.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..n]).to_ascii_lowercase();
            assert!(request.starts_with("post /configured/mcp http/1.1"));
            assert!(request.contains("authorization: bearer synthetic-key"));
            assert!(request.contains("mcp-protocol-version: 2025-11-25"));
            assert!(request.contains("mcp-session-id: synthetic-session"));
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await.unwrap();
        });
        let config = crate::model::core::ServerConfig {
            id: crate::model::core::ServerId::new(),
            name: "fixture".into(),
            url: format!("http://{addr}/configured/mcp"),
            allow_http: true,
            secret_headers: vec![("Authorization".into(), CredentialRef::new())],
        };
        let mut response = NetClient::new(Key)
            .unwrap()
            .mcp_request(
                &config,
                Method::POST,
                Some(b"{}".to_vec()),
                Some("2025-11-25"),
                Some("synthetic-session"),
                Instant::now() + Duration::from_secs(5),
                &Cancellation::new(),
            )
            .await
            .unwrap();
        assert_eq!(
            next_chunk(
                &mut response,
                Instant::now() + Duration::from_secs(5),
                &Cancellation::new(),
                2
            )
            .await
            .unwrap()
            .unwrap(),
            b"{}"
        );
        server.await.unwrap();
    }
    #[tokio::test]
    async fn mcp_invalid_headers_rejected_before_secret_resolution() {
        struct PanicKey;
        impl CredentialProvider for PanicKey {
            fn resolve(
                &self,
                _: CredentialRef,
            ) -> Pin<Box<dyn Future<Output = CoreResult<SecretValue>> + Send + '_>> {
                Box::pin(async { panic!("secret resolved before validation") })
            }
        }
        for names in [
            vec!["MCP-Protocol-Version"],
            vec!["Authorization", "authorization"],
            vec!["Host"],
            vec!["bad name"],
        ] {
            let config = crate::model::core::ServerConfig {
                id: crate::model::core::ServerId::new(),
                name: "fixture".into(),
                url: "http://127.0.0.1:1/mcp".into(),
                allow_http: true,
                secret_headers: names
                    .into_iter()
                    .map(|n| (n.into(), CredentialRef::new()))
                    .collect(),
            };
            assert!(matches!(
                NetClient::new(PanicKey)
                    .unwrap()
                    .mcp_request(
                        &config,
                        Method::POST,
                        Some(b"{}".to_vec()),
                        None,
                        None,
                        Instant::now() + Duration::from_secs(1),
                        &Cancellation::new(),
                    )
                    .await,
                Err(CoreError::InvalidInput)
            ));
        }
    }
    #[test]
    fn net_generic_request_future_is_send() {
        fn assert_send<T: Send>(_: T) {}
        fn check<P: CredentialProvider>(client: &NetClient<P>, endpoint: &EndpointConfig) {
            assert_send(client.get(
                endpoint,
                "models",
                Instant::now() + Duration::from_secs(1),
                Cancellation::new(),
            ));
        }
        struct NoKey;
        impl CredentialProvider for NoKey {
            fn resolve(
                &self,
                _: CredentialRef,
            ) -> Pin<Box<dyn Future<Output = CoreResult<SecretValue>> + Send + '_>> {
                Box::pin(async { panic!("unused") })
            }
        }
        let client = NetClient::new(NoKey).unwrap();
        check(&client, &endpoint("https://example.test/v1"));
    }
    #[test]
    fn net_http_requires_opt_in_and_userinfo_rejected() {
        assert!(endpoint_url(&endpoint("http://127.0.0.1:1/v1"), "chat/completions").is_err());
        let mut e = endpoint("http://127.0.0.1:1/v1");
        e.allow_http = true;
        assert_eq!(
            endpoint_url(&e, "chat/completions").unwrap().path(),
            "/v1/chat/completions"
        );
        assert!(endpoint_url(&endpoint("https://user:pass@example.test/v1"), "models").is_err());
        assert!(endpoint_url(&endpoint("file:///tmp/a"), "models").is_err());
        assert_eq!(
            endpoint_url(&endpoint("https://example.test/custom/v1/"), "models")
                .unwrap()
                .path(),
            "/custom/v1/models"
        );
    }
    #[test]
    fn net_bearer_header_debug_redacts_while_preserving_wire_value() {
        let value = authorization_header(&SecretValue::new("highly-private".into())).unwrap();
        assert!(value.is_sensitive());
        assert!(!format!("{value:?}").contains("highly-private"));
        assert_eq!(value.to_str().unwrap(), "Bearer highly-private");
    }
    #[test]
    fn net_secret_value_debug_redacts() {
        let secret = SecretValue::new("highly-private".into());
        assert!(!format!("{secret:?}").contains("highly-private"));
    }
    #[tokio::test]
    async fn net_loopback_rejects_redirect_without_forwarding_auth() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        let second = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let second_addr = second.local_addr().unwrap();
        let second_task = tokio::spawn(async move {
            if let Ok(Ok((_socket, _))) =
                tokio::time::timeout(Duration::from_millis(100), second.accept()).await
            {
                panic!("redirect target contacted")
            }
        });
        let first = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let first_addr = first.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = first.accept().await.unwrap();
            let mut request = vec![0; 4096];
            let n = socket.read(&mut request).await.unwrap();
            let req = String::from_utf8_lossy(&request[..n]);
            assert!(req.starts_with("GET /prefix/v1/models HTTP/1.1"));
            assert!(req
                .to_ascii_lowercase()
                .contains("authorization: bearer private-key"));
            socket.write_all(format!("HTTP/1.1 302 Found\r\nLocation: http://{second_addr}/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
        });
        struct Key;
        impl CredentialProvider for Key {
            fn resolve(
                &self,
                _: CredentialRef,
            ) -> Pin<Box<dyn Future<Output = CoreResult<SecretValue>> + Send + '_>> {
                Box::pin(async { Ok(SecretValue::new("private-key".into())) })
            }
        }
        let mut e = endpoint(&format!("http://{first_addr}/prefix/v1"));
        e.allow_http = true;
        e.credential = Some(CredentialRef::new());
        let result = NetClient::new(Key)
            .unwrap()
            .get(
                &e,
                "models",
                Instant::now() + Duration::from_secs(5),
                Cancellation::new(),
            )
            .await;
        assert!(matches!(result, Err(CoreError::Protocol)));
        server.await.unwrap();
        second_task.await.unwrap();
    }
    #[tokio::test]
    async fn net_provider_status_classification_without_body_disclosure() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        struct Key;
        impl CredentialProvider for Key {
            fn resolve(
                &self,
                _: CredentialRef,
            ) -> Pin<Box<dyn Future<Output = CoreResult<SecretValue>> + Send + '_>> {
                Box::pin(async { Ok(SecretValue::new("private-key".into())) })
            }
        }
        let cases = [
            (
                400,
                r#"{"error":{"code":"context_length_exceeded","message":"private-key"}}"#,
                CoreError::ContextLength,
            ),
            (
                422,
                r#"{"error":{"code":"context_window_exceeded","message":"private-key"}}"#,
                CoreError::ContextLength,
            ),
            (
                400,
                r#"{"error":{"code":"unsupported_parameter","param":"tools","message":"private-key"}}"#,
                CoreError::ToolUnsupported,
            ),
            (
                400,
                r#"{"error":{"code":"model_not_found","param":"model","message":"private-key"}}"#,
                CoreError::Protocol,
            ),
            (
                401,
                r#"{"error":{"code":"unsupported_parameter","param":"tools"}}"#,
                CoreError::Authentication,
            ),
            (
                429,
                r#"{"error":{"code":"unsupported_parameter","param":"tools"}}"#,
                CoreError::RateLimited,
            ),
            (
                400,
                r#"{"error":{"code":"unsupported_parameter","param":"messages"}}"#,
                CoreError::Protocol,
            ),
        ];
        for (status, body, expected) in cases {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0; 4096];
                let _ = socket.read(&mut buf).await.unwrap();
                socket.write_all(format!("HTTP/1.1 {status} Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            });
            let mut e = endpoint(&format!("http://{addr}/v1"));
            e.allow_http = true;
            e.credential = Some(CredentialRef::new());
            let err = NetClient::new(Key)
                .unwrap()
                .post(
                    &e,
                    "chat/completions",
                    b"{}".to_vec(),
                    Instant::now() + Duration::from_secs(5),
                    &Cancellation::new(),
                )
                .await
                .unwrap_err();
            assert_eq!(err, expected);
            assert!(!format!("{err:?} {err}").contains("private-key"));
            server.await.unwrap();
        }
    }
    #[tokio::test(start_paused = true)]
    async fn net_connect_deadline_is_30_seconds_with_pending_tls_handshake() {
        use tokio::io::AsyncReadExt;
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut hello = [0; 2048];
            let n = socket.read(&mut hello).await.unwrap();
            assert!(n > 0);
            accepted_tx.send(()).unwrap();
            tokio::time::sleep(Duration::from_secs(300)).await;
        });
        struct NoKey;
        impl CredentialProvider for NoKey {
            fn resolve(
                &self,
                _: CredentialRef,
            ) -> Pin<Box<dyn Future<Output = CoreResult<SecretValue>> + Send + '_>> {
                Box::pin(async { panic!("unexpected") })
            }
        }
        let e = endpoint(&format!("https://127.0.0.1:{}/v1", addr.port()));
        let waiting = tokio::spawn(async move {
            NetClient::new(NoKey)
                .unwrap()
                .get(
                    &e,
                    "models",
                    Instant::now() + Duration::from_secs(300),
                    Cancellation::new(),
                )
                .await
        });
        accepted_rx.await.unwrap();
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(29)).await;
        assert!(!waiting.is_finished());
        tokio::time::advance(Duration::from_secs(1)).await;
        assert!(matches!(
            waiting.await.unwrap(),
            Err(CoreError::UnavailableTransport)
        ));
        server.abort();
    }
    #[tokio::test(start_paused = true)]
    async fn net_response_headers_have_120_second_idle_limit() {
        use tokio::io::AsyncReadExt;
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (seen_tx, seen_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            let _ = socket.read(&mut buffer).await.unwrap();
            seen_tx.send(()).unwrap();
            tokio::time::sleep(Duration::from_secs(300)).await;
        });
        struct NoKey;
        impl CredentialProvider for NoKey {
            fn resolve(
                &self,
                _: CredentialRef,
            ) -> Pin<Box<dyn Future<Output = CoreResult<SecretValue>> + Send + '_>> {
                Box::pin(async { panic!("unexpected") })
            }
        }
        let mut e = endpoint(&format!("http://{addr}/v1"));
        e.allow_http = true;
        let wait = tokio::spawn(async move {
            NetClient::new(NoKey)
                .unwrap()
                .get(
                    &e,
                    "models",
                    Instant::now() + Duration::from_secs(300),
                    Cancellation::new(),
                )
                .await
        });
        seen_rx.await.unwrap();
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(120)).await;
        assert!(matches!(
            wait.await.unwrap(),
            Err(CoreError::UnavailableTransport)
        ));
        server.abort();
    }
    #[tokio::test(start_paused = true)]
    async fn net_caller_deadline_and_cancellation_interrupt_response() {
        use tokio::io::AsyncReadExt;
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            let _ = socket.read(&mut buffer).await.unwrap();
            tokio::time::sleep(Duration::from_secs(300)).await;
        });
        struct NoKey;
        impl CredentialProvider for NoKey {
            fn resolve(
                &self,
                _: CredentialRef,
            ) -> Pin<Box<dyn Future<Output = CoreResult<SecretValue>> + Send + '_>> {
                Box::pin(async { panic!("not expected") })
            }
        }
        let client = NetClient::new(NoKey).unwrap();
        let mut e = endpoint(&format!("http://{addr}/v1"));
        e.allow_http = true;
        let cancel = Cancellation::new();
        let waiting = tokio::spawn(async move {
            client
                .get(
                    &e,
                    "models",
                    Instant::now() + Duration::from_secs(10),
                    cancel,
                )
                .await
        });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(10)).await;
        assert!(matches!(waiting.await.unwrap(), Err(CoreError::Cancelled)));
        server.abort();
    }
    #[tokio::test(start_paused = true)]
    async fn net_idle_timeout_preserves_partial_text() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            let _ = socket.read(&mut buffer).await.unwrap();
            let body = "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"},\"finish_reason\":null}]}\n\n";
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{body}\r\n", body.len()).as_bytes()).await.unwrap();
            tokio::time::sleep(Duration::from_secs(500)).await;
        });
        struct NoKey;
        impl CredentialProvider for NoKey {
            fn resolve(
                &self,
                _: CredentialRef,
            ) -> Pin<Box<dyn Future<Output = CoreResult<SecretValue>> + Send + '_>> {
                Box::pin(async { panic!("not expected") })
            }
        }
        let mut e = endpoint(&format!("http://{addr}/v1"));
        e.allow_http = true;
        let mut response = NetClient::new(NoKey)
            .unwrap()
            .get(
                &e,
                "models",
                Instant::now() + Duration::from_secs(300),
                Cancellation::new(),
            )
            .await
            .unwrap();
        let cancel = Cancellation::new();
        let first = next_chunk(
            &mut response,
            Instant::now() + Duration::from_secs(300),
            &cancel,
            MAX_RESPONSE_BYTES,
        )
        .await
        .unwrap()
        .unwrap();
        assert!(String::from_utf8(first).unwrap().contains("partial"));
        let wait = tokio::spawn(async move {
            next_chunk(
                &mut response,
                Instant::now() + Duration::from_secs(300),
                &cancel,
                MAX_RESPONSE_BYTES,
            )
            .await
        });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(120)).await;
        assert!(matches!(
            wait.await.unwrap(),
            Err(CoreError::UnavailableTransport)
        ));
        server.abort();
    }
    #[test]
    fn net_validation_precedes_credential_lookup() {
        struct CredentialPanic;
        impl CredentialProvider for CredentialPanic {
            fn resolve(
                &self,
                _: CredentialRef,
            ) -> Pin<Box<dyn Future<Output = CoreResult<SecretValue>> + Send + '_>> {
                Box::pin(async { panic!("credential was resolved") })
            }
        }
        let mut e = endpoint("http://example.test/v1");
        e.credential = Some(CredentialRef::new());
        let client = NetClient::new(CredentialPanic).unwrap();
        let result = tokio::runtime::Runtime::new().unwrap().block_on(client.get(
            &e,
            "models",
            tokio::time::Instant::now() + Duration::from_secs(1),
            Cancellation::new(),
        ));
        assert!(matches!(result, Err(CoreError::InvalidInput)));
    }

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

    async fn one_shot(
        status: &'static str,
        body: &'static str,
    ) -> (std::net::SocketAddr, tokio::task::JoinHandle<String>) {
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
        // Shared with the Android describe test (integration_test/
        // credential_lifecycle_test.dart) so both sides agree on the scheme.
        let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../test/fixtures/authorization_parity.json"
        ))
        .unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let value = case["value"].as_str().unwrap();
            let expected = if case["scheme"].is_string() {
                value.to_string()
            } else {
                format!("Bearer {value}")
            };
            assert_eq!(authorization_value(value), expected, "case {value:?}");
            if case["sendable"] == true {
                assert!(
                    header::HeaderValue::from_str(&authorization_value(value)).is_ok(),
                    "sendable case {value:?} must be a valid header"
                );
            }
        }
    }

    #[tokio::test]
    async fn net_endpoint_401_notes_reason_and_redacts_key() {
        let (addr, server) = one_shot(
            "401 Unauthorized",
            r#"{"error":{"message":"unknown API key"}}"#,
        )
        .await;
        let mut endpoint = endpoint(&format!("http://{addr}/v1"));
        endpoint.allow_http = true;
        endpoint.credential = Some(CredentialRef::new());
        let (result, detail) = crate::failure::scope(async {
            NetClient::new(FixedKey("sk-test-key"))
                .unwrap()
                .get(
                    &endpoint,
                    "models",
                    Instant::now() + Duration::from_secs(5),
                    Cancellation::new(),
                )
                .await
        })
        .await;
        assert!(matches!(result, Err(CoreError::Authentication)));
        let request = server.await.unwrap().to_ascii_lowercase();
        assert!(request.contains("authorization: bearer sk-test-key"));
        let detail = detail.expect("detail noted");
        assert_eq!(detail.status, Some(401));
        assert_eq!(detail.reason.as_deref(), Some("unknown API key"));
        assert_eq!(
            detail.target,
            crate::failure::FailureTarget::Inference {
                name: "test".into()
            }
        );
        assert_eq!(
            detail.url.as_deref(),
            Some(format!("http://{addr}/v1/models").as_str())
        );
    }

    /// A provider whose stored credential is absent, noting it as the
    /// platform credential provider does.
    struct MissingKey;
    impl CredentialProvider for MissingKey {
        fn resolve(
            &self,
            _: CredentialRef,
        ) -> Pin<Box<dyn Future<Output = CoreResult<SecretValue>> + Send + '_>> {
            Box::pin(async {
                crate::failure::note(crate::failure::credential_missing());
                Err(CoreError::CredentialMissing)
            })
        }
    }

    #[tokio::test]
    async fn net_missing_endpoint_credential_note_names_the_endpoint() {
        let mut endpoint = endpoint("https://never-contact.invalid/v1");
        endpoint.credential = Some(CredentialRef::new());
        let (result, detail) = crate::failure::scope(async {
            NetClient::new(MissingKey)
                .unwrap()
                .get(
                    &endpoint,
                    "models",
                    Instant::now() + Duration::from_secs(5),
                    Cancellation::new(),
                )
                .await
        })
        .await;
        assert!(matches!(result, Err(CoreError::CredentialMissing)));
        let detail = detail.expect("credential note");
        assert_eq!(
            detail.target,
            crate::failure::FailureTarget::Inference {
                name: "test".into()
            }
        );
        assert_eq!(detail.step, crate::failure::FailureStep::ResolveCredential);
        assert_eq!(
            crate::failure::promote(CoreError::Authentication, Some(&detail)),
            CoreError::CredentialMissing
        );
    }

    #[tokio::test]
    async fn net_missing_mcp_header_credential_note_names_the_server() {
        let config = crate::model::core::ServerConfig {
            id: crate::model::core::ServerId::new(),
            name: "tools".into(),
            url: "https://never-contact.invalid/mcp".into(),
            allow_http: false,
            secret_headers: vec![("X-Api-Key".into(), CredentialRef::new())],
        };
        let (result, detail) = crate::failure::scope(async {
            NetClient::new(MissingKey)
                .unwrap()
                .mcp_request(
                    &config,
                    Method::POST,
                    Some(b"{}".to_vec()),
                    None,
                    None,
                    Instant::now() + Duration::from_secs(5),
                    &Cancellation::new(),
                )
                .await
                .map(|_| ())
        })
        .await;
        assert!(matches!(result, Err(CoreError::CredentialMissing)));
        let detail = detail.expect("credential note");
        assert_eq!(
            detail.target,
            crate::failure::FailureTarget::McpServer {
                name: "tools".into()
            }
        );
        assert_eq!(detail.step, crate::failure::FailureStep::ResolveCredential);
        assert_eq!(
            detail.reason.as_deref(),
            Some(crate::failure::CREDENTIAL_MISSING_REASON)
        );
    }

    #[tokio::test]
    async fn net_endpoint_error_body_echoing_key_is_redacted() {
        let (addr, server) = one_shot(
            "401 Unauthorized",
            r#"{"error":{"message":"bad key sk-test-key"}}"#,
        )
        .await;
        let mut endpoint = endpoint(&format!("http://{addr}/v1"));
        endpoint.allow_http = true;
        endpoint.credential = Some(CredentialRef::new());
        let (_, detail) = crate::failure::scope(async {
            NetClient::new(FixedKey("sk-test-key"))
                .unwrap()
                .get(
                    &endpoint,
                    "models",
                    Instant::now() + Duration::from_secs(5),
                    Cancellation::new(),
                )
                .await
        })
        .await;
        server.await.unwrap();
        assert_eq!(detail.unwrap().reason.as_deref(), Some("bad key •••"));
    }

    #[tokio::test]
    async fn net_endpoint_error_body_echoing_bare_token_of_scheme_credential_is_redacted() {
        let (addr, server) = one_shot(
            "401 Unauthorized",
            r#"{"error":{"message":"bad key sk-test-key"}}"#,
        )
        .await;
        let mut endpoint = endpoint(&format!("http://{addr}/v1"));
        endpoint.allow_http = true;
        endpoint.credential = Some(CredentialRef::new());
        let (_, detail) = crate::failure::scope(async {
            NetClient::new(FixedKey("Bearer sk-test-key"))
                .unwrap()
                .get(
                    &endpoint,
                    "models",
                    Instant::now() + Duration::from_secs(5),
                    Cancellation::new(),
                )
                .await
        })
        .await;
        server.await.unwrap();
        assert_eq!(detail.unwrap().reason.as_deref(), Some("bad key •••"));
    }

    #[tokio::test(start_paused = true)]
    async fn net_stalled_error_body_keeps_status_mapping() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 8192];
            let _ = socket.read(&mut request).await.unwrap();
            socket
                .write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 100\r\n\r\n")
                .await
                .unwrap();
            let _ = socket.read(&mut request).await;
        });
        let mut endpoint = endpoint(&format!("http://{addr}/v1"));
        endpoint.allow_http = true;
        let (result, detail) = crate::failure::scope(async {
            NetClient::new(FixedKey("unused"))
                .unwrap()
                .get(
                    &endpoint,
                    "models",
                    Instant::now() + Duration::from_secs(5),
                    Cancellation::new(),
                )
                .await
        })
        .await;
        assert!(matches!(result, Err(CoreError::Authentication)));
        let detail = detail.unwrap();
        assert_eq!(detail.status, Some(401));
        assert_eq!(detail.reason, None);
        server.abort();
    }

    #[tokio::test]
    async fn net_endpoint_credential_with_scheme_is_not_double_prefixed() {
        let (addr, server) = one_shot("200 OK", r#"{"data":[]}"#).await;
        let mut endpoint = endpoint(&format!("http://{addr}/v1"));
        endpoint.allow_http = true;
        endpoint.credential = Some(CredentialRef::new());
        let _ = NetClient::new(FixedKey("Bearer sk-test-key"))
            .unwrap()
            .get(
                &endpoint,
                "models",
                Instant::now() + Duration::from_secs(5),
                Cancellation::new(),
            )
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
                .get(
                    &endpoint,
                    "models",
                    Instant::now() + Duration::from_secs(5),
                    Cancellation::new(),
                )
                .await
        })
        .await;
        assert!(matches!(result, Err(CoreError::UnavailableTransport)));
        let detail = detail.unwrap();
        assert_eq!(detail.status, None);
        assert_eq!(detail.reason.as_deref(), Some("connection refused"));
    }

    #[tokio::test]
    async fn net_mcp_authorization_header_rule_and_other_headers_verbatim() {
        for (stored, expected) in [
            ("okf_abc", "authorization: bearer okf_abc\r\n"),
            ("Bearer okf_abc", "authorization: bearer okf_abc\r\n"),
        ] {
            for header_name in ["Authorization", "authorization"] {
                let (addr, server) = one_shot("200 OK", "{}").await;
                let config = crate::model::core::ServerConfig {
                    id: crate::model::core::ServerId::new(),
                    name: "fixture".into(),
                    url: format!("http://{addr}/mcp"),
                    allow_http: true,
                    secret_headers: vec![(header_name.into(), CredentialRef::new())],
                };
                NetClient::new(FixedKey(stored))
                    .unwrap()
                    .mcp_request(
                        &config,
                        Method::POST,
                        Some(b"{}".to_vec()),
                        None,
                        None,
                        Instant::now() + Duration::from_secs(5),
                        &Cancellation::new(),
                    )
                    .await
                    .unwrap();
                let request = server.await.unwrap().to_ascii_lowercase();
                assert!(request.contains(expected), "{request}");
                assert!(!request.contains("bearer bearer"));
            }
        }
        let (addr, server) = one_shot("200 OK", "{}").await;
        let config = crate::model::core::ServerConfig {
            id: crate::model::core::ServerId::new(),
            name: "fixture".into(),
            url: format!("http://{addr}/mcp"),
            allow_http: true,
            secret_headers: vec![("X-Api-Key".into(), CredentialRef::new())],
        };
        NetClient::new(FixedKey("k1"))
            .unwrap()
            .mcp_request(
                &config,
                Method::POST,
                Some(b"{}".to_vec()),
                None,
                None,
                Instant::now() + Duration::from_secs(5),
                &Cancellation::new(),
            )
            .await
            .unwrap();
        let request = server.await.unwrap().to_ascii_lowercase();
        assert!(request.contains("x-api-key: k1\r\n"), "{request}");
    }

    #[tokio::test]
    async fn net_mcp_connect_failure_notes_transport_reason() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let config = crate::model::core::ServerConfig {
            id: crate::model::core::ServerId::new(),
            name: "fixture".into(),
            url: format!("http://{addr}/mcp"),
            allow_http: true,
            secret_headers: vec![],
        };
        let (result, detail) = crate::failure::scope(async {
            NetClient::new(FixedKey("unused"))
                .unwrap()
                .mcp_request(
                    &config,
                    Method::POST,
                    Some(b"{}".to_vec()),
                    None,
                    None,
                    Instant::now() + Duration::from_secs(5),
                    &Cancellation::new(),
                )
                .await
        })
        .await;
        assert!(matches!(result, Err(CoreError::UnavailableTransport)));
        let detail = detail.unwrap();
        assert_eq!(
            detail.target,
            crate::failure::FailureTarget::McpServer {
                name: "fixture".into()
            }
        );
        assert_eq!(detail.reason.as_deref(), Some("connection refused"));
    }
}
