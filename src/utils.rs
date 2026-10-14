use crate::command_parser::LogLevel;
use crate::rustls_config::create_dangerous_config;
use log::Level;
use std::fmt::Display;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::{
    Connector, MaybeTlsStream, WebSocketStream, connect_async, connect_async_tls_with_config,
};
use url::{ParseError, Url};

pub fn init_logger(log_level: &LogLevel) {
    #[cfg(target_os = "windows")]
    simple_logger::set_up_windows_color_terminal();

    match log_level {
        LogLevel::Error => simple_logger::init_with_level(Level::Error).unwrap(),
        LogLevel::Warn => simple_logger::init_with_level(Level::Warn).unwrap(),
        LogLevel::Info => simple_logger::init_with_level(Level::Info).unwrap(),
        LogLevel::Debug => simple_logger::init_with_level(Level::Debug).unwrap(),
        LogLevel::Trace => simple_logger::init_with_level(Level::Trace).unwrap(),
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionUrls {
    /// v2 JSON-RPC HTTP endpoint (also used for task/ping result callbacks).
    pub rpc_http: String,
    /// v2 JSON-RPC WebSocket endpoint.
    pub rpc_ws: String,
    pub ws_terminal: String,
}

impl Display for ConnectionUrls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Connection URLs:")?;
        // Tokens are query parameters; omit queries from diagnostic output.
        writeln!(
            f,
            "  JSON-RPC HTTP URL: {}",
            self.rpc_http.split('?').next().unwrap_or_default()
        )?;
        writeln!(
            f,
            "  JSON-RPC WebSocket URL: {}",
            self.rpc_ws.split('?').next().unwrap_or_default()
        )?;
        writeln!(
            f,
            "  WebSocket Terminal URL: {}",
            self.ws_terminal.split('?').next().unwrap_or_default()
        )
    }
}

pub fn build_urls(
    http_server: &str,
    ws_server: Option<&String>,
    token: &str,
) -> Result<ConnectionUrls, ParseError> {
    // 1. Construct http_url_base
    let http_url = Url::parse(http_server)?;

    // 2. Construct ws_url_base
    let ws_url = if let Some(ws) = ws_server {
        Url::parse(ws)?
    } else {
        let mut ws_url = http_url.clone();
        match ws_url.scheme() {
            "http" => ws_url.set_scheme("ws").unwrap(),
            "https" => ws_url.set_scheme("wss").unwrap(),
            _ => return Err(ParseError::RelativeUrlWithoutBase),
        }
        ws_url
    };
    // 3. Construct final URLs
    let endpoint = |mut base: Url, path: &str| {
        base.set_path(&format!("{}/{path}", base.path().trim_end_matches('/')));
        base.set_query(None);
        base.set_fragment(None);
        base.query_pairs_mut().append_pair("token", token);
        base.to_string()
    };
    let rpc_http_url = endpoint(http_url, "api/clients/v2/rpc");
    let ws_terminal_url = endpoint(ws_url.clone(), "api/clients/terminal");
    let rpc_ws_url = endpoint(ws_url, "api/clients/v2/rpc");

    let connection_urls = ConnectionUrls {
        rpc_http: rpc_http_url,
        rpc_ws: rpc_ws_url,
        ws_terminal: ws_terminal_url,
    };

    Ok(connection_urls)
}

/// Send a JSON-RPC request over HTTP. The optional client backends are kept
/// behind the existing feature flags used by the release builds.
pub async fn post_json_async(
    url: String,
    body: String,
    disable_verification: bool,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || post_json(&url, &body, disable_verification))
        .await
        .map_err(|e| format!("HTTP worker failed: {e}"))?
}

pub fn post_json(url: &str, body: &str, disable_verification: bool) -> Result<String, String> {
    #[cfg(feature = "ureq-support")]
    {
        let agent = create_ureq_agent(disable_verification);
        let response = agent
            .post(url)
            .header("Content-Type", "application/json")
            .send(body)
            .map_err(|e| format!("HTTP request failed: {e}"))?;
        let status = response.status();
        let text = response
            .into_body()
            .read_to_string()
            .map_err(|e| format!("Failed reading HTTP response: {e}"))?;
        if status.is_success() {
            crate::protocol::parse_response(&text)?;
            Ok(text)
        } else {
            Err(format!("Server returned HTTP status {status}"))
        }
    }

    #[cfg(all(feature = "nyquest-support", not(feature = "ureq-support")))]
    {
        use nyquest::{Body, Request};
        let client = create_nyquest_client(disable_verification);
        let url = url.to_string();
        let response = client
            .request(Request::post(url).with_body(Body::text(body.to_string(), "application/json")))
            .map_err(|e| format!("HTTP request failed: {e}"))?;
        if response.status().is_successful() {
            let text = response
                .text()
                .map_err(|e| format!("Failed reading HTTP response: {e}"))?;
            crate::protocol::parse_response(&text)?;
            Ok(text)
        } else {
            Err(format!("Server returned HTTP status {}", response.status()))
        }
    }

    #[cfg(not(any(feature = "ureq-support", feature = "nyquest-support")))]
    {
        let _ = (url, body, disable_verification);
        Err("No HTTP client backend enabled; build with `ureq-support` or `nyquest-support`".into())
    }
}

pub async fn connect_ws(
    url: &str,
    tls: bool,
    skip_verify: bool,
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, String> {
    let connection_timeout = Duration::from_secs(10);

    if tls || url.starts_with("wss://") {
        if skip_verify {
            timeout(
                connection_timeout,
                connect_async_tls_with_config(
                    url,
                    None,
                    false,
                    Some(Connector::Rustls(Arc::new(create_dangerous_config()))),
                ),
            )
            .await
            .map_err(|_| "WebSocket connection timeout".to_string())?
            .map(|ws| ws.0)
            .map_err(|_| "Failed to establish WebSocket connection".to_string())
        } else {
            timeout(
                connection_timeout,
                connect_async_tls_with_config(url, None, false, None),
            )
            .await
            .map_err(|_| "WebSocket connection timeout".to_string())?
            .map(|ws| ws.0)
            .map_err(|_| "Failed to establish WebSocket connection".into())
        }
    } else {
        timeout(connection_timeout, connect_async(url))
            .await
            .map_err(|_| "WebSocket connection timeout".to_string())?
            .map(|ws| ws.0)
            .map_err(|_| "Failed to establish WebSocket connection".to_string())
    }
}

#[cfg(feature = "ureq-support")]
pub fn create_ureq_agent(disable_verification: bool) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .disable_verification(disable_verification)
                .build(),
        )
        .timeout_global(Some(Duration::from_secs(5)))
        .build();
    config.new_agent()
}

#[cfg(feature = "nyquest-support")]
pub fn create_nyquest_client(disable_verification: bool) -> nyquest::BlockingClient {
    use std::time::Duration;
    static REGISTER: std::sync::Once = std::sync::Once::new();
    REGISTER.call_once(|| {
        #[cfg(target_os = "linux")]
        nyquest_backend_curl::register();
        #[cfg(not(target_os = "linux"))]
        nyquest_preset::register();
    });
    let mut client = nyquest::ClientBuilder::default()
        .request_timeout(Duration::from_secs(5))
        .user_agent("curl/8.7.1");
    if disable_verification {
        client = client.dangerously_ignore_certificate_errors();
    }
    client.build_blocking().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_preserves_subpath_and_encodes_token() {
        let urls = build_urls("https://example.test/komari/", None, "a+b&c#d").unwrap();
        let url = Url::parse(&urls.rpc_http).unwrap();
        assert_eq!(url.path(), "/komari/api/clients/v2/rpc");
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            vec![("token".into(), "a+b&c#d".into())]
        );
    }

    #[test]
    #[cfg(any(feature = "ureq-support", feature = "nyquest-support"))]
    fn post_rejects_json_rpc_error_and_html_success_responses() {
        use std::io::{Read, Write};
        for (body, expected) in [
            (
                r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32602,"message":"invalid report params"}}"#,
                "JSON-RPC error -32602",
            ),
            ("<html>proxy login page</html>", "Invalid JSON-RPC response"),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!(
                "http://{}/api/clients/v2/rpc",
                listener.local_addr().unwrap()
            );
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut buf = [0; 4096];
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n{}") {
                    let n = socket.read(&mut buf).unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&buf[..n]);
                }
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            });
            let result = post_json(&url, "{}", false);
            server.join().unwrap();
            assert!(result.unwrap_err().contains(expected));
        }
    }

    #[test]
    fn build_urls_uses_v2_rpc_for_http_and_websocket() {
        let urls = build_urls("https://example.test/", None, "abc").unwrap();
        assert_eq!(
            urls.rpc_http,
            "https://example.test/api/clients/v2/rpc?token=abc"
        );
        assert_eq!(
            urls.rpc_ws,
            "wss://example.test/api/clients/v2/rpc?token=abc"
        );
        assert_eq!(
            urls.ws_terminal,
            "wss://example.test/api/clients/terminal?token=abc"
        );
    }
}
