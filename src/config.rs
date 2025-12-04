use log::{info, warn};
use std::fmt::Write;
use std::fs;
use std::path::Path;

/// 配置结构体
#[derive(Debug, Clone)]
pub struct Config {
    pub http_server: String,
    pub ws_server: Option<String>,
    pub token: String,
    pub ip_provider: IpProvider,
    pub terminal: bool,
    pub terminal_entry: String,
    pub fake: f64,
    pub realtime_info_interval: u64,
    pub tls: bool,
    pub ignore_unsafe_cert: bool,
    pub log_level: LogLevel,
    pub billing_day: u32,
    pub auto_update: u64,
    pub update_repo: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpProvider {
    Cloudflare,
    Ipinfo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            http_server: String::new(),
            ws_server: None,
            token: String::new(),
            ip_provider: IpProvider::Ipinfo,
            terminal: false,
            terminal_entry: default_terminal_entry(),
            fake: 1.0,
            realtime_info_interval: 1000,
            tls: false,
            ignore_unsafe_cert: false,
            log_level: LogLevel::Info,
            billing_day: 1,
            auto_update: 0,
            update_repo: "ilnli/komari-monitor-rs".to_string(),
        }
    }
}

fn default_terminal_entry() -> String {
    if cfg!(windows) {
        "cmd.exe".to_string()
    } else if fs::exists("/bin/bash").unwrap_or(false) {
        "bash".to_string()
    } else {
        "sh".to_string()
    }
}

impl Config {
    /// 从配置文件加载
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("无法读取配置文件: {e}"))?;
        
        let mut config = Self::default();
        
        for line in content.lines() {
            let line = line.trim();
            // 跳过空行和注释
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"');
                
                match key {
                    "http_server" => config.http_server = value.to_string(),
                    "ws_server" => {
                        if !value.is_empty() {
                            config.ws_server = Some(value.to_string());
                        }
                    }
                    "token" => config.token = value.to_string(),
                    "ip_provider" => {
                        config.ip_provider = match value.to_lowercase().as_str() {
                            "cloudflare" => IpProvider::Cloudflare,
                            _ => IpProvider::Ipinfo,
                        };
                    }
                    "terminal" => config.terminal = value == "true" || value == "1",
                    "terminal_entry" => {
                        if !value.is_empty() && value != "default" {
                            config.terminal_entry = value.to_string();
                        }
                    }
                    "fake" => config.fake = value.parse().unwrap_or(1.0),
                    "realtime_info_interval" => {
                        config.realtime_info_interval = value.parse().unwrap_or(1000);
                    }
                    "tls" => config.tls = value == "true" || value == "1",
                    "ignore_unsafe_cert" => {
                        config.ignore_unsafe_cert = value == "true" || value == "1";
                    }
                    "log_level" => {
                        config.log_level = match value.to_lowercase().as_str() {
                            "error" => LogLevel::Error,
                            "warn" => LogLevel::Warn,
                            "debug" => LogLevel::Debug,
                            "trace" => LogLevel::Trace,
                            _ => LogLevel::Info,
                        };
                    }
                    "billing_day" => config.billing_day = value.parse().unwrap_or(1),
                    "auto_update" => config.auto_update = value.parse().unwrap_or(0),
                    "update_repo" => {
                        if !value.is_empty() {
                            config.update_repo = value.to_string();
                        }
                    }
                    _ => warn!("未知配置项: {key}"),
                }
            }
        }
        
        // 验证必需字段
        if config.http_server.is_empty() {
            return Err("配置文件中缺少 http_server".to_string());
        }
        if config.token.is_empty() {
            return Err("配置文件中缺少 token".to_string());
        }
        
        Ok(config)
    }
    
    /// 保存到配置文件
    #[allow(dead_code)]
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let mut content = String::with_capacity(512);
        
        content.push_str("# Komari Monitor RS 配置文件\n\n");
        content.push_str("# 主端地址 (必需)\n");
        let _ = writeln!(content, "http_server = \"{}\"", self.http_server);
        
        if let Some(ws) = &self.ws_server {
            let _ = writeln!(content, "ws_server = \"{ws}\"");
        }
        
        let _ = writeln!(content, "token = \"{}\"\n", self.token);
        
        content.push_str("# IP 提供商 (ipinfo / cloudflare)\n");
        let _ = writeln!(
            content,
            "ip_provider = \"{}\"\n",
            match self.ip_provider {
                IpProvider::Cloudflare => "cloudflare",
                IpProvider::Ipinfo => "ipinfo",
            }
        );
        
        content.push_str("# 功能开关\n");
        let _ = writeln!(content, "terminal = {}", self.terminal);
        if self.terminal {
            let _ = writeln!(content, "terminal_entry = \"{}\"", self.terminal_entry);
        }
        let _ = writeln!(content, "tls = {}", self.tls);
        let _ = writeln!(content, "ignore_unsafe_cert = {}\n", self.ignore_unsafe_cert);
        
        content.push_str("# 性能设置\n");
        let _ = writeln!(content, "fake = {}", self.fake);
        let _ = writeln!(content, "realtime_info_interval = {}", self.realtime_info_interval);
        let _ = writeln!(content, "billing_day = {}\n", self.billing_day);
        
        content.push_str("# 日志等级 (error / warn / info / debug / trace)\n");
        let _ = writeln!(
            content,
            "log_level = \"{}\"\n",
            match self.log_level {
                LogLevel::Error => "error",
                LogLevel::Warn => "warn",
                LogLevel::Info => "info",
                LogLevel::Debug => "debug",
                LogLevel::Trace => "trace",
            }
        );
        
        content.push_str("# 自动升级 (0 = 禁用，其他数字为检查间隔小时数)\n");
        let _ = writeln!(content, "auto_update = {}", self.auto_update);
        let _ = writeln!(content, "update_repo = \"{}\"", self.update_repo);
        
        // 确保目录存在
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("无法创建目录: {e}"))?;
        }
        
        fs::write(path, content).map_err(|e| format!("无法写入配置文件: {e}"))?;
        info!("配置已保存到: {}", path.display());
        Ok(())
    }
    
    /// 获取默认配置文件路径（与程序同目录）
    pub fn default_path() -> std::path::PathBuf {
        std::env::current_exe()
            .unwrap_or_default()
            .parent()
            .map_or_else(
                || std::path::PathBuf::from("config"),
                |p| p.join("config"),
            )
    }
}
