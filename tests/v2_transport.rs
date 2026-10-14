//! Exercise the real binary against the official v2 wire format.
#![cfg(any(feature = "ureq-support", feature = "nyquest-support"))]

use futures::{SinkExt, StreamExt};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

struct Child(std::process::Child);

#[test]
fn reports_package_version_without_starting_monitor() {
    let output = Command::new(env!("CARGO_BIN_EXE_komari-monitor-rs"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        concat!("komari-monitor-rs ", env!("CARGO_PKG_VERSION"))
    );
}

impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn fallback_reports_continue_during_ws_recovery_then_switch_to_ws() {
    let http = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    http.set_nonblocking(true).unwrap();
    let http_address = http.local_addr().unwrap();
    let ws = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ws_address = ws.local_addr().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_server = stop.clone();
    let server = std::thread::spawn(move || {
        let mut reports = 0;
        let mut failed_callback = false;
        while !stop_server.load(Ordering::Relaxed) {
            let Ok((mut socket, _)) = http.accept() else {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut buf = [0; 8192];
            let mut is_report = false;
            let mut reject_callback = false;
            loop {
                let n = socket.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..n]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let length: usize = headers
                        .lines()
                        .find_map(|l| {
                            let (name, value) = l.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse().unwrap())
                        })
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        assert!(headers.starts_with("POST /api/clients/v2/rpc?token=test "));
                        let body =
                            String::from_utf8(request[end + 4..end + 4 + length].to_vec()).unwrap();
                        is_report = body.contains("agent.report");
                        if body.contains("fallback-task") && !failed_callback {
                            failed_callback = true;
                            reject_callback = true;
                        }
                        tx.send((Instant::now(), body)).unwrap();
                        break;
                    }
                }
            }
            if is_report {
                reports += 1;
            }
            let event = r#"{"id":"evt-1","method":"agent.exec","params":{"task_id":"fallback-task","command":"echo fallback-ok"}}"#;
            let body = if reject_callback {
                r#"{"jsonrpc":"2.0","error":{"code":-32000,"message":"temporary save failure"}}"#
                    .into()
            } else if is_report && reports <= 3 {
                format!(
                    r#"{{"jsonrpc":"2.0","result":{{"status":"success","events":[{event},{event}]}}}}"#
                )
            } else {
                r#"{"jsonrpc":"2.0","result":{"status":"success","events":[]}}"#.into()
            };
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });

    let ready = Arc::new(AtomicBool::new(false));
    let ws_ready = ready.clone();
    let ws_server = tokio::spawn(async move {
        loop {
            let (mut socket, _) = ws.accept().await.unwrap();
            if !ws_ready.load(Ordering::Relaxed) {
                // Slow failed upgrades must not pause HTTP reports.
                tokio::time::sleep(Duration::from_millis(700)).await;
                use tokio::io::AsyncWriteExt;
                let _ = socket.write_all(b"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
                continue;
            }
            let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
            let message = socket.next().await.unwrap().unwrap().into_text().unwrap();
            assert!(message.contains("\"method\":\"agent.report\""));
            assert!(message.contains("\"report\":{\"cpu\""));
            socket.send(Message::Text(format!(r#"{{"jsonrpc":"2.0","id":42,"method":"agent.ping","params":{{"ping_task_id":7,"ping_type":"tcp","ping_target":"{ws_address}"}}}}"#).into())).await.unwrap();
            loop {
                let message = socket.next().await.unwrap().unwrap().into_text().unwrap();
                if message.contains("agent.pingResult") {
                    assert!(message.contains("\"task_id\":7"));
                    break;
                }
            }
            socket.send(Message::Text(r#"{"jsonrpc":"2.0","method":"agent.exec","params":{"task_id":"test-task","command":"echo komari-v2"}}"#.into())).await.unwrap();
            // The official server can replay unacknowledged tasks on WS recovery.
            socket.send(Message::Text(r#"{"jsonrpc":"2.0","method":"agent.exec","params":{"task_id":"fallback-task","command":"echo fallback-ok"}}"#.into())).await.unwrap();
            tokio::time::sleep(Duration::from_millis(500)).await;
            socket.close(None).await.unwrap();
            break;
        }
    });

    let mut child = Child(
        Command::new(env!("CARGO_BIN_EXE_komari-monitor-rs"))
            .args([
                "--endpoint",
                &format!("http://{http_address}"),
                "--ws-server",
                &format!("ws://{ws_address}"),
                "--token",
                "test",
                "--max-retries",
                "1",
                "--reconnect-interval",
                "1",
                "--interval",
                "0.1",
                "--terminal",
                "--disable-network-statistics",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );

    let outcome = tokio::time::timeout(Duration::from_secs(30), async {
        let mut reports = Vec::new();
        let mut basic = false;
        let mut fallback_results = Vec::new();
        let mut acknowledged = false;
        while reports.len() < 4 {
            let (at, body) = rx.recv().await.unwrap();
            if body.contains("agent.basicInfo") {
                basic = body.contains("\"info\":");
            }
            if body.contains("fallback-task") {
                fallback_results.push(body.clone());
            }
            if body.contains("agent.report") {
                reports.push(at);
                acknowledged |= body.contains("evt-1");
            }
        }
        assert_eq!(
            fallback_results.len(),
            2,
            "failed callback must be retried once"
        );
        assert_eq!(
            fallback_results[0], fallback_results[1],
            "callback retry re-executed command instead of reusing its result"
        );
        assert!(acknowledged, "completed event was not acknowledged");
        let gap = reports[3].duration_since(reports[0]);
        assert!(
            gap < Duration::from_millis(1200),
            "HTTP sampling stalled during WS retries: {gap:?}"
        );
        ready.store(true, Ordering::Relaxed);
        let mut ws_task = false;
        let mut replayed = false;
        while !ws_task || !replayed || !basic {
            let (_, body) = rx.recv().await.unwrap();
            if body.contains("agent.basicInfo") {
                basic = body.contains("\"info\":");
            }
            if body.contains("fallback-task") {
                assert_eq!(
                    body, fallback_results[0],
                    "WS recovery re-executed completed HTTP task"
                );
                replayed = true;
            }
            if body.contains("test-task") {
                assert!(body.contains("komari-v2"));
                assert!(body.contains("\"exit_code\":0"));
                ws_task = true;
            }
        }
        child.0.try_wait().unwrap().is_none()
    })
    .await;
    drop(child);
    ws_server.abort();
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();
    assert!(outcome.expect("v2 transport timed out"));
}
