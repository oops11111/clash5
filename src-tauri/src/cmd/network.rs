use super::CmdResult;
use crate::cmd::StringifyErr as _;
use crate::core::{proxy_control, sysopt::Sysopt};

use gethostname::gethostname;
use sysproxy::{Autoproxy, Sysproxy};
use tauri_plugin_clash_verge_sysinfo;

#[derive(serde::Serialize, specta::Type)]
pub struct SystemProxyWire {
    enable: bool,
    server: std::string::String,
    bypass: std::string::String,
}

#[derive(serde::Serialize, specta::Type)]
pub struct AutoProxyWire {
    enable: bool,
    url: std::string::String,
}

#[allow(dead_code)]
#[derive(serde::Serialize, specta::Type)]
pub struct NetworkInterfaceWire {
    name: std::string::String,
    addr: Vec<NetworkAddressWire>,
    mac_addr: Option<std::string::String>,
    index: u32,
    internal: bool,
}

#[allow(dead_code)]
#[derive(serde::Serialize, specta::Type)]
enum NetworkAddressWire {
    V4(NetworkV4AddressWire),
    V6(NetworkV6AddressWire),
}

#[allow(dead_code)]
#[derive(serde::Serialize, specta::Type)]
struct NetworkV4AddressWire {
    ip: std::net::Ipv4Addr,
    broadcast: Option<std::net::Ipv4Addr>,
    netmask: Option<std::net::Ipv4Addr>,
}

#[allow(dead_code)]
#[derive(serde::Serialize, specta::Type)]
struct NetworkV6AddressWire {
    ip: std::net::Ipv6Addr,
    broadcast: Option<std::net::Ipv6Addr>,
    netmask: Option<std::net::Ipv6Addr>,
}

#[tauri::command]
#[specta::specta]
pub async fn get_sys_proxy() -> CmdResult<SystemProxyWire> {
    Sysopt::global().wait_idle().await;
    // With no network service there is no proxy configured anywhere, which reads as disabled.
    let sys_proxy = match Sysproxy::get_system_proxy() {
        Err(error) if proxy_control::is_missing_network_service(&error) => Sysproxy::default(),
        other => other.stringify_err()?,
    };
    let Sysproxy {
        ref host,
        ref bypass,
        ref port,
        ref enable,
    } = sys_proxy;

    Ok(SystemProxyWire {
        enable: *enable,
        server: format!("{}:{}", host, port),
        bypass: bypass.clone(),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn get_auto_proxy() -> CmdResult<AutoProxyWire> {
    Sysopt::global().wait_idle().await;
    let auto_proxy = match Autoproxy::get_auto_proxy() {
        Err(error) if proxy_control::is_missing_network_service(&error) => Autoproxy::default(),
        other => other.stringify_err()?,
    };
    let Autoproxy { ref enable, ref url } = auto_proxy;

    Ok(AutoProxyWire {
        enable: *enable,
        url: url.clone(),
    })
}

#[tauri::command]
#[specta::specta]
pub fn get_embedded_server_port() -> CmdResult<u16> {
    crate::utils::server::embedded_server_port().stringify_err()
}

#[tauri::command]
#[specta::specta]
pub fn get_system_hostname() -> String {
    match gethostname().into_string() {
        Ok(name) => name,
        Err(os_string) => {
            let fallback = format!("{os_string:?}");
            fallback.trim_matches('"').to_string()
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_network_interfaces() -> Vec<String> {
    tauri_plugin_clash_verge_sysinfo::list_network_interfaces()
}

#[tauri::command]
#[specta::specta]
pub fn get_network_interfaces_info() -> CmdResult<Vec<NetworkInterfaceWire>> {
    use network_interface::{NetworkInterface, NetworkInterfaceConfig as _};

    let names = get_network_interfaces();
    let interfaces = NetworkInterface::show().stringify_err()?;

    let mut result = Vec::new();

    for interface in interfaces {
        if names.contains(&interface.name) {
            let addr = interface
                .addr
                .into_iter()
                .map(|address| match address {
                    network_interface::Addr::V4(address) => NetworkAddressWire::V4(NetworkV4AddressWire {
                        ip: address.ip,
                        broadcast: address.broadcast,
                        netmask: address.netmask,
                    }),
                    network_interface::Addr::V6(address) => NetworkAddressWire::V6(NetworkV6AddressWire {
                        ip: address.ip,
                        broadcast: address.broadcast,
                        netmask: address.netmask,
                    }),
                })
                .collect();
            result.push(NetworkInterfaceWire {
                name: interface.name,
                addr,
                mac_addr: interface.mac_addr,
                index: interface.index,
                internal: interface.internal,
            });
        }
    }

    Ok(result)
}

/// The services the Home IP card may query. Requests go through the running core's mixed port, so
/// the reported address is the one the selected route exits from.
const IP_INFO_SERVICES: [&str; 6] = [
    "https://api.ip.sb/geoip",
    "https://ipapi.co/json",
    "https://api.ipapi.is/",
    "https://ipwho.is/",
    "https://ip.api.skk.moe/cf-geoip",
    "https://get.geojs.io/v1/ip/geo.json",
];

fn ip_info_service_allowed(url: &str) -> bool {
    IP_INFO_SERVICES.contains(&url)
}

/// Fetches a small HTTPS document through the running core's mixed port, so the request leaves on the
/// route the core selects. Redirects are refused and the body is capped at `limit` bytes.
pub(crate) async fn fetch_via_core(url: &str, limit: usize) -> CmdResult<Vec<u8>> {
    let url = crate::clash5::catalog::validate_catalog_url(url).stringify_err()?;
    let port = crate::config::active_mixed_port().await;
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(format!("http://127.0.0.1:{port}")).stringify_err()?)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(10))
        .user_agent(format!("clash5/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .stringify_err()?;
    let mut response = client.get(url).send().await.stringify_err()?;
    if !response.status().is_success() {
        return Err(super::CommandFailure::plain(format!(
            "remote service returned {}",
            response.status()
        )));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.stringify_err()? {
        if body.len() + chunk.len() > limit {
            return Err(super::CommandFailure::plain("remote response is too large"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[tauri::command]
#[specta::specta]
pub async fn fetch_ip_info(service_url: String) -> CmdResult<String> {
    if !ip_info_service_allowed(&service_url) {
        return Err(super::CommandFailure::plain("IP information service is not allowed"));
    }
    let body = fetch_via_core(&service_url, 16 * 1024).await?;
    String::from_utf8(body).map_err(super::CommandFailure::plain)
}

#[cfg(test)]
mod tests {
    use super::ip_info_service_allowed;

    #[test]
    fn ip_info_requests_are_limited_to_the_listed_https_services() {
        assert!(ip_info_service_allowed("https://api.ip.sb/geoip"));
        assert!(!ip_info_service_allowed("http://api.ip.sb/geoip"));
        assert!(!ip_info_service_allowed("http://127.0.0.1:9097/configs"));
        assert!(!ip_info_service_allowed("https://example.invalid/"));
    }
}
