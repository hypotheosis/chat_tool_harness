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

fn bearer_header(secret: &SecretValue) -> CoreResult<header::HeaderValue> {
    let mut value = header::HeaderValue::from_str(&format!("Bearer {}", secret.as_str()))
        .map_err(|_| CoreError::Configuration)?;
    value.set_sensitive(true);
    Ok(value)
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
        let url = Url::parse(&server.url).map_err(|_| CoreError::InvalidInput)?;
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
        for (name, reference) in custom {
            let secret = tokio::select! {
                _ = cancel.cancelled() => return Err(CoreError::Cancelled),
                result = tokio::time::timeout_at(deadline, self.provider.resolve(reference)) => result.map_err(|_| CoreError::Cancelled)??,
            };
            let mut value = header::HeaderValue::from_str(secret.as_str())
                .map_err(|_| CoreError::Configuration)?;
            value.set_sensitive(true);
            builder = builder.header(name, value);
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
                Ok(Err(_)) | Ok(Ok(Err(_))) => Err(CoreError::UnavailableTransport),
                Ok(Ok(Ok(response))) => Ok(response),
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
        let mut builder = self
            .client
            .request(method, url)
            .header(header::ACCEPT, "application/json, text/event-stream");
        if let Some(reference) = endpoint.credential {
            let secret = tokio::select! {
                _ = cancel.cancelled() => return Err(CoreError::Cancelled),
                result = tokio::time::timeout_at(deadline, self.provider.resolve(reference)) => result.map_err(|_| CoreError::Cancelled)??,
            };
            builder = builder.header(header::AUTHORIZATION, bearer_header(&secret)?);
        }
        if let Some(body) = body {
            builder = builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(body);
        }
        let response = tokio::select! {
            _ = cancel.cancelled() => return Err(CoreError::Cancelled),
            result = tokio::time::timeout_at(deadline, tokio::time::timeout(IDLE_TIMEOUT, builder.send())) => {
                match result {
                    Err(_) => return Err(CoreError::Cancelled),
                    Ok(Err(_)) => return Err(CoreError::UnavailableTransport),
                    Ok(Ok(Err(_))) => return Err(CoreError::UnavailableTransport),
                    Ok(Ok(Ok(response))) => response,
                }
            },
        };
        match response.status().as_u16() {
            200..=299 => Ok(response),
            401 | 403 => Err(CoreError::Authentication),
            429 => Err(CoreError::RateLimited),
            400 | 422 if suffix == "chat/completions" => {
                let mut response = response;
                let mut body = Vec::new();
                loop {
                    let chunk = match next_chunk(&mut response, deadline, cancel, 4096 - body.len())
                        .await
                    {
                        Err(CoreError::Limit) => return Err(CoreError::Protocol),
                        other => other?,
                    };
                    let Some(chunk) = chunk else {
                        break;
                    };
                    body.extend_from_slice(&chunk);
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
            assert!(request.contains("authorization: synthetic-key"));
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
        let value = bearer_header(&SecretValue::new("highly-private".into())).unwrap();
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
}
