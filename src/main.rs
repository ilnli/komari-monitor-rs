#![warn(clippy::all, clippy::pedantic)]
#![allow(
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::similar_names,
    clippy::too_many_lines
)]

use crate::callbacks::{handle_callbacks, handle_http_events};
use crate::command_parser::Args;
use crate::data_struct::{BasicInfo, RealTimeInfo};
use crate::dry_run::dry_run;
use crate::get_info::network::network_saver::network_saver;
use crate::utils::{build_urls, connect_ws, init_logger, post_json_async};
use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt};
use log::{debug, error, info};
use std::process::exit;
use std::sync::Arc;
use std::time::Duration;
use sysinfo::{CpuRefreshKind, DiskRefreshKind, Disks, MemoryRefreshKind, Networks, RefreshKind};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, mpsc};
use tokio::time::{Instant, MissedTickBehavior, sleep, timeout};
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

mod callbacks;
mod command_parser;
mod data_struct;
mod dry_run;
mod get_info;
mod protocol;
mod rustls_config;
mod utils;

#[tokio::main]
async fn main() {
    let args = Args::par();

    init_logger(&args.log_level);

    dry_run().await;

    if args.dry_run {
        exit(0);
    }

    let network_config = args.network_config();

    let (http_server, token) = match (args.http_server.clone(), args.token.clone()) {
        (Some(http_server), Some(token)) => (http_server, token),
        (_, _) => {
            error!(
                "The `--endpoint` (or `--http-server`) and `--token` parameters must be specified."
            );
            exit(1);
        }
    };

    for line in args.to_string().lines() {
        debug!("{line}");
    }

    let connection_urls = build_urls(
        http_server.as_ref(),
        args.ws_server.as_ref(),
        token.as_ref(),
    )
    .unwrap_or_else(|e| {
        error!("Failed to parse server address: {e}");
        exit(1);
    });

    for line in connection_urls.to_string().lines() {
        debug!("{line}");
    }

    #[cfg(target_os = "windows")]
    {
        if !args.disable_toast_notify {
            use win_toast_notify::{Action, ActivationType, WinToastNotify};
            WinToastNotify::new()
                .set_title("Komari-monitor-rs Is Running!")
                .set_messages(vec![
                    "Komari-monitor-rs is an application used to monitor your system, granting it near-complete access to your computer. If you did not actively install this program, please check your system immediately. If you have intentionally used this software on your system, please ignore this message or add `--disable-toast-notify` to your startup parameters."
                ])
                .set_actions(vec![
                    Action {
                        activation_type: ActivationType::Protocol,
                        action_content: "komari-monitor".to_string(),
                        arguments: "https://github.com/komari-monitor".to_string(),
                        image_url: None
                    },
                    Action {
                        activation_type: ActivationType::Protocol,
                        action_content: "komari-monitor-rs".to_string(),
                        arguments: "https://github.com/GenshinMinecraft/komari-monitor-rs".to_string(),
                        image_url: None
                    },
                ])
                .show()
                .expect("Failed to show toast notification");
        }
    }

    if !network_config.disable_network_statistics {
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let _listener = tokio::spawn(async move {
            network_saver(&network_config, ready_tx).await;
        });
        if ready_rx.await.is_err() {
            error!(
                "Periodic traffic statistics failed to initialize; reports will use raw interface totals. See the preceding error for details."
            );
        }
    } else {
        info!(
            "Network statistics feature disabled. This will fallback to statistics only showing network interface traffic since the current startup"
        );
    }

    // Basic information is refreshed in both transports, independently of reconnects.
    let basic_args = args.clone();
    let basic_url = connection_urls.rpc_http.clone();
    tokio::spawn(async move {
        let mut system = sysinfo::System::new();
        loop {
            system.refresh_cpu_list(CpuRefreshKind::nothing());
            system.refresh_memory_specifics(MemoryRefreshKind::everything());
            let info = BasicInfo::build(&system, basic_args.fake, &basic_args.ip_provider).await;
            if let Err(e) = post_json_async(
                basic_url.clone(),
                protocol::basic_info(info),
                basic_args.ignore_unsafe_cert,
            )
            .await
            {
                error!("Failed to push Basic Info: {e}");
            }
            sleep(Duration::from_secs(
                basic_args.info_report_interval.max(1).saturating_mul(60),
            ))
            .await;
        }
    });

    // Execute fallback events independently, so a slow command does not stop reports.
    let (event_tx, mut event_rx) = mpsc::channel::<String>(8);
    let (ack_tx, mut ack_rx) = mpsc::unbounded_channel();
    let exec_cache = Arc::new(Mutex::new(callbacks::exec::ExecCache::default()));
    let event_exec_cache = exec_cache.clone();
    let event_args = args.clone();
    let event_urls = connection_urls.clone();
    tokio::spawn(async move {
        let mut processed = std::collections::VecDeque::new();
        while let Some(body) = event_rx.recv().await {
            let ids = handle_http_events(
                &event_args,
                &event_urls,
                &body,
                &mut processed,
                &event_exec_cache,
            )
            .await;
            if ack_tx.send(ids).is_err() {
                break;
            }
        }
    });

    // Keep the same counters across WS/POST transitions; network and CPU usage
    // require two samples rather than a new System/Networks on every report.
    let mut system = sysinfo::System::new();
    let mut networks = Networks::new_with_refreshed_list();
    let mut disks = Disks::new();
    system.refresh_cpu_list(CpuRefreshKind::everything().without_frequency());
    let mut last_sample = Instant::now();
    let mut ticker =
        tokio::time::interval(Duration::from_millis(args.realtime_info_interval.max(1)));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut connection = Box::pin(reconnect_ws(&connection_urls.rpc_ws, &args, Duration::ZERO));
    let mut writer = None;
    let mut callbacks: Option<tokio::task::JoinHandle<()>> = None;
    let mut failures = 0_u32;
    let mut fallback = false;
    let mut report_id = 0_u64;
    let mut ack_ids = Vec::new();

    loop {
        tokio::select! {
            result = &mut connection, if writer.is_none() => {
                match result {
                    Ok(stream) => {
                        let (write, mut read) = stream.split();
                        let locked_write: Arc<Mutex<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>> = Arc::new(Mutex::new(write));
                        let callback_writer = locked_write.clone();
                        let callback_args = args.clone();
                        let callback_urls = connection_urls.clone();
                        let callback_exec_cache = exec_cache.clone();
                        callbacks = Some(tokio::spawn(async move {
                            handle_callbacks(&callback_args, &callback_urls, &mut read, &callback_writer, &callback_exec_cache).await;
                        }));
                        writer = Some(locked_write);
                        failures = 0;
                        info!("Connected to Komari v2 WebSocket");
                    }
                    Err(e) => {
                        failures = failures.saturating_add(1);
                        fallback = failures >= args.max_retries.max(1);
                        error!("WebSocket connection failed: {e}; POST fallback: {fallback}");
                        connection = Box::pin(reconnect_ws(
                            &connection_urls.rpc_ws, &args,
                            Duration::from_secs(args.reconnect_interval.max(1)),
                        ));
                    }
                }
            }
            _ = ticker.tick() => {
                if callbacks.as_ref().is_some_and(tokio::task::JoinHandle::is_finished) {
                    callbacks.take();
                    writer = None;
                    connection = Box::pin(reconnect_ws(&connection_urls.rpc_ws, &args, Duration::ZERO));
                }
                system.refresh_specifics(
                    RefreshKind::nothing()
                        .with_cpu(CpuRefreshKind::everything().without_frequency())
                        .with_memory(MemoryRefreshKind::everything()),
                );
                networks.refresh(true);
                disks.refresh_specifics(true, DiskRefreshKind::nothing().with_storage());
                let elapsed = u64::try_from(last_sample.elapsed().as_millis()).unwrap_or(u64::MAX).max(1);
                last_sample = Instant::now();
                let report = RealTimeInfo::build(&system, &networks, &disks, args.fake, elapsed);

                if let Some(write) = &writer {
                    let send = async {
                        write.lock().await.send(Message::Text(Utf8Bytes::from(protocol::report(report)))).await
                    };
                    if !matches!(timeout(Duration::from_secs(5), send).await, Ok(Ok(()))) {
                        error!("WebSocket report failed; reconnecting");
                        if let Some(task) = callbacks.take() { task.abort(); }
                        writer = None;
                        connection = Box::pin(reconnect_ws(&connection_urls.rpc_ws, &args, Duration::ZERO));
                    }
                } else if fallback {
                    while let Ok(ids) = ack_rx.try_recv() { ack_ids.extend(ids); }
                    ack_ids.sort_unstable();
                    ack_ids.dedup();
                    report_id = report_id.wrapping_add(1);
                    let payload = protocol::report_request(report, format!("report-{report_id}"), ack_ids.clone());
                    match post_json_async(connection_urls.rpc_http.clone(), payload, args.ignore_unsafe_cert).await {
                        Ok(body) => {
                            ack_ids.clear();
                            // A full queue is safe: unacknowledged events are delivered again.
                            let _ = event_tx.try_send(body);
                        }
                        Err(e) => error!("v2 POST fallback report failed: {e}"),
                    }
                }
            }
        }
    }
}

async fn reconnect_ws(
    url: &str,
    args: &Args,
    delay: Duration,
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, String> {
    sleep(delay).await;
    connect_ws(url, args.tls, args.ignore_unsafe_cert).await
}
