use super::{CmdResult, coded_error};
use crate::{
    clash5::dns_check::{self, DnsCheckReport, DnsResolvers, LeakCheck},
    config::Config,
};
use serde_yaml_ng::Value;

#[cfg(target_os = "linux")]
fn run_checks(resolvers: &DnsResolvers) -> Vec<LeakCheck> {
    let servers = std::fs::read_to_string("/etc/resolv.conf")
        .map(|text| dns_check::parse_resolv_conf(&text))
        .unwrap_or_default();
    let system = dns_check::system_resolver_check(&servers);
    let listing = std::process::Command::new("ss")
        .args(["-H", "-n", "-t", "-u", "state", "connected"])
        .output();
    let plaintext = match listing {
        Ok(output) if output.status.success() => {
            let peers = dns_check::parse_port_53_peers(&String::from_utf8_lossy(&output.stdout));
            dns_check::plaintext_leak_check(&peers, &dns_check::plaintext_upstreams(resolvers))
        }
        Ok(_) => dns_check::unsupported("plaintext_dns", "ss could not list connections"),
        Err(_) => dns_check::unsupported("plaintext_dns", "ss is not installed"),
    };
    vec![system, plaintext]
}

#[cfg(not(target_os = "linux"))]
fn run_checks(_resolvers: &DnsResolvers) -> Vec<LeakCheck> {
    vec![
        dns_check::unsupported("system_resolver", "not yet supported on this platform"),
        dns_check::unsupported("plaintext_dns", "not yet supported on this platform"),
    ]
}

#[tauri::command]
#[specta::specta]
pub async fn clash5_dns_check() -> CmdResult<DnsCheckReport> {
    let dns = {
        let runtime = Config::runtime().await.data_arc();
        runtime
            .config
            .as_ref()
            .and_then(|config| config.get("dns"))
            .and_then(Value::as_mapping)
            .cloned()
    };
    let resolvers = dns_check::resolvers_from_dns(dns.as_ref());
    let checks = {
        let resolvers = resolvers.clone();
        tokio::task::spawn_blocking(move || run_checks(&resolvers))
            .await
            .map_err(|_| coded_error("C5-DNS-001", "DNS check did not complete"))?
    };
    Ok(DnsCheckReport {
        resolvers,
        checks,
        checked_at: chrono::Utc::now().to_rfc3339(),
    })
}
