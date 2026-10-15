use crate::core::{singbox_config, singbox_process::SingboxProcess, singbox_relay::AuthenticatedRelay};
use anyhow::{Context as _, Result};
use reqwest::Method;
use serde_yaml_ng::Value as YamlValue;
use std::{net::SocketAddr, process::Command, time::Duration};
use tauri_plugin_mihomo::Mihomo;
use tokio::{net::TcpListener, time::sleep};

const SINGBOX_READY_TIMEOUT: Duration = Duration::from_secs(10);
const SINGBOX_READY_INTERVAL: Duration = Duration::from_millis(50);

/// Applies a runtime config from memory so relay credentials never need a config file.
/// Mihomo accepts the complete YAML config in `PUT /configs`'s `payload` field.
pub async fn apply_config_payload(mihomo: &Mihomo, yaml: &str) -> Result<()> {
    let response = mihomo
        .load_ctx()
        .build_request(Method::PUT, "/configs")
        .context("failed to prepare in-memory Mihomo config request")?
        .query(&[("force", "true")])
        .json(&serde_json::json!({ "payload": yaml }))
        .send()
        .await
        .context("failed to send in-memory Mihomo config")?;
    let status = response.status();
    anyhow::ensure!(
        status.is_success(),
        "Mihomo rejected the in-memory runtime config (HTTP {status})"
    );
    Ok(())
}

/// Owns one temporary M4 route. Stop restores the original in-memory Mihomo runtime first,
/// then tears down sing-box and the authenticated relay. Callers must retain this value for
/// as long as the route is active.
pub struct SingboxRoute {
    landing_name: String,
    original_yaml: String,
    process: Option<SingboxProcess>,
    relay: Option<AuthenticatedRelay>,
}

impl SingboxRoute {
    /// Starts a sing-box outbound for one generated landing proxy, then replaces only that
    /// compiled copy in Mihomo. The source profile and persisted runtime file stay credential-free.
    pub async fn start(
        mihomo: &Mihomo,
        mut command: Command,
        runtime_yaml: String,
        landing_name: &str,
    ) -> Result<Self> {
        let inbound = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .context("failed to reserve a loopback sing-box inbound")?;
        let inbound_port = inbound.local_addr()?.port();
        drop(inbound);

        let relay = AuthenticatedRelay::start(SocketAddr::from(([127, 0, 0, 1], inbound_port))).await?;
        let launch = async {
            let config: YamlValue =
                serde_yaml_ng::from_str(&runtime_yaml).context("failed to parse the generated runtime config")?;
            let proxies = config
                .get("proxies")
                .and_then(YamlValue::as_sequence)
                .context("runtime config has no proxy list")?;
            let mut matches = proxies
                .iter()
                .filter(|proxy| proxy.get("name").and_then(YamlValue::as_str) == Some(landing_name));
            let landing = matches.next().context("generated landing proxy is missing")?;
            anyhow::ensure!(matches.next().is_none(), "generated landing proxy is ambiguous");
            let front = landing
                .get("dialer-proxy")
                .and_then(YamlValue::as_str)
                .map(|front_name| {
                    let mut matches = proxies
                        .iter()
                        .filter(|proxy| proxy.get("name").and_then(YamlValue::as_str) == Some(front_name));
                    let front = matches.next().context("generated chain front proxy is missing")?;
                    anyhow::ensure!(matches.next().is_none(), "generated chain front proxy is ambiguous");
                    Ok::<_, anyhow::Error>(front)
                })
                .transpose()?;
            let singbox_config =
                singbox_config::singbox_config(landing, front, inbound_port, "exit", relay.endpoint())?;
            let config_bytes = serde_json::to_vec(&singbox_config)?;

            command.args(["run", "-c", "stdin"]);
            let process = SingboxProcess::start(command, &config_bytes).await?;
            wait_for_inbound(inbound_port, &process).await?;

            let relay_config = singbox_config::relay_config_json(&runtime_yaml, landing_name, relay.endpoint())?;
            let relay_yaml = String::from_utf8(relay_config).context("in-memory relay config was not UTF-8")?;
            apply_config_payload(mihomo, &relay_yaml).await?;
            Ok::<_, anyhow::Error>(process)
        }
        .await;

        match launch {
            Ok(process) => Ok(Self {
                landing_name: landing_name.to_owned(),
                original_yaml: runtime_yaml,
                process: Some(process),
                relay: Some(relay),
            }),
            Err(error) => {
                // No failed start may leave a listener or child process behind.
                relay.stop().await;
                Err(error)
            }
        }
    }

    pub const fn landing_name(&self) -> &str {
        &self.landing_name
    }

    pub async fn is_running(&self) -> Result<bool> {
        match &self.process {
            Some(process) => process.is_running().await,
            None => Ok(false),
        }
    }

    /// A failed Mihomo rollback keeps the route resources alive so active traffic is not stranded.
    pub async fn stop(&mut self, mihomo: &Mihomo) -> Result<()> {
        apply_config_payload(mihomo, &self.original_yaml).await?;
        let process_result = if let Some(process) = self.process.take() {
            process.stop().await
        } else {
            Ok(())
        };
        if let Some(relay) = self.relay.take() {
            relay.stop().await;
        }
        process_result
    }
}

async fn wait_for_inbound(port: u16, process: &SingboxProcess) -> Result<()> {
    let deadline = tokio::time::Instant::now() + SINGBOX_READY_TIMEOUT;
    loop {
        anyhow::ensure!(
            process.is_running().await?,
            "sing-box exited before its inbound became ready"
        );
        if tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .is_ok()
        {
            return Ok(());
        }
        anyhow::ensure!(
            tokio::time::Instant::now() < deadline,
            "sing-box inbound did not become ready"
        );
        sleep(SINGBOX_READY_INTERVAL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::{net::SocketAddr, time::Duration};
    use tauri_plugin_mihomo::{MihomoContext, models::Protocol};
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    async fn read_request(stream: &mut tokio::net::TcpStream) -> Result<(String, Vec<u8>)> {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 1024];
        let header_end = loop {
            let read = stream.read(&mut chunk).await?;
            anyhow::ensure!(read > 0, "fixture closed before request headers");
            bytes.extend_from_slice(&chunk[..read]);
            if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                break index + 4;
            }
        };
        let headers = String::from_utf8(bytes[..header_end].to_vec())?;
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .context("fixture request has no content length")?;
        while bytes.len() < header_end + content_length {
            let read = stream.read(&mut chunk).await?;
            anyhow::ensure!(read > 0, "fixture closed before request body");
            bytes.extend_from_slice(&chunk[..read]);
        }
        Ok((headers, bytes[header_end..header_end + content_length].to_vec()))
    }

    #[tokio::test]
    async fn config_payload_is_sent_to_the_authenticated_local_api_without_a_path() -> Result<()> {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let address = listener.local_addr()?;
        let secret_config = "proxies:\n  - name: relay\n    password: in-memory-session-token\n";
        let fixture = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await?;
            let (headers, body) = read_request(&mut stream).await?;
            anyhow::ensure!(headers.starts_with("PUT /configs?force=true HTTP/1.1\r\n"));
            anyhow::ensure!(
                headers
                    .to_ascii_lowercase()
                    .contains("authorization: bearer api-secret")
            );
            let body: Value = serde_json::from_slice(&body)?;
            anyhow::ensure!(body.get("path").is_none());
            anyhow::ensure!(body["payload"] == secret_config);
            stream
                .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                .await?;
            Result::<()>::Ok(())
        });
        let mihomo = fixture_mihomo(address)?;

        apply_config_payload(&mihomo, secret_config).await?;

        fixture.await??;
        Ok(())
    }

    fn fixture_mihomo(address: SocketAddr) -> Result<Mihomo> {
        let context = MihomoContext::new(
            Protocol::Http,
            Some(address.ip().to_string()),
            Some(address.port()),
            Some("api-secret".into()),
            None,
            Duration::from_secs(3),
            MihomoContext::build_client(&Protocol::Http, None)?,
        );
        Ok(Mihomo::new(context))
    }
}
