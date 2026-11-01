use super::{CmdResult, coded_error};
use crate::{
    clash5::dns_check::{self, DnsCheckReport, DnsResolvers, DnsSample, LeakCheck},
    config::Config,
    core::handle::Handle,
};
use serde_yaml_ng::Value;
use std::time::{Duration, Instant};

const SAMPLE_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

async fn query_sample(domain: &str) -> Result<Option<String>, String> {
    let request = Handle::mihomo()
        .load_ctx()
        .build_request(reqwest::Method::GET, "/dns/query")
        .map_err(|_| "mihomo controller is unavailable".to_owned())?
        .query(&[("name", domain), ("type", "A")])
        .timeout(SAMPLE_TIMEOUT);
    let response = request.send().await.map_err(|error| {
        if error.is_timeout() {
            "resolver timed out".to_owned()
        } else if error.is_connect() {
            "mihomo controller is unreachable".to_owned()
        } else {
            "resolver request failed".to_owned()
        }
    })?;
    if !response.status().is_success() {
        return Err(format!("controller returned HTTP {}", response.status().as_u16()));
    }
    let body = response.text().await.map_err(|_| "unreadable response".to_owned())?;
    dns_check::parse_dns_answer(&body)
}

#[cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]
/// Stdout of a helper program. A program that is missing or fails is reported by name.
fn command_output(program: &str, args: &[&str]) -> Result<String, String> {
    let mut command = std::process::Command::new(program);
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().map_err(|_| format!("{program} is not available"))?;
    if !output.status.success() {
        return Err(format!("{program} could not list the system state"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(target_os = "linux")]
fn run_checks(resolvers: &DnsResolvers) -> Vec<LeakCheck> {
    let servers = std::fs::read_to_string("/etc/resolv.conf")
        .map(|text| dns_check::parse_resolv_conf(&text))
        .unwrap_or_default();
    let system = dns_check::system_resolver_check(&servers);
    let listeners = command_output("ss", &["-H", "-n", "-l", "-t", "-u"])
        .map(|output| dns_check::listener_check(&dns_check::parse_ss_listeners(&output)))
        .unwrap_or_else(|reason| dns_check::unsupported("dns_listener", &reason));
    let plaintext = match command_output("ss", &["-H", "-n", "-t", "-u", "state", "connected"]) {
        Ok(output) => {
            let peers = dns_check::parse_port_53_peers(&output);
            dns_check::plaintext_leak_check(&peers, &dns_check::plaintext_upstreams(resolvers))
        }
        Err(reason) => dns_check::unsupported("plaintext_dns", &reason),
    };
    vec![system, listeners, plaintext]
}

#[cfg(windows)]
fn run_checks(_resolvers: &DnsResolvers) -> Vec<LeakCheck> {
    let system = command_output(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-DnsClientServerAddress -AddressFamily IPv4 | ConvertTo-Json -Compress",
        ],
    )
    .and_then(|output| {
        dns_check::parse_dns_server_json(&output).ok_or_else(|| "DNS server list could not be read".to_owned())
    })
    .map_or_else(
        |reason| dns_check::unsupported("system_resolver", &reason),
        |servers| dns_check::system_resolver_check(&servers),
    );
    let listeners = ["UDP", "TCP"]
        .iter()
        .map(|protocol| command_output("netstat", &["-ano", "-p", protocol]))
        .collect::<Result<Vec<String>, String>>()
        .map(|outputs| {
            let listeners: Vec<String> = outputs
                .iter()
                .flat_map(|output| dns_check::parse_netstat_listeners(output))
                .collect();
            dns_check::listener_check(&listeners)
        })
        .unwrap_or_else(|reason| dns_check::unsupported("dns_listener", &reason));
    // netstat does not report the remote end of a UDP socket, so plaintext queries cannot be seen here.
    let plaintext = dns_check::unsupported(
        "plaintext_dns",
        "UDP destinations are not visible to netstat on Windows",
    );
    vec![system, listeners, plaintext]
}

#[cfg(not(any(target_os = "linux", windows)))]
fn run_checks(_resolvers: &DnsResolvers) -> Vec<LeakCheck> {
    vec![
        dns_check::unsupported("system_resolver", "not yet supported on this platform"),
        dns_check::unsupported("dns_listener", "not yet supported on this platform"),
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
