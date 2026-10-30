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
}
