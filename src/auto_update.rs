use log::{error, info, warn};
use std::env;
use std::fs;
use std::process::Command;

const GITHUB_API: &str = "https://api.github.com/repos/{repo}/releases/latest";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 获取当前可执行文件的架构标识
fn get_arch_suffix() -> Option<&'static str> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return Some("linux-x86_64-gnu");
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return Some("linux-aarch64-gnu");
    #[cfg(all(target_os = "linux", target_arch = "arm"))]
    return Some("linux-armv7-gnueabihf");
    #[cfg(all(target_os = "linux", target_arch = "x86"))]
    return Some("linux-i686-gnu");
    #[cfg(not(target_os = "linux"))]
    return None;
}

/// 从 JSON 中提取字符串值（简单解析，避免引入额外依赖）
fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let start = json.find(&pattern)? + pattern.len();
    let rest = &json[start..];
    // 跳过 : 和空格
    let rest = rest.trim_start_matches([':', ' ']);
    if let Some(rest) = rest.strip_prefix('"') {
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    } else {
        None
    }
}

/// 比较版本号，返回 true 表示 remote 更新
fn is_newer_version(current: &str, remote: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.trim_start_matches('v')
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect()
    };
    let cur = parse(current);
    let rem = parse(remote);
    rem > cur
}

/// 检查并执行自动升级
pub fn check_and_upgrade(repo: &str, ignore_unsafe_cert: bool) {
    let Some(arch) = get_arch_suffix() else {
        warn!("自动升级不支持当前平台");
        return;
    };

    let api_url = GITHUB_API.replace("{repo}", repo);

    // 获取最新版本信息
    let response = match http_get(&api_url, ignore_unsafe_cert) {
        Ok(r) => r,
        Err(e) => {
            warn!("检查更新失败: {e}");
            return;
        }
    };

    let Some(tag_name) = extract_json_string(&response, "tag_name") else {
        warn!("无法解析版本信息");
        return;
    };

    if !is_newer_version(CURRENT_VERSION, &tag_name) {
        info!("当前已是最新版本 v{CURRENT_VERSION}");
        return;
    }

    info!("发现新版本: {tag_name}，当前版本: v{CURRENT_VERSION}，开始升级...");

    // 构建下载 URL
    let download_url = format!(
        "https://github.com/{repo}/releases/download/{tag_name}/komari-monitor-rs-{arch}"
    );

    // 下载新版本到临时文件
    let exe_path = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            error!("无法获取当前可执行文件路径: {e}");
            return;
        }
    };

    let tmp_path = exe_path.with_extension("new");

    if let Err(e) = download_file(&download_url, &tmp_path, ignore_unsafe_cert) {
        error!("下载新版本失败: {e}");
        return;
    }

    // 设置可执行权限
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o755)) {
            error!("设置执行权限失败: {e}");
            let _ = fs::remove_file(&tmp_path);
            return;
        }
    }

    // 备份旧版本
    let backup_path = exe_path.with_extension("old");
    if let Err(e) = fs::rename(&exe_path, &backup_path) {
        error!("备份旧版本失败: {e}");
        let _ = fs::remove_file(&tmp_path);
        return;
    }

    // 替换为新版本
    if let Err(e) = fs::rename(&tmp_path, &exe_path) {
        error!("替换新版本失败: {e}，尝试回滚...");
        let _ = fs::rename(&backup_path, &exe_path);
        return;
    }

    info!("升级完成，正在重启服务...");

    // 重启 systemd 服务
    let _ = Command::new("systemctl")
        .args(["restart", "komari-agent-rs"])
        .spawn();
}

#[cfg(feature = "ureq-support")]
fn http_get(url: &str, _ignore_cert: bool) -> Result<String, String> {
    ureq::get(url)
        .header("User-Agent", "komari-monitor-rs")
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())
}

#[cfg(feature = "nyquest-support")]
fn http_get(url: &str, _ignore_cert: bool) -> Result<String, String> {
    let client = nyquest::blocking::Client::builder()
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(url).send().map_err(|e| e.to_string())?;
    resp.text().map_err(|e| e.to_string())
}

#[cfg(not(any(feature = "ureq-support", feature = "nyquest-support")))]
fn http_get(_url: &str, _ignore_cert: bool) -> Result<String, String> {
    Err("未启用 HTTP 客户端".to_string())
}

#[cfg(feature = "ureq-support")]
fn download_file(
    url: &str,
    path: &std::path::Path,
    _ignore_cert: bool,
) -> Result<(), String> {
    let resp = ureq::get(url)
        .header("User-Agent", "komari-monitor-rs")
        .call()
        .map_err(|e| e.to_string())?;

    let mut file = fs::File::create(path).map_err(|e| e.to_string())?;
    std::io::copy(&mut resp.into_body().into_reader(), &mut file).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(feature = "nyquest-support")]
fn download_file(
    url: &str,
    path: &std::path::Path,
    _ignore_cert: bool,
) -> Result<(), String> {
    let client = nyquest::blocking::Client::builder()
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(url).send().map_err(|e| e.to_string())?;
    let bytes = resp.bytes().map_err(|e| e.to_string())?;
    fs::write(path, bytes).map_err(|e| e.to_string())
}

#[cfg(not(any(feature = "ureq-support", feature = "nyquest-support")))]
fn download_file(
    _url: &str,
    _path: &std::path::Path,
    _ignore_cert: bool,
) -> Result<(), String> {
    Err("未启用 HTTP 客户端".to_string())
}
