use crate::{config::Config, core::CoreManager};
use anyhow::{Result, anyhow};
use reqwest::{Client, Proxy, Url};
use serde::Serialize;
use serde_yaml_ng::{Mapping, Value};
use std::{
    net::{IpAddr, Ipv4Addr, TcpListener},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize)]
pub struct ExitObservation {
    pub ip: IpAddr,
    pub elapsed_ms: u64,
}

pub(crate) fn service_url(raw: &str) -> Result<Url> {
    let url = Url::parse(raw).map_err(|_| anyhow!("C5-EXIT-001"))?;
    let loopback = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if !(url.scheme() == "https" || url.scheme() == "http" && loopback)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(anyhow!("C5-EXIT-001"));
    }
    Ok(url)
}

pub(crate) fn parse_ip(bytes: &[u8]) -> Result<IpAddr> {
    let raw = std::str::from_utf8(bytes).map_err(|_| anyhow!("C5-EXIT-003"))?;
    if let Ok(ip) = raw.trim().parse() {
        return Ok(ip);
    }
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|_| anyhow!("C5-EXIT-003"))?;
    value
        .get("ip")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| anyhow!("C5-EXIT-003"))
}

async fn request(url: Url, port: u16) -> Result<ExitObservation> {
    let proxy = Proxy::all(format!("http://127.0.0.1:{port}"))?;
    let client = Client::builder()
        .no_proxy()
        .proxy(proxy)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(12))
        .connect_timeout(Duration::from_secs(5))
        .user_agent("clash5/exit-ip")
        .build()?;
    let started = Instant::now();
    let mut response = client.get(url).send().await?.error_for_status()?;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if body.len() + chunk.len() > 4_096 {
            return Err(anyhow!("C5-EXIT-003"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(ExitObservation {
        ip: parse_ip(&body)?,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

/// Probe through a temporary loopback inbound pinned to one chain, then remove only that inbound.
pub(crate) async fn detect(proxy_name: &str, url: Url) -> Result<ExitObservation> {
    let reservation = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let port = reservation.local_addr()?.port();
    let name = format!("clash5-exit-probe-{port}");
    let runtime = Config::runtime().await.data_arc();
    let config = runtime.config.as_ref().ok_or_else(|| anyhow!("C5-EXIT-002"))?;
    if config.get("listeners").is_some_and(|value| !value.is_sequence())
        || config
            .get("listeners")
            .and_then(Value::as_sequence)
            .is_some_and(|listeners| {
                listeners
                    .iter()
                    .any(|listener| listener.get("name").and_then(Value::as_str) == Some(name.as_str()))
            })
    {
        return Err(anyhow!("C5-EXIT-002"));
    }
    let mut listener = Mapping::new();
    listener.insert("name".into(), name.clone().into());
    listener.insert("type".into(), "http".into());
    listener.insert("listen".into(), "127.0.0.1".into());
    listener.insert("port".into(), port.into());
    listener.insert("proxy".into(), proxy_name.into());
    drop(reservation);
    let applied = CoreManager::global()
        .update_runtime_config(move |runtime| {
            if let Some(config) = runtime.config.as_mut() {
                let listeners = config
                    .entry(Value::from("listeners"))
                    .or_insert_with(|| Value::Sequence(vec![]));
                if let Some(listeners) = listeners.as_sequence_mut() {
                    listeners.push(Value::Mapping(listener));
                }
            }
        })
        .await;
    let observed = if applied.as_ref().is_ok_and(|outcome| outcome.is_valid()) {
        request(url, port).await.map_err(|_| anyhow!("C5-EXIT-003"))
    } else {
        Err(anyhow!("C5-EXIT-002"))
    };
    let removed = CoreManager::global()
        .update_runtime_config(move |runtime| {
            if let Some(config) = runtime.config.as_mut()
                && let Some(listeners) = config.get_mut("listeners").and_then(Value::as_sequence_mut)
            {
                listeners.retain(|listener| listener.get("name").and_then(Value::as_str) != Some(name.as_str()));
            }
        })
        .await;
    if !removed.as_ref().is_ok_and(|outcome| outcome.is_valid()) {
        return Err(anyhow!("C5-EXIT-004"));
    }
    observed
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    #[test]
    fn service_accepts_https_and_loopback_fixtures_without_url_credentials() {
        assert!(service_url("https://api64.ipify.org").is_ok());
        assert!(service_url("http://127.0.0.1:18080/ip").is_ok());
        assert!(service_url("http://[::1]:18080/ip").is_ok());
        for url in [
            "http://example.com",
            "https://user:secret@example.com",
            "file:///secret",
            "https://example.com/#secret",
        ] {
            assert!(service_url(url).is_err());
        }
    }

    #[test]
    fn responses_require_an_ip_address_and_do_not_return_server_error_text() {
        assert_eq!(parse_ip(b"203.0.113.8\n").ok(), Some(IpAddr::from([203, 0, 113, 8])));
        assert_eq!(parse_ip(br#"{"ip":"2001:db8::1"}"#).ok(), "2001:db8::1".parse().ok());
        assert!(parse_ip(b"secret or upstream error").is_err());
        assert!(parse_ip(br#"{"ip":"not-an-ip"}"#).is_err());
    }

    #[tokio::test]
    async fn https_detection_uses_connect_through_the_pinned_local_proxy() -> Result<()> {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let port = listener.local_addr()?.port();
        let server = tokio::spawn(async move {
            let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept()).await??;
            let mut bytes = [0_u8; 2_048];
            let count = tokio::time::timeout(Duration::from_secs(5), socket.read(&mut bytes)).await??;
            socket
                .write_all(b"HTTP/1.1 502 Fixture termination\r\nContent-Length: 0\r\n\r\n")
                .await?;
            Ok::<_, anyhow::Error>(String::from_utf8_lossy(&bytes[..count]).into_owned())
        });
        let result = request(service_url("https://exit.example.invalid/ip")?, port).await;
        let headers = server.await??;
        assert!(headers.starts_with("CONNECT exit.example.invalid:443 HTTP/1.1"));
        assert!(!headers.to_lowercase().contains("authorization:"));
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn http_detection_reads_the_exit_ip_from_the_pinned_proxy_response() -> Result<()> {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let port = listener.local_addr()?.port();
        let server = tokio::spawn(async move {
            let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept()).await??;
            let mut bytes = [0_u8; 2_048];
            let count = tokio::time::timeout(Duration::from_secs(5), socket.read(&mut bytes)).await??;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 12\r\nConnection: close\r\n\r\n203.0.113.27")
                .await?;
            Ok::<_, anyhow::Error>(String::from_utf8_lossy(&bytes[..count]).into_owned())
        });
        let result = request(service_url("http://127.0.0.1:1/ip")?, port).await?;
        assert_eq!(result.ip, IpAddr::from([203, 0, 113, 27]));
        assert!(server.await??.starts_with("GET http://127.0.0.1:1/ip HTTP/1.1"));
        Ok(())
    }
}
