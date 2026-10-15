use anyhow::{Context as _, Result};
use reqwest::Method;
use tauri_plugin_mihomo::Mihomo;

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
