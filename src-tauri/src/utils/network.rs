use crate::config::MixedPort;
use anyhow::{Context as _, Result, bail};
use base64::{Engine as _, engine::general_purpose};
use reqwest::{
    Client, Proxy, StatusCode,
    header::{HeaderMap, HeaderValue, USER_AGENT},
};
use smartstring::alias::String;
use std::{sync::Arc, time::Duration};
use sysproxy::Sysproxy;
use tauri::Url;

#[derive(Debug)]
pub struct HttpResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

/// Maximum decompressed response size accepted for subscription-style text downloads.
///
/// reqwest transparently decompresses gzip/deflate/br responses before yielding chunks, so
/// applying this limit while consuming chunks bounds the actual memory used by the caller.
pub const MAX_TEXT_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

fn append_response_chunk(body: &mut Vec<u8>, chunk: &[u8]) -> Result<()> {
    let next_len = body
        .len()
        .checked_add(chunk.len())
        .context("response body length overflow")?;
    if next_len > MAX_TEXT_RESPONSE_BYTES {
        bail!("response body exceeds 32 MiB after decompression");
    }
    body.extend_from_slice(chunk);
    Ok(())
}

async fn read_text_response(mut response: reqwest::Response) -> Result<String> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_TEXT_RESPONSE_BYTES as u64)
    {
        bail!("response body exceeds 32 MiB after decompression");
    }

    let mut body = Vec::with_capacity(
        response
            .content_length()
            .map_or(0, |length| length.min(MAX_TEXT_RESPONSE_BYTES as u64) as usize),
    );
    while let Some(chunk) = response.chunk().await.context("Failed to read response body")? {
        append_response_chunk(&mut body, &chunk)?;
    }
    Ok(std::string::String::from_utf8(body)
        .context("response body is not valid UTF-8")?
        .into())
}

impl HttpResponse {
    const fn new(status: StatusCode, headers: HeaderMap, body: String) -> Self {
        Self { status, headers, body }
    }

    pub const fn status(&self) -> StatusCode {
        self.status
    }

    pub const fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    pub fn text(&self) -> &str {
        &self.body
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ProxyType {
    None,
    Localhost,
    System,
}

#[derive(Debug, Clone, Copy)]
enum TlsRootMode {
    PlatformVerifier,
    StaticWebpkiRoots,
}

pub struct NetworkManager;

impl NetworkManager {
    pub const fn new() -> Self {
        Self
    }

    fn build_client(
        &self,
        proxy_url: Option<std::string::String>,
        default_headers: HeaderMap,
        accept_invalid_certs: bool,
        timeout_secs: Option<u64>,
        tls_root_mode: TlsRootMode,
    ) -> Result<Client> {
        let mut builder = Client::builder()
            .tls_backend_rustls()
            .redirect(reqwest::redirect::Policy::limited(10))
            .tcp_keepalive(Duration::from_secs(60))
            .pool_max_idle_per_host(0)
            .pool_idle_timeout(None);

        if matches!(tls_root_mode, TlsRootMode::StaticWebpkiRoots) {
            builder = builder.tls_backend_preconfigured(Self::build_static_webpki_tls_config()?);
        }

        // 设置代理
        if let Some(proxy_str) = proxy_url {
            let proxy = Proxy::all(proxy_str)?;
            builder = builder.proxy(proxy);
        } else {
            builder = builder.no_proxy();
        }

        builder = builder.default_headers(default_headers);

        // SSL/TLS
        if accept_invalid_certs {
            builder = builder.danger_accept_invalid_certs(true);
        }

        // 超时设置
        if let Some(secs) = timeout_secs {
            builder = builder
                .timeout(Duration::from_secs(secs))
                .connect_timeout(Duration::from_secs(secs.min(30)));
        }

        Ok(builder.build()?)
    }

    fn build_static_webpki_tls_config() -> Result<rustls::ClientConfig> {
        let root_store = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let mut config =
            rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()?
                .with_root_certificates(root_store)
                .with_no_client_auth();

        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        Ok(config)
    }

    fn should_retry_with_static_webpki_roots(err: &anyhow::Error) -> bool {
        if err.chain().any(Self::is_legacy_tls_protocol_error) {
            return false;
        }

        err.chain().any(|e| {
            let msg = e.to_string().to_ascii_lowercase();
            [
                "certificate",
                "cert",
                "tls",
                "ssl",
                "rustls",
                "webpki",
                "revocation",
                "ocsp",
                "crl",
                "issuer",
                "unknownissuer",
            ]
            .iter()
            .any(|kw| msg.contains(kw))
        })
    }

    fn context_reqwest_error(err: reqwest::Error, context: &'static str) -> anyhow::Error {
        let legacy_tls = Self::is_legacy_tls_protocol_error(&err);
        let err = anyhow::Error::new(err).context(context);

        if legacy_tls {
            err.context("Subscription server uses legacy TLS; only TLS 1.2/1.3 is supported. TLS 1.0/1.1 is insecure")
        } else {
            err
        }
    }

    fn is_legacy_tls_protocol_error(err: &(dyn std::error::Error + 'static)) -> bool {
        let detail = format!("{err:#?}").to_ascii_lowercase();
        detail.contains("protocolversion") || detail.contains("protocol version")
    }

    async fn get_with_tls_mode(
        &self,
        url: &str,
        proxy_type: ProxyType,
        timeout_secs: Option<u64>,
        user_agent: Option<String>,
        accept_invalid_certs: bool,
        tls_root_mode: TlsRootMode,
    ) -> Result<HttpResponse> {
        let mut parsed = Url::parse(url)?;
        let mut extra_headers = HeaderMap::new();

        if !parsed.username().is_empty() {
            let username = percent_encoding::percent_decode_str(parsed.username())
                .decode_utf8_lossy()
                .into_owned();
            let password = percent_encoding::percent_decode_str(parsed.password().unwrap_or_default())
                .decode_utf8_lossy()
                .into_owned();
            let auth_str = format!("{}:{}", username, password);
            let encoded = general_purpose::STANDARD.encode(auth_str);
            extra_headers.insert("Authorization", HeaderValue::from_str(&format!("Basic {}", encoded))?);
        }

        parsed.set_username("").ok();
        parsed.set_password(None).ok();

        // 创建请求
        let client = self
            .create_request_with_tls_mode(
                proxy_type,
                timeout_secs,
                user_agent,
                accept_invalid_certs,
                tls_root_mode,
            )
            .await?;

        let mut request_builder = client.get(parsed);

        for (key, value) in extra_headers.iter() {
            request_builder = request_builder.header(key, value);
        }

        let response = match request_builder.send().await {
            Ok(resp) => resp,
            Err(e) => {
                return Err(Self::context_reqwest_error(e, "Request failed"));
            }
        };

        let status = response.status();
        let headers = response.headers().to_owned();
        let body = match read_text_response(response).await {
            Ok(text) => text.into(),
            Err(e) => return Err(e),
        };

        Ok(HttpResponse::new(status, headers, body))
    }

    async fn create_request_with_tls_mode(
        &self,
        proxy_type: ProxyType,
        timeout_secs: Option<u64>,
        user_agent: Option<String>,
        accept_invalid_certs: bool,
        tls_root_mode: TlsRootMode,
    ) -> Result<Client> {
        let proxy_url: Option<std::string::String> = match proxy_type {
            ProxyType::None => None,
            ProxyType::Localhost => {
                let port = MixedPort::desired().await;
                Some(format!("http://127.0.0.1:{port}"))
            }
            ProxyType::System => {
                let p = Sysproxy::get_system_proxy()
                    .map_err(|error| anyhow::anyhow!("system proxy could not be read: {error}"))?;
                if !p.enable {
                    bail!("system proxy is not enabled");
                }
                Some(format!("http://{}:{}", p.host, p.port))
            }
        };

        let mut headers = HeaderMap::new();

        // 设置 User-Agent
        if let Some(ua) = user_agent {
            headers.insert(USER_AGENT, HeaderValue::from_str(ua.as_str())?);
        } else {
            headers.insert(
                USER_AGENT,
                HeaderValue::from_str(&format!("clash5/v{}", env!("CARGO_PKG_VERSION")))?,
            );
        }

        self.build_client(proxy_url, headers, accept_invalid_certs, timeout_secs, tls_root_mode)
    }

    pub async fn get(
        &self,
        url: &str,
        proxy_type: ProxyType,
        timeout_secs: Option<u64>,
        user_agent: Option<String>,
        accept_invalid_certs: bool,
    ) -> Result<HttpResponse> {
        let platform_result = self
            .get_with_tls_mode(
                url,
                proxy_type,
                timeout_secs,
                user_agent.clone(),
                accept_invalid_certs,
                TlsRootMode::PlatformVerifier,
            )
            .await;

        match platform_result {
            Ok(response) => Ok(response),
            Err(err) if !accept_invalid_certs && Self::should_retry_with_static_webpki_roots(&err) => self
                .get_with_tls_mode(
                    url,
                    proxy_type,
                    timeout_secs,
                    user_agent,
                    accept_invalid_certs,
                    TlsRootMode::StaticWebpkiRoots,
                )
                .await
                .map_err(|fallback_err| {
                    fallback_err.context("static webpki roots fallback failed after platform TLS verifier failed")
                }),
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_TEXT_RESPONSE_BYTES, append_response_chunk};

    #[test]
    fn decompressed_response_limit_is_enforced_without_overflow() {
        let mut body = vec![0_u8; MAX_TEXT_RESPONSE_BYTES - 1];
        assert!(append_response_chunk(&mut body, &[0]).is_ok());
        assert!(append_response_chunk(&mut body, &[0]).is_err());
    }
}
