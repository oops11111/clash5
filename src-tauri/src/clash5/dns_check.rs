//! Local DNS leak checks. Everything runs on this device and nothing is uploaded.
// Only the Linux probes call these helpers; other platforms report every check as unsupported.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use serde::Serialize;
use serde_yaml_ng::{Mapping, Value};
use std::collections::BTreeSet;
use std::net::IpAddr;

#[derive(Debug, Clone, Copy, Serialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Fail,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct LeakCheck {
    pub id: std::string::String,
    pub status: CheckStatus,
    /// Addresses that caused a failure, or the reason a check is unsupported.
    pub details: Vec<std::string::String>,
}

/// Domains resolved through the local listener as samples; they are never used to judge a leak.
pub const SAMPLE_DOMAINS: [&str; 3] = ["www.baidu.com", "api.github.com", "registry.npmjs.org"];

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct DnsSample {
    pub domain: std::string::String,
    pub answer: Option<std::string::String>,
    pub error: Option<std::string::String>,
    pub duration_ms: u32,
}

/// First IPv4 answer in a mihomo `/dns/query` body.
pub fn parse_dns_answer(body: &str) -> Result<Option<std::string::String>, std::string::String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|_| "unreadable DNS answer".to_owned())?;
    let status = value.get("Status").and_then(serde_json::Value::as_i64).unwrap_or(-1);
    if status != 0 {
        return Err(format!("DNS status {status}"));
    }
    Ok(value
        .get("Answer")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|record| record.get("type").and_then(serde_json::Value::as_u64) == Some(1))
        .find_map(|record| {
            record
                .get("data")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        }))
}

#[derive(Debug, Clone, Default, Serialize, specta::Type)]
pub struct DnsResolvers {
    pub nameserver: Vec<std::string::String>,
    pub fallback: Vec<std::string::String>,
    pub default_nameserver: Vec<std::string::String>,
    pub listen: Option<std::string::String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct DnsCheckReport {
    pub resolvers: DnsResolvers,
    pub checks: Vec<LeakCheck>,
    pub checked_at: std::string::String,
}

fn sequence_strings(dns: &Mapping, key: &str) -> Vec<std::string::String> {
    dns.get(key)
        .and_then(Value::as_sequence)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

pub fn resolvers_from_dns(dns: Option<&Mapping>) -> DnsResolvers {
    let Some(dns) = dns else {
        return DnsResolvers::default();
    };
    DnsResolvers {
        nameserver: sequence_strings(dns, "nameserver"),
        fallback: sequence_strings(dns, "fallback"),
        default_nameserver: sequence_strings(dns, "default-nameserver"),
        listen: dns.get("listen").and_then(Value::as_str).map(str::to_owned),
    }
}

/// Host of a plaintext (`udp://` or scheme-less) resolver entry; `None` for encrypted schemes.
pub fn plaintext_host(entry: &str) -> Option<std::string::String> {
    let entry = entry.split('#').next().unwrap_or_default().trim();
    let rest = match entry.split_once("://") {
        Some(("udp" | "tcp", rest)) => rest,
        Some(_) => return None,
        None => entry,
    };
    let host = if let Some(bracketed) = rest.strip_prefix('[') {
        bracketed.split(']').next().unwrap_or_default()
    } else if rest.matches(':').count() == 1 {
        rest.split(':').next().unwrap_or_default()
    } else {
        rest
    };
    (!host.is_empty()).then(|| host.to_owned())
}

pub fn plaintext_upstreams(resolvers: &DnsResolvers) -> BTreeSet<std::string::String> {
    resolvers
        .nameserver
        .iter()
        .chain(&resolvers.fallback)
        .chain(&resolvers.default_nameserver)
        .filter_map(|entry| plaintext_host(entry))
        .collect()
}

fn is_loopback(address: &str) -> bool {
    address
        .split('%')
        .next()
        .and_then(|host| host.parse::<IpAddr>().ok())
        .is_some_and(|ip| ip.is_loopback())
}

/// Nameserver addresses from a `resolv.conf` file.
pub fn parse_resolv_conf(text: &str) -> Vec<std::string::String> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            (fields.next()? == "nameserver").then(|| fields.next().map(str::to_owned))?
        })
        .collect()
}

pub fn system_resolver_check(servers: &[std::string::String]) -> LeakCheck {
    if servers.is_empty() {
        return LeakCheck {
            id: "system_resolver".into(),
            status: CheckStatus::Unsupported,
            details: vec!["no system nameserver was found".into()],
        };
    }
    let remote: Vec<std::string::String> = servers.iter().filter(|server| !is_loopback(server)).cloned().collect();
    let (status, details) = if remote.is_empty() {
        (CheckStatus::Pass, Vec::new())
    } else {
        (CheckStatus::Fail, remote)
    };
    LeakCheck {
        id: "system_resolver".into(),
        status,
        details,
    }
}

/// Peer hosts of connections whose remote port is 53, from `ss -H -n -t -u state connected`.
pub fn parse_port_53_peers(output: &str) -> Vec<std::string::String> {
    output
        .lines()
        .filter_map(|line| {
            let peer = line.split_whitespace().nth(4)?;
            let (host, port) = peer.rsplit_once(':')?;
            if port != "53" {
                return None;
            }
            let host = host.trim_start_matches('[').trim_end_matches(']');
            Some(host.to_owned())
        })
        .collect()
}

pub fn plaintext_leak_check(peers: &[std::string::String], upstreams: &BTreeSet<std::string::String>) -> LeakCheck {
    let external: BTreeSet<std::string::String> = peers
        .iter()
        .filter(|peer| !is_loopback(peer) && !upstreams.contains(peer.as_str()))
        .cloned()
        .collect();
    let (status, details) = if external.is_empty() {
        (CheckStatus::Pass, Vec::new())
    } else {
        (CheckStatus::Fail, external.into_iter().collect())
    };
    LeakCheck {
        id: "plaintext_dns".into(),
        status,
        details,
    }
}

pub fn unsupported(id: &str, reason: &str) -> LeakCheck {
    LeakCheck {
        id: id.into(),
        status: CheckStatus::Unsupported,
        details: vec![reason.into()],
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn plaintext_hosts_skip_encrypted_schemes() {
        assert_eq!(plaintext_host("223.6.6.6").as_deref(), Some("223.6.6.6"));
        assert_eq!(plaintext_host("223.6.6.6:53").as_deref(), Some("223.6.6.6"));
        assert_eq!(plaintext_host("udp://8.8.8.8:53#en0").as_deref(), Some("8.8.8.8"));
        assert_eq!(plaintext_host("[2400::1]:53").as_deref(), Some("2400::1"));
        assert_eq!(plaintext_host("https://doh.pub/dns-query"), None);
        assert_eq!(plaintext_host("tls://dns.alidns.com"), None);
    }

    #[test]
    fn system_resolver_must_point_at_loopback() {
        let resolv = "# generated\nnameserver 127.0.0.53\nsearch lan\n";
        let servers = parse_resolv_conf(resolv);
        assert_eq!(system_resolver_check(&servers).status, CheckStatus::Pass);

        let leaking = parse_resolv_conf("nameserver 127.0.0.1\nnameserver 192.168.1.1\n");
        let check = system_resolver_check(&leaking);
        assert_eq!(check.status, CheckStatus::Fail);
        assert_eq!(check.details, vec!["192.168.1.1".to_owned()]);

        assert_eq!(system_resolver_check(&[]).status, CheckStatus::Unsupported);
    }

    #[test]
    fn plaintext_queries_to_upstreams_are_not_leaks() {
        let resolvers = DnsResolvers {
            nameserver: vec!["223.6.6.6".into(), "https://doh.pub/dns-query".into()],
            ..DnsResolvers::default()
        };
        let upstreams = plaintext_upstreams(&resolvers);
        let ss = "\
UNCONN 0 0 10.0.0.2:41000 223.6.6.6:53
UNCONN 0 0 10.0.0.2:41001 1.1.1.1:53
ESTAB 0 0 10.0.0.2:52000 203.0.113.9:443
UNCONN 0 0 127.0.0.1:41002 127.0.0.53:53
";
        let peers = parse_port_53_peers(ss);
        assert_eq!(
            peers,
            vec!["223.6.6.6".to_owned(), "1.1.1.1".to_owned(), "127.0.0.53".to_owned()]
        );

        let check = plaintext_leak_check(&peers, &upstreams);
        assert_eq!(check.status, CheckStatus::Fail);
        assert_eq!(check.details, vec!["1.1.1.1".to_owned()]);

        assert_eq!(plaintext_leak_check(&[], &upstreams).status, CheckStatus::Pass);
    }

    #[test]
    fn dns_answers_come_from_the_first_ipv4_record() {
        let body =
            r#"{"Status":0,"Answer":[{"name":"x.","type":5,"data":"alias."},{"name":"x.","type":1,"data":"1.2.3.4"}]}"#;
        assert_eq!(parse_dns_answer(body).unwrap().as_deref(), Some("1.2.3.4"));
        assert_eq!(parse_dns_answer(r#"{"Status":0}"#).unwrap(), None);
        assert_eq!(parse_dns_answer(r#"{"Status":3}"#).unwrap_err(), "DNS status 3");
        assert!(parse_dns_answer("not json").is_err());
    }
}

/// Every IPv4 server address in `Get-DnsClientServerAddress | ConvertTo-Json` output.
/// PowerShell prints one object for a single interface and an array otherwise.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn parse_dns_server_json(json: &str) -> Option<Vec<std::string::String>> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let interfaces = match value {
        serde_json::Value::Array(items) => items,
        other => vec![other],
    };
    Some(
        interfaces
            .iter()
            .filter_map(|item| item.get("ServerAddresses")?.as_array())
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_owned)
            .collect(),
    )
}

/// Local addresses listening on port 53 in `netstat -ano -p UDP|TCP` output, with the owning process.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn parse_netstat_listeners(output: &str) -> Vec<std::string::String> {
    output
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let protocol = fields.first()?;
            if !protocol.eq_ignore_ascii_case("UDP") && !protocol.eq_ignore_ascii_case("TCP") {
                return None;
            }
            let local = fields.get(1)?;
            let (_, port) = local.rsplit_once(':')?;
            if port != "53" {
                return None;
            }
            let pid = fields.last().filter(|pid| pid.chars().all(|c| c.is_ascii_digit()))?;
            Some(format!("{local} (pid {pid})"))
        })
        .collect()
}

/// Local addresses listening on port 53 in `ss -H -l -n -t -u` output.
pub fn parse_ss_listeners(output: &str) -> Vec<std::string::String> {
    output
        .lines()
        .filter_map(|line| {
            let local = line.split_whitespace().nth(3)?;
            let (_, port) = local.rsplit_once(':')?;
            (port == "53").then(|| local.to_owned())
        })
        .collect()
}

/// Port 53 must only be bound to loopback. An address bound on any other interface, or on all of them, is a failure.
pub fn listener_check(listeners: &[std::string::String]) -> LeakCheck {
    let exposed: Vec<std::string::String> = listeners
        .iter()
        .filter(|listener| {
            let address = listener.split(" (").next().unwrap_or_default();
            let host = address.rsplit_once(':').map_or(address, |(host, _)| host);
            let host = host.trim_start_matches('[').trim_end_matches(']');
            !is_loopback(host)
        })
        .cloned()
        .collect();
    let (status, details) = if exposed.is_empty() {
        (CheckStatus::Pass, Vec::new())
    } else {
        (CheckStatus::Fail, exposed)
    };
    LeakCheck {
        id: "dns_listener".into(),
        status,
        details,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod platform_parser_tests {
    use super::*;

    #[test]
    fn windows_server_lists_are_read_for_one_or_many_interfaces() {
        let many = r#"[{"InterfaceAlias":"Wi-Fi","InterfaceIndex":4,"AddressFamily":2,"ServerAddresses":["192.0.2.1","192.0.2.2"]},{"InterfaceAlias":"Loopback Pseudo-Interface 1","InterfaceIndex":1,"AddressFamily":2,"ServerAddresses":[]}]"#;
        assert_eq!(
            parse_dns_server_json(many).unwrap(),
            vec!["192.0.2.1".to_owned(), "192.0.2.2".to_owned()]
        );
        let one =
            r#"{"InterfaceAlias":"Ethernet","InterfaceIndex":7,"AddressFamily":2,"ServerAddresses":["127.0.0.1"]}"#;
        assert_eq!(parse_dns_server_json(one).unwrap(), vec!["127.0.0.1".to_owned()]);
        assert!(parse_dns_server_json("not json").is_none());
    }

    #[test]
    fn netstat_port_53_listeners_keep_their_owner() {
        let output = "\
Active Connections

  Proto  Local Address          Foreign Address        State           PID
  UDP    0.0.0.0:53             *:*                                    4312
  UDP    127.0.0.1:53           *:*                                    9001
  UDP    [::1]:5353             *:*                                    1200
  TCP    0.0.0.0:53             0.0.0.0:0              LISTENING       4312
";
        assert_eq!(
            parse_netstat_listeners(output),
            vec![
                "0.0.0.0:53 (pid 4312)".to_owned(),
                "127.0.0.1:53 (pid 9001)".to_owned(),
                "0.0.0.0:53 (pid 4312)".to_owned(),
            ]
        );
    }

    #[test]
    fn only_non_loopback_port_53_listeners_fail_the_check() {
        let loopback_only = listener_check(&["127.0.0.1:53 (pid 9001)".to_owned(), "[::1]:53".to_owned()]);
        assert_eq!(loopback_only.status, CheckStatus::Pass);
        let exposed = listener_check(&["0.0.0.0:53 (pid 4312)".to_owned(), "127.0.0.1:53".to_owned()]);
        assert_eq!(exposed.status, CheckStatus::Fail);
        assert_eq!(exposed.details, vec!["0.0.0.0:53 (pid 4312)".to_owned()]);
    }

    #[test]
    fn ss_listeners_are_read_from_the_local_column() {
        let output = "UNCONN 0 0 127.0.0.53%lo:53 0.0.0.0:*\nLISTEN 0 4096 0.0.0.0:22 0.0.0.0:*\n";
        assert_eq!(parse_ss_listeners(output), vec!["127.0.0.53%lo:53".to_owned()]);
    }
}
