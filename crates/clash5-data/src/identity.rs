use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt, net::IpAddr};

pub type StableId = String;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataError {
    InvalidInput,
    NotFound,
    Database,
    SchemaVersion,
    IdentityRecovery,
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidInput => "INVALID_INPUT",
            Self::NotFound => "NOT_FOUND",
            Self::Database => "DATABASE_ERROR",
            Self::SchemaVersion => "SCHEMA_VERSION",
            Self::IdentityRecovery => "IDENTITY_RECOVERY",
        })
    }
}
impl std::error::Error for DataError {}
impl From<rusqlite::Error> for DataError {
    fn from(_: rusqlite::Error) -> Self {
        Self::Database
    }
}

// Credentials deliberately have no Debug implementation and are never persisted.
#[derive(Clone, Serialize, Deserialize)]
pub struct Node {
    pub kind: String,
    pub server: String,
    pub port: u16,
    pub credentials: BTreeMap<String, String>,
    pub display_name: String,
    #[serde(default)]
    pub transport: String,
    #[serde(default)]
    pub sni: String,
}

/// Installation-local secret used only for v2 stable identities.
/// It intentionally does not implement `Debug` or serialization.
#[derive(Clone, PartialEq, Eq)]
pub struct IdentityKey([u8; 32]);

impl IdentityKey {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DataError> {
        let key: [u8; 32] = bytes.try_into().map_err(|_| DataError::InvalidInput)?;
        Ok(Self(key))
    }

    pub fn key_id(&self) -> String {
        let mut digest = Sha256::new();
        digest.update(b"clash5-identity-key-id-v1");
        digest.update(self.0);
        hex(&digest.finalize()[..16])
    }
}

pub(crate) fn text(value: &str) -> Result<(), DataError> {
    if value.is_empty() || value.contains('\0') {
        return Err(DataError::InvalidInput);
    }
    Ok(())
}

impl Node {
    pub(crate) fn endpoint_compatibility(&self) -> Result<(String, String, u16, String, String), DataError> {
        Ok((
            self.normalized_kind()?,
            normalize_server(&self.server)?,
            self.port,
            normalize_transport(&self.normalized_kind()?, &self.transport)?,
            normalize_sni(&self.sni)?,
        ))
    }

    pub fn stable_id_v2(&self, key: &IdentityKey) -> Result<StableId, DataError> {
        let kind = self.normalized_kind()?;
        let server = normalize_server(&self.server)?;
        let transport = normalize_transport(&kind, &self.transport)?;
        let sni = normalize_sni(&self.sni)?;

        let mut credential_bytes = Vec::new();
        let mut sorted: Vec<_> = self.credentials.iter().collect();
        sorted.sort_by(|(left, _), (right, _)| left.as_bytes().cmp(right.as_bytes()));
        for (name, value) in sorted {
            text(name)?;
            let value = normalize_credential(name, value)?;
            append_field(&mut credential_bytes, name.as_bytes());
            append_field(&mut credential_bytes, value.as_bytes());
        }
        let credential_fingerprint = hmac_sha256(&key.0, b"clash5-credential-fingerprint-v2", &credential_bytes)?;

        let mut payload = Vec::new();
        append_field(&mut payload, b"clash5-node-v2");
        append_field(&mut payload, kind.as_bytes());
        append_field(&mut payload, server.as_bytes());
        append_field(&mut payload, &self.port.to_be_bytes());
        append_field(&mut payload, transport.as_bytes());
        append_field(&mut payload, sni.as_bytes());
        append_field(&mut payload, &credential_fingerprint);
        let digest = hmac_sha256(&key.0, b"clash5-node-identity-v2", &payload)?;
        Ok(format!("v2:{}", hex(&digest)))
    }

    pub fn stable_id(&self) -> Result<StableId, DataError> {
        let kind = self.normalized_kind()?;
        let server = normalize_server(&self.server)?;
        let mut credentials = Vec::new();
        let mut sorted: Vec<_> = self.credentials.iter().collect();
        // v1 was specified by the prototype's JavaScript UTF-16 key ordering.
        sorted.sort_by(|(left, _), (right, _)| left.encode_utf16().cmp(right.encode_utf16()));
        for (key, value) in sorted {
            text(key)?;
            let value = normalize_credential(key, value)?;
            credentials.push((key, value));
        }
        let payload = serde_json::to_vec(&("clash5-node-v1", kind, server, self.port, credentials))
            .map_err(|_| DataError::InvalidInput)?;
        let digest = Sha256::digest(payload);
        Ok(format!("v1:{}", hex(&digest)))
    }

    fn normalized_kind(&self) -> Result<String, DataError> {
        let kind = self.kind.trim().to_ascii_lowercase();
        if self.port == 0
            || kind.is_empty()
            || !kind.as_bytes()[0].is_ascii_alphanumeric()
            || !kind
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(DataError::InvalidInput);
        }
        Ok(kind)
    }
}

fn normalize_server(value: &str) -> Result<String, DataError> {
    let host = value.trim();
    let host = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V6(ip)) => Ok(format!("[{ip}]")),
        Ok(ip) => Ok(ip.to_string()),
        Err(_) => {
            let dns = idna::domain_to_ascii(host.strip_suffix('.').unwrap_or(host))
                .map_err(|_| DataError::InvalidInput)?
                .to_ascii_lowercase();
            if dns.len() > 253
                || !dns.split('.').all(|label| {
                    !label.is_empty()
                        && label.len() <= 63
                        && label.as_bytes()[0].is_ascii_alphanumeric()
                        && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                        && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                })
            {
                return Err(DataError::InvalidInput);
            }
            Ok(dns)
        }
    }
}

fn normalize_transport(kind: &str, value: &str) -> Result<String, DataError> {
    let default = match kind {
        "hysteria" | "hysteria2" | "tuic" => "quic",
        "wireguard" => "wireguard",
        "ss" | "ssr" | "trojan" | "vless" | "vmess" | "anytls" | "socks5" | "http" | "ssh" | "shadowtls" => "tcp",
        _ => return Err(DataError::InvalidInput),
    };
    let transport = if value.trim().is_empty() {
        default.to_owned()
    } else {
        value.trim().to_ascii_lowercase()
    };
    if !transport.is_ascii() || !transport.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err(DataError::InvalidInput);
    }
    if matches!(kind, "wireguard") && transport != "wireguard" {
        return Err(DataError::InvalidInput);
    }
    Ok(transport)
}

pub(crate) fn normalize_sni(value: &str) -> Result<String, DataError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(String::new());
    }
    let value = value
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .unwrap_or(value);
    if let Ok(ip) = value.parse::<IpAddr>() {
        return Ok(match ip {
            IpAddr::V6(v6) => format!("[{v6}]"),
            _ => ip.to_string(),
        });
    }
    normalize_server(value)
}

fn normalize_credential(key: &str, value: &str) -> Result<String, DataError> {
    if key != "uuid" {
        return Ok(value.to_owned());
    }
    let uuid = value.trim();
    let uuid = uuid.strip_prefix('{').and_then(|v| v.strip_suffix('}')).unwrap_or(uuid);
    let valid = uuid.len() == 32 && uuid.bytes().all(|b| b.is_ascii_hexdigit())
        || uuid.len() == 36
            && uuid.bytes().enumerate().all(|(i, b)| {
                if [8, 13, 18, 23].contains(&i) {
                    b == b'-'
                } else {
                    b.is_ascii_hexdigit()
                }
            });
    if !valid {
        return Err(DataError::InvalidInput);
    }
    Ok(uuid.replace('-', "").to_ascii_lowercase())
}

fn append_field(output: &mut Vec<u8>, field: &[u8]) {
    output.extend_from_slice(&(field.len() as u64).to_be_bytes());
    output.extend_from_slice(field);
}

fn hmac_sha256(key: &[u8], domain: &[u8], payload: &[u8]) -> Result<Vec<u8>, DataError> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| DataError::InvalidInput)?;
    mac.update(domain);
    mac.update(&[0]);
    mac.update(payload);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn hex(value: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(value.len() * 2);
    for byte in value {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 15) as usize] as char);
    }
    output
}
