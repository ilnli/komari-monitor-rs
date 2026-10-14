use crate::callbacks::exec::{SharedExecCache, exec_command};
use crate::callbacks::ping::ping_target;
use crate::callbacks::pty::{get_pty_ws_link, handle_pty_session};
use crate::command_parser::Args;
use crate::protocol::{
    self, AGENT_EVENT, AGENT_EXEC, AGENT_MESSAGE, AGENT_PING, AGENT_TERMINAL_REQUEST,
    IncomingRequest, RpcResponse,
};
use crate::utils::{ConnectionUrls, connect_ws};
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use log::{error, info};
use miniserde::{Deserialize, Serialize, json};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

pub mod exec;
pub mod ping;
pub mod pty;

#[derive(Serialize, Deserialize)]
struct LegacyMsg {
    message: String,
}

type Reader = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;
type LockedWriter = Arc<Mutex<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>>;

/// Process events returned in a v2 POST response. POST fallback cannot open a
/// server-initiated WebSocket, so task and ping results are acknowledged via
/// the same HTTP JSON-RPC endpoint.
pub async fn handle_http_events(
    args: &Args,
    connection_urls: &ConnectionUrls,
    body: &str,
    processed: &mut VecDeque<String>,
    exec_cache: &SharedExecCache,
) -> Vec<String> {
    let Ok(response) = json::from_str::<RpcResponse>(body) else {
        return Vec::new();
    };
    let Some(result) = response.result else {
        return Vec::new();
    };
    let mut processed_ids = Vec::new();
    for event in result.events.unwrap_or_default() {
        if processed.contains(&event.id) {
            processed_ids.push(event.id);
            continue;
        }
        let request = IncomingRequest {
            jsonrpc: Some(protocol::JSONRPC_VERSION.to_string()),
            method: event.method.clone(),
            params: event.params,
            id: None,
            message: None,
        };
        let event_json = json::to_string(&request);
        let mut completed = true;
        match event.method.as_str() {
            AGENT_EXEC if args.terminal => {
                if let Err(e) = exec_command(
                    &event_json,
                    connection_urls.rpc_http.clone(),
                    &args.ignore_unsafe_cert,
                    exec_cache,
                )
                .await
                {
                    error!("Exec Error: {e}");
                    completed = false;
                }
            }
            AGENT_PING => match ping_target(&event_json).await {
                Ok(result) => {
                    let payload = protocol::ping_result(protocol::PingResultParams {
                        task_id: result.task_id,
                        ping_type: result.ping_type,
                        value: result.value.unwrap_or(-1),
                        finished_at: result.finished_at,
                    });
                    if let Err(e) = crate::utils::post_json_async(
                        connection_urls.rpc_http.clone(),
                        payload,
                        args.ignore_unsafe_cert,
                    )
                    .await
                    {
                        error!("Ping result upload failed: {e}");
                        completed = false;
                    }
                }
                Err(e) => {
                    error!("Ping Error: {e}");
                    completed = false;
                }
            },
            AGENT_TERMINAL_REQUEST => {
                error!("Terminal requests are unavailable in POST fallback mode");
            }
            AGENT_MESSAGE | AGENT_EVENT => {
                info!("Received server event {}", event.method);
            }
            _ => {}
        }
        if completed {
            processed.push_back(event.id.clone());
            // The server queues at most 128 events; retain recent IDs for replay.
            while processed.len() > 256 {
                processed.pop_front();
            }
            processed_ids.push(event.id);
        }
    }
    processed_ids
}

pub async fn handle_callbacks(
    args: &Args,
    connection_urls: &ConnectionUrls,
    reader: &mut Reader,
    locked_writer: &LockedWriter,
    exec_cache: &SharedExecCache,
) -> () {
    while let Some(msg) = reader.next().await {
        let msg = match msg {
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(payload)) => {
                if locked_writer
                    .lock()
                    .await
                    .send(Message::Pong(payload))
                    .await
                    .is_err()
                {
                    break;
                }
                continue;
            }
            Ok(Message::Pong(_)) => continue,
            Ok(msg) => msg,
            Err(e) => {
                error!("WebSocket receive error: {e}");
                break;
            }
        };

        let Ok(utf8) = msg.into_text() else {
            continue;
        };

        info!("Received message from main server: {}", utf8.as_str());

        if let Ok(response) = json::from_str::<RpcResponse>(utf8.as_str()) {
            if let Some(error) = response.error {
                error!("Server JSON-RPC error {}: {}", error.code, error.message);
                continue;
            }
        }

        let rpc = json::from_str::<IncomingRequest>(utf8.as_str());
        let (method, legacy_message) = match rpc {
            Ok(value) => (value.method, value.message.unwrap_or_default()),
            Err(_) => match json::from_str::<LegacyMsg>(utf8.as_str()) {
                Ok(value) => (value.message.clone(), value.message),
                Err(_) => continue,
            },
        };

        let utf8_cloned = utf8.clone();

        let method = match method.as_str() {
            "exec" => AGENT_EXEC,
            "ping" => AGENT_PING,
            "terminal" => AGENT_TERMINAL_REQUEST,
            other => other,
        };

        match method {
            AGENT_EXEC => {
                if args.terminal {
                    tokio::spawn({
                        let utf8_cloned_for_exec = utf8_cloned.clone();
                        let exec_callback_url = connection_urls.rpc_http.clone();
                        let ignore_unsafe_cert = args.ignore_unsafe_cert;
                        let exec_cache = exec_cache.clone();

                        async move {
                            if let Err(e) = exec_command(
                                &utf8_cloned_for_exec,
                                exec_callback_url,
                                &ignore_unsafe_cert,
                                &exec_cache,
                            )
                            .await
                            {
                                error!("Exec Error: {e}");
                            }
                        }
                    });
                } else {
                    error!("Terminal feature is disabled");
                }
            }

            AGENT_PING => {
                let locked_write_for_ping = locked_writer.clone();
                tokio::spawn(async move {
                    match ping_target(&utf8_cloned).await {
                        Ok(json_res) => {
                            let mut write = locked_write_for_ping.lock().await;
                            info!("Ping successful: {}", json::to_string(&json_res));
                            let payload = protocol::ping_result(protocol::PingResultParams {
                                task_id: json_res.task_id,
                                ping_type: json_res.ping_type.clone(),
                                value: json_res.value.unwrap_or(-1),
                                finished_at: json_res.finished_at.clone(),
                            });
                            if let Err(e) =
                                write.send(Message::Text(Utf8Bytes::from(payload))).await
                            {
                                error!(
                                    "Error occurred while pushing ping result, attempting to reconnect: {e}"
                                );
                            }
                        }
                        Err(err) => {
                            error!("Ping Error: {err}");
                        }
                    }
                });
            }

            AGENT_TERMINAL_REQUEST => {
                if args.terminal {
                    let ws_terminal_url = connection_urls.clone().ws_terminal.clone();
                    let args = args.clone();
                    let utf8_cloned = utf8_cloned.clone();

                    tokio::spawn(async move {
                        let ws_url = match get_pty_ws_link(&utf8_cloned, &ws_terminal_url) {
                            Ok(ws_url) => ws_url,
                            Err(e) => {
                                error!("Failed to get PTY WebSocket URL: {e}");
                                return;
                            }
                        };

                        let ws_stream =
                            match connect_ws(&ws_url, args.tls, args.ignore_unsafe_cert).await {
                                Ok(ws_stream) => ws_stream,
                                Err(e) => {
                                    error!("Failed to connect to PTY WebSocket: {e}");
                                    return;
                                }
                            };

                        if let Err(e) = handle_pty_session(ws_stream, &args.terminal_entry).await {
                            error!("PTY WebSocket handling error: {e}");
                        }
                    });
                } else {
                    error!("Terminal feature is disabled");
                }
            }
            AGENT_MESSAGE | AGENT_EVENT => {
                info!("Received server event {method}");
            }
            _ => {
                if !legacy_message.is_empty() {
                    info!("Ignoring unsupported server message: {legacy_message}");
                }
            }
        }
    }
}
