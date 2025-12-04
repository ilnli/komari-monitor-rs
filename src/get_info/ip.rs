use crate::config::IpProvider;
use log::trace;
use miniserde::{Deserialize, Serialize, json};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;
use tokio::task::JoinHandle;

#[cfg(feature = "ureq-support")]
use std::time::Duration;

pub async fn ip(provider: &IpProvider) -> IPInfo {
    match provider {
        IpProvider::Cloudflare => ip_cloudflare().await,
        IpProvider::Ipinfo => ip_ipinfo().await,
    }
}

#[derive(Debug)]
pub struct IPInfo {
    pub ipv4: Option<Ipv4Addr>,
    pub ipv6: Option<Ipv6Addr>,
}

#[derive(Serialize, Deserialize)]
struct IpJson {
    ip: String,
}

pub async fn ip_ipinfo() -> IPInfo {
    let ipv4: JoinHandle<Option<Ipv4Addr>> = tokio::spawn(async move {
        #[cfg(feature = "ureq-support")]
        let body = {
            let Ok(mut resp) = ureq::get("https://ipinfo.io")
                .header("User-Agent", "curl/8.7.1")
                .config()
                .timeout_global(Some(Duration::from_secs(5)))
                .ip_family(ureq::config::IpFamily::Ipv4Only)
                .build()
                .call()
            else {
                return None;
            };

            let Ok(body) = resp.body_mut().read_to_string() else {
                return None;
            };

            body
        };

        #[cfg(feature = "nyquest-support")]
        let body = {
            use nyquest::Request;
            let client = crate::utils::create_nyquest_client(false);
            let request = Request::get("https://ipinfo.io");

            if let Ok(res) = client.request(request) {
                res.text().ok()?
            } else {
                return None;
            }
        };

        let json: IpJson = match json::from_str(&body) {
            Err(_) => {
                return None;
            }
            Ok(json) => json,
        };

        Ipv4Addr::from_str(json.ip.as_str()).ok()
    });

    let ipv6: JoinHandle<Option<Ipv6Addr>> = tokio::spawn(async move {
        #[cfg(feature = "ureq-support")]
        let body = {
            let Ok(mut resp) = ureq::get("https://6.ipinfo.io")
                .header("User-Agent", "curl/8.7.1")
                .config()
                .timeout_global(Some(Duration::from_secs(5)))
                .ip_family(ureq::config::IpFamily::Ipv6Only)
                .build()
                .call()
            else {
                return None;
            };

            let Ok(body) = resp.body_mut().read_to_string() else {
                return None;
            };
            body
        };

        #[cfg(feature = "nyquest-support")]
        let body = {
            use nyquest::Request;
            let client = crate::utils::create_nyquest_client(false);
            let request = Request::get("https://6.ipinfo.io");

            if let Ok(res) = client.request(request) {
                res.text().ok()?
            } else {
                return None;
            }
        };

        let json: IpJson = match json::from_str(&body) {
            Err(_) => {
                return None;
            }
            Ok(json) => json,
        };

        Ipv6Addr::from_str(json.ip.as_str()).ok()
    });

    let ip_info = IPInfo {
        ipv4: ipv4.await.unwrap(),
        ipv6: ipv6.await.unwrap(),
    };

    trace!("IP INFO (ipinfo) 获取成功: {ip_info:?}");

    ip_info
}

pub async fn ip_cloudflare() -> IPInfo {
    let ipv4: JoinHandle<Option<Ipv4Addr>> = tokio::spawn(async move {
        #[cfg(feature = "ureq-support")]
        let body = {
            let Ok(mut resp) = ureq::get("https://www.cloudflare.com/cdn-cgi/trace")
                .header("User-Agent", "curl/8.7.1")
                .config()
                .timeout_global(Some(Duration::from_secs(5)))
                .ip_family(ureq::config::IpFamily::Ipv4Only)
                .build()
                .call()
            else {
                return None;
            };

            let Ok(body) = resp.body_mut().read_to_string() else {
                return None;
            };
            body
        };

        #[cfg(feature = "nyquest-support")]
        let body = {
            use nyquest::Request;
            let client = crate::utils::create_nyquest_client(false);
            let request = Request::get("https://1.1.1.1/cdn-cgi/trace");

            if let Ok(res) = client.request(request) {
                res.text().ok()?
            } else {
                return None;
            }
        };

        let mut ip = String::new();

        for line in body.lines() {
            if line.starts_with("ip=") {
                ip = line.replace("ip=", "");
                break;
            }
        }

        Ipv4Addr::from_str(ip.as_str()).ok()
    });

    let ipv6: JoinHandle<Option<Ipv6Addr>> = tokio::spawn(async move {
        #[cfg(feature = "ureq-support")]
        let body = {
            let Ok(mut resp) = ureq::get("https://www.cloudflare.com/cdn-cgi/trace")
                .header("User-Agent", "curl/8.7.1")
                .config()
                .timeout_global(Some(Duration::from_secs(5)))
                .ip_family(ureq::config::IpFamily::Ipv6Only)
                .build()
                .call()
            else {
                return None;
            };

            let Ok(body) = resp.body_mut().read_to_string() else {
                return None;
            };
            body
        };

        #[cfg(feature = "nyquest-support")]
        let body = {
            use nyquest::Request;
            let client = crate::utils::create_nyquest_client(false);
            let request = Request::get("https://[2606:4700:4700::1111]/cdn-cgi/trace");

            if let Ok(res) = client.request(request) {
                res.text().ok()?
            } else {
                return None;
            }
        };

        let mut ip = String::new();

        for line in body.lines() {
            if line.starts_with("ip=") {
                ip = line.replace("ip=", "");
                break;
            }
        }

        Ipv6Addr::from_str(ip.as_str()).ok()
    });

    let ip_info = IPInfo {
        ipv4: ipv4.await.unwrap(),
        ipv6: ipv6.await.unwrap(),
    };

    trace!("IP INFO (cloudflare) 获取成功: {ip_info:?}");

    ip_info
}
