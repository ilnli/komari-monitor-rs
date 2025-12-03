#![warn(clippy::all, clippy::pedantic)]
#![allow(
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::similar_names,
    clippy::too_many_lines
)]

use crate::callbacks::handle_callbacks;
use crate::command_parser::Args;
use crate::data_struct::{BasicInfo, RealTimeInfo};
use crate::get_info::network::traffic_stats::TrafficStats;
use crate::utils::{build_urls, connect_ws, init_logger};
use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt};
use log::{error, info};
use miniserde::json;
use std::sync::Arc;
use std::time::Duration;
use sysinfo::{CpuRefreshKind, DiskRefreshKind, Disks, MemoryRefreshKind, Networks, RefreshKind};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

mod callbacks;
mod command_parser;
mod data_struct;
mod get_info;
mod rustls_config;
mod utils;

#[tokio::main]
async fn main() {
    let args = Args::par();

    init_logger(&args.log_level);

    #[cfg(all(feature = "nyquest-support", not(target_os = "linux")))]
    {
        nyquest_preset::register();
        info!("komari-monitor-rs 正在使用 Nyquest 作为 Http Client，该功能暂未稳定，请谨慎使用");
    }

    #[cfg(all(feature = "nyquest-support", target_os = "linux"))]
    {
        nyquest_backend_curl::register();
        info!("komari-monitor-rs 正在使用 Nyquest 作为 Http Client，该功能暂未稳定，请谨慎使用");
    }

    let connection_urls =
        build_urls(&args.http_server, args.ws_server.as_ref(), &args.token).unwrap();

    info!("成功读取参数: {args:?}");

    loop {
        let Ok(ws_stream) = connect_ws(
            &connection_urls.ws_real_time,
            args.tls,
            args.ignore_unsafe_cert,
        )
        .await
        else {
            error!("无法连接到 Websocket 服务器，5 秒后重新尝试");
            sleep(Duration::from_secs(5)).await;
            continue;
        };

        let (write, mut read) = ws_stream.split();

        let locked_write: Arc<
            Mutex<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>,
        > = Arc::new(Mutex::new(write));

        // Handle callbacks
        {
            let args_cloned = args.clone();
            let connection_urls_cloned = connection_urls.clone();
            let locked_write_cloned = locked_write.clone();
            let _listener = tokio::spawn(async move {
                handle_callbacks(
                    &args_cloned,
                    &connection_urls_cloned,
                    &mut read,
                    &locked_write_cloned,
                )
                .await;
            });
        }

        let mut sysinfo_sys = sysinfo::System::new();
        let mut networks = Networks::new_with_refreshed_list();
        let mut disks = Disks::new();
        sysinfo_sys.refresh_cpu_list(
            CpuRefreshKind::nothing()
                .without_cpu_usage()
                .without_frequency(),
        );
        sysinfo_sys.refresh_memory_specifics(MemoryRefreshKind::everything());

        let basic_info = BasicInfo::build(&sysinfo_sys, args.fake, &args.ip_provider).await;

        basic_info.push(connection_urls.basic_info.clone(), args.ignore_unsafe_cert);

        // 初始化流量统计
        let mut traffic_stats = TrafficStats::load_or_create(args.billing_day);

        // 设置初始的系统总流量（用于计算增量）
        if traffic_stats.last_total_up == 0 && traffic_stats.last_total_down == 0 {
            let (total_up, total_down) =
                crate::get_info::network::get_system_total_traffic(&networks);
            traffic_stats.last_total_up = total_up;
            traffic_stats.last_total_down = total_down;
            traffic_stats.save();
        }

        // 保存计数器，用于定期持久化
        let mut save_counter: u32 = 0;

        loop {
            let start_time = tokio::time::Instant::now();
            sysinfo_sys.refresh_specifics(
                RefreshKind::nothing()
                    .with_cpu(CpuRefreshKind::everything().without_frequency())
                    .with_memory(MemoryRefreshKind::everything()),
            );
            networks.refresh(true);
            disks.refresh_specifics(true, DiskRefreshKind::nothing().with_storage());
            let real_time = RealTimeInfo::build(
                &sysinfo_sys,
                &networks,
                &disks,
                &mut traffic_stats,
                args.realtime_info_interval,
                args.fake,
            );

            // 每 60 次上报保存一次流量统计（默认间隔下约 1 分钟）
            save_counter += 1;
            if save_counter >= 60 {
                traffic_stats.save();
                save_counter = 0;
            }

            let json = json::to_string(&real_time);
            {
                let mut write = locked_write.lock().await;
                if let Err(e) = write.send(Message::Text(Utf8Bytes::from(json))).await {
                    error!("推送 RealTime 时发生错误，尝试重新连接: {e}");
                    break;
                }
            }
            let end_time = start_time.elapsed();

            sleep(Duration::from_millis({
                let end = u64::try_from(end_time.as_millis()).unwrap_or(0);
                args.realtime_info_interval.saturating_sub(end)
            }))
            .await;
        }
    }
}
