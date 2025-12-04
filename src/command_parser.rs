use crate::config::Config;
use std::path::PathBuf;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const HELP_TEXT: &str = r#"komari-monitor-rs - Komari 第三方高性能监控 Agent

用法:
  komari-monitor-rs [选项]

选项:
  -c, --config <路径>    配置文件路径 (默认: 程序同目录下的 config 文件)
  -h, --help             显示帮助信息
  -V, --version          显示版本号

配置文件格式 (key = value):
  http_server = "http://your.server:port"   # 必需
  token = "your_token"                       # 必需
  ws_server = "ws://your.server:port"        # 可选
  ip_provider = "ipinfo"                     # ipinfo / cloudflare
  terminal = false                           # 启用 Web Terminal
  terminal_entry = "bash"                    # Terminal 入口程序
  fake = 1.0                                 # 虚假倍率
  realtime_info_interval = 1000              # 上报间隔 (ms)
  tls = false                                # 启用 TLS
  ignore_unsafe_cert = false                 # 忽略证书验证
  log_level = "info"                         # error/warn/info/debug/trace
  billing_day = 1                            # 计费日 (每月第几号)
  auto_update = 0                            # 自动升级间隔 (小时，0=禁用)
  update_repo = "ilnli/komari-monitor-rs"    # 升级仓库

本 Agent 开源于 Github, 使用强力的 Rust 驱动, 爱来自 Komari
"#;

pub fn parse_args() -> Config {
    let args: Vec<String> = std::env::args().collect();
    let mut config_path: Option<PathBuf> = None;
    
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                println!("{HELP_TEXT}");
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("komari-monitor-rs {VERSION}");
                std::process::exit(0);
            }
            "-c" | "--config" => {
                if i + 1 < args.len() {
                    config_path = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                } else {
                    eprintln!("错误: --config 需要指定路径");
                    std::process::exit(1);
                }
            }
            _ => {
                eprintln!("未知参数: {}", args[i]);
                eprintln!("使用 --help 查看帮助");
                std::process::exit(1);
            }
        }
    }
    
    let path = config_path.unwrap_or_else(Config::default_path);
    
    match Config::load(&path) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("加载配置失败: {e}");
            eprintln!("配置文件路径: {}", path.display());
            eprintln!("\n请确保配置文件存在且包含必需的 http_server 和 token 配置项");
            eprintln!("使用 --help 查看配置文件格式");
            std::process::exit(1);
        }
    }
}

