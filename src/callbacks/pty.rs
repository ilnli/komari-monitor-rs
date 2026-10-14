use futures::{SinkExt, StreamExt};
use log::{error, info};
use miniserde::{Deserialize, Serialize};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use tokio::{sync::mpsc, task};
use tokio_tungstenite::tungstenite::Bytes;
use tokio_tungstenite::{WebSocketStream, tungstenite::protocol::Message};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TerminalEvent {
    message: String,
    request_id: String,
}

pub fn get_pty_ws_link(utf8_str: &str, ws_terminal_url: &str) -> Result<String, String> {
    let request_id = match miniserde::json::from_str::<TerminalEvent>(utf8_str) {
        Ok(event) => event.request_id,
        Err(_) => {
            let rpc: crate::protocol::IncomingRequest = miniserde::json::from_str(utf8_str)
                .map_err(|_| "Failed to parse TerminalEvent".to_string())?;
            rpc.params
                .and_then(|params| params.request_id)
                .ok_or_else(|| "Terminal request is missing request_id".to_string())?
        }
    };

    Ok(format!(
        "{ws_terminal_url}&id={request_id}",
        request_id = request_id
    ))
}

pub async fn handle_pty_session<S>(ws_stream: WebSocketStream<S>, cmd: &str) -> Result<(), String>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let pty_system = NativePtySystem::default();

    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to create PTY: {e}"))?;

    let mut cmd = CommandBuilder::new(cmd);

    if !cfg!(windows) {
        cmd.env("TERM", "xterm-256color");
        cmd.env("LANG", "C.UTF-8");
        cmd.env("LC_ALL", "C.UTF-8");
    }

    let mut pty_reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("Failed to get PTY Reader: {e}"))?;
    let pty_writer = Arc::new(Mutex::new(
        pair.master
            .take_writer()
            .map_err(|e| format!("Failed to get PTY Writer: {e}"))?,
    ));

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("Failed to spawn process: {e}"))?;

    info!("Terminal started in PTY, PID: {:?}", child.process_id());

    let (ws_sender, mut ws_receiver) = ws_stream.split();
    let (pty_to_ws_tx, mut pty_to_ws_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    task::spawn_blocking(move || {
        let mut buffer = [0u8; 8192];
        loop {
            match pty_reader.read(&mut buffer) {
                Ok(count) if count > 0 => {
                    if pty_to_ws_tx.send(buffer[..count].to_vec()).is_err() {
                        info!("PTY reader: WebSocket side closed, stopping read.");
                        break;
                    }
                }
                Ok(_) | Err(_) => {
                    info!("PTY reader: PTY closed, stopping read.");
                    break;
                }
            }
        }
    });

    let pty_to_ws_task = tokio::spawn(async move {
        let mut ws_sender = ws_sender;
        while let Some(data) = pty_to_ws_rx.recv().await {
            if ws_sender
                .send(Message::Binary(Bytes::from(data)))
                .await
                .is_err()
            {
                error!("Failed to send data to WebSocket");
                break;
            }
        }
    });

    let ws_to_pty_task = tokio::spawn(async move {
        while let Some(result) = ws_receiver.next().await {
            match result {
                Ok(msg) => match handle_ws_message(msg, &pty_writer) {
                    Err(e) => {
                        error!("Failed to handle WebSocket message: {e}");
                        break;
                    }
                    Ok(Some(resize)) => {
                        if let Err(e) = pair.master.resize(PtySize {
                            rows: resize.rows,
                            cols: resize.cols,
                            pixel_width: 0,
                            pixel_height: 0,
                        }) {
                            error!("Failed to resize PTY: {e}");
                        }
                    }
                    _ => {}
                },
                Err(e) => {
                    error!("Error receiving message from WebSocket: {e}");
                    break;
                }
            }
        }
    });

    tokio::select! {
        _ = pty_to_ws_task => info!("PTY -> WebSocket task finished."),
        _ = ws_to_pty_task => info!("WebSocket -> PTY task finished."),
    }

    info!("Closing session, terminating child process...");
    if let Err(e) = child.kill() {
        error!("Failed to terminate child process: {e}");
    }
    child
        .wait()
        .map_err(|e| format!("Failed to wait for child process: {e}"))?;
    info!("Session successfully closed.");

    Ok(())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct NeedResize {
    #[serde(rename = "type")]
    type_str: String,
    cols: u16,
    rows: u16,
}

fn handle_ws_message(
    msg: Message,
    pty_writer: &Arc<Mutex<Box<dyn Write + Send>>>,
) -> Result<Option<NeedResize>, String> {
    #[derive(Serialize, Deserialize, Debug, Clone)]
    struct HeartBeat {
        #[serde(rename = "type")]
        type_str: String,
        timestamp: String,
    }

    match msg {
        Message::Text(text) => {
            if miniserde::json::from_str::<HeartBeat>(text.as_ref()).is_ok() {
                return Ok(None);
            }
            if let Ok(resize) = miniserde::json::from_str::<NeedResize>(text.as_ref()) {
                return Ok(Some(resize));
            }
            pty_writer
                .lock()
                .unwrap()
                .write_all(text.as_bytes())
                .map_err(|e| format!("Failed to write to PTY: {e}"))?;
        }
        Message::Binary(data) => {
            pty_writer
                .lock()
                .unwrap()
                .write_all(&data)
                .map_err(|e| format!("Failed to write to PTY: {e}"))?;
        }
        Message::Close(_) => {
            return Err(String::from("WebSocket connection closed"));
        }
        _ => {}
    }
    Ok(None)
}
