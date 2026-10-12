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
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidInput => "INVALID_INPUT",
            Self::NotFound => "NOT_FOUND",
            Self::Database => "DATABASE_ERROR",
            Self::SchemaVersion => "SCHEMA_VERSION",
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
}

pub(crate) fn text(value: &str) -> Result<(), DataError> {
    if value.is_empty() || value.contains('\0') {
        return Err(DataError::InvalidInput);
    }
    Ok(())
}

impl Node {
    pub fn stable_id(&self) -> Result<StableId, DataError> {
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
        let host = self.server.trim();
        let host = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
        let server = match host.parse::<IpAddr>() {
            Ok(IpAddr::V6(ip)) => format!("[{ip}]"),
            Ok(ip) => ip.to_string(),
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
                dns
            }
        };
        let mut credentials = Vec::new();
        let mut sorted: Vec<_> = self.credentials.iter().collect();
        // v1 was specified by the prototype's JavaScript UTF-16 key ordering.
        sorted.sort_by(|(left, _), (right, _)| left.encode_utf16().cmp(right.encode_utf16()));
        for (key, value) in sorted {
            text(key)?;
            let value = if key == "uuid" {
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
                uuid.replace('-', "").to_ascii_lowercase()
            } else {
                value.clone()
            };
            credentials.push((key, value));
        }
        let payload = serde_json::to_vec(&("clash5-node-v1", kind, server, self.port, credentials))
            .map_err(|_| DataError::InvalidInput)?;
        let digest = Sha256::digest(payload);
        Ok(format!("v1:{digest:x}"))
    }
}
