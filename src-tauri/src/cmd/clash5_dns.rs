use super::{CmdResult, coded_error};
use crate::{
    clash5::dns_check::{self, DnsCheckReport, DnsResolvers, DnsSample, LeakCheck},
    config::Config,
    core::handle::Handle,
};
use serde_yaml_ng::Value;
use std::time::{Duration, Instant};

const SAMPLE_TIMEOUT: Duration = Duration::from_secs(5);

async fn query_sample(domain: &str) -> Result<Option<String>, String> {
    let path = format!("/dns/query?name={domain}&type=A");
    let request = Handle::mihomo()
        .load_ctx()
        .build_request(reqwest::Method::GET, &path)
        .map_err(|_| "mihomo controller is unavailable".to_owned())?;
    let response = request
        .timeout(SAMPLE_TIMEOUT)
        .send()
        .await
        .map_err(|_| "resolver did not answer".to_owned())?;
    let body = response.text().await.map_err(|_| "unreadable response".to_owned())?;
    dns_check::parse_dns_answer(&body)
}

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

#[tauri::command]
#[specta::specta]
pub async fn clash5_dns_samples() -> CmdResult<Vec<DnsSample>> {
    let mut samples = Vec::with_capacity(dns_check::SAMPLE_DOMAINS.len());
    for domain in dns_check::SAMPLE_DOMAINS {
        let started = Instant::now();
        let outcome = query_sample(domain).await;
        samples.push(DnsSample {
            domain: domain.to_owned(),
            answer: outcome.as_ref().ok().cloned().flatten(),
            error: outcome.err(),
            duration_ms: u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX),
        });
    }
    Ok(samples)
}
