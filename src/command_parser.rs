use palc::{Parser, ValueEnum};
use std::fs;

#[derive(Parser, Debug, Clone)]
#[command(
    version,
    long_about = "komari-monitor-rs is a third-party high-performance monitoring agent for the komari monitoring service.",
    after_long_help = "必须设置 --http-server / --token\n--ip-provider 接受 cloudflare / ipinfo\n--log-level 接受 error, warn, info, debug, trace\n\n本 Agent 开源于 Github , 使用强力的 Rust 驱动, 爱来自 Komari"
)]
pub struct Args {
    /// 设置主端 Http 地址
    #[arg(long)]
    pub http_server: String,

    /// 设置主端 WebSocket 地址
    #[arg(long)]
    pub ws_server: Option<String>,

    /// 设置 Token
    #[arg(short, long, allow_hyphen_values = true)]
    pub token: String,

    /// 公网 IP 接口
    #[arg(long, default_value_t=ip_provider())]
    pub ip_provider: IpProvider,

    /// 启用 Terminal (默认关闭)
    #[arg(long, default_value_t = false)]
    pub terminal: bool,

    /// 自定义 Terminal 入口
    #[arg(long, default_value_t = terminal_entry())]
    pub terminal_entry: String,

    /// 设置虚假倍率
    #[arg(short, long, default_value_t = 1.0)]
    pub fake: f64,

    /// 设置 Real-Time Info 上传间隔时间 (ms)
    #[arg(long, default_value_t = 1000)]
    pub realtime_info_interval: u64,

    /// 启用 TLS (默认关闭)
    #[arg(long, default_value_t = false)]
    pub tls: bool,

    /// 忽略证书验证
    #[arg(long, default_value_t = false)]
    pub ignore_unsafe_cert: bool,

    /// 设置日志等级 (反馈问题请开启 Debug 或者 Trace)
    #[arg(long, default_value_t = log_level())]
    pub log_level: LogLevel,

    /// 设置计费日 (每月第几号开始统计流量，默认为1号)
    #[arg(long, default_value_t = 1)]
    pub billing_day: u32,
}

fn terminal_entry() -> String {
    "default".to_string()
}

fn ip_provider() -> IpProvider {
    IpProvider::Ipinfo
}

#[derive(Debug, Clone, ValueEnum)]
pub enum IpProvider {
    Cloudflare,
    Ipinfo,
}

fn log_level() -> LogLevel {
    LogLevel::Info
}

#[derive(Debug, Clone, ValueEnum)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Args {
    pub fn par() -> Self {
        let mut args = Self::parse();
        if args.terminal_entry == "default" {
            args.terminal_entry = {
                if cfg!(windows) {
                    "cmd.exe".to_string()
                } else if fs::exists("/bin/bash").unwrap_or(false) {
                    "bash".to_string()
                } else {
                    "sh".to_string()
                }
            };
        }
        args
    }
}
