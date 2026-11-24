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
