use crate::{DataError, Node};
use serde_yaml_ng::Value;
use std::collections::BTreeMap;

const CREDENTIAL_FIELDS: &[&str] = &[
    "uuid",
    "password",
    "username",
    "cipher",
    "alterId",
    "auth",
    "auth-str",
    "token",
    "private-key",
    "public-key",
    "pre-shared-key",
    "ports",
    "obfs-password",
];

fn scalar(value: &Value) -> Result<String, DataError> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        _ => Err(DataError::InvalidInput),
    }
}

fn field<'a>(mapping: &'a serde_yaml_ng::Mapping, key: &str) -> Option<&'a Value> {
    mapping.get(Value::String(key.into()))
}

/// Parse one Mihomo proxy definition into a transient identity input.
pub fn parse_proxy_node(proxy: &Value) -> Result<Node, DataError> {
    let mapping = proxy.as_mapping().ok_or(DataError::InvalidInput)?;
    let required =
        |key: &str| -> Result<String, DataError> { scalar(field(mapping, key).ok_or(DataError::InvalidInput)?) };
    let port: u16 = required("port")?.parse().map_err(|_| DataError::InvalidInput)?;
    let mut credentials = BTreeMap::new();
    for &key in CREDENTIAL_FIELDS {
        if let Some(value) = field(mapping, key) {
            credentials.insert(key.into(), scalar(value)?);
        }
    }
    let node = Node {
        kind: required("type")?,
        server: required("server")?,
        port,
        credentials,
        display_name: required("name")?,
    };
    let identity_fields: &[&str] = match node.kind.as_str() {
        "vless" | "vmess" | "tuic" => &["uuid"],
        "ss" | "ssr" | "trojan" | "hysteria2" | "anytls" => &["password"],
        "wireguard" => &["private-key"],
        "ssh" => &["username"],
        "hysteria" => &["auth", "auth-str"],
        "socks5" | "http" | "shadowtls" | "naive" => &[],
        _ => return Err(DataError::InvalidInput),
    };
    if !identity_fields.is_empty()
        && !identity_fields
            .iter()
            .any(|key| node.credentials.get(*key).is_some_and(|value| !value.is_empty()))
    {
        return Err(DataError::InvalidInput);
    }
    node.stable_id()?;
    Ok(node)
}

/// Extract inline proxy identities from a downloaded Clash YAML profile.
/// External proxy-provider contents are managed separately by Mihomo and are not part of this snapshot.
pub fn parse_inline_nodes(yaml: &str) -> Result<Option<Vec<Node>>, DataError> {
    let root: Value = serde_yaml_ng::from_str(yaml).map_err(|_| DataError::InvalidInput)?;
    let root = root.as_mapping().ok_or(DataError::InvalidInput)?;
    let Some(proxies) = field(root, "proxies") else {
        // A rules-only or external-provider profile does not establish an inline-node snapshot.
        return Ok(None);
    };
    let proxies = proxies.as_sequence().ok_or(DataError::InvalidInput)?;
    let mut nodes = Vec::with_capacity(proxies.len());
    for proxy in proxies {
        nodes.push(parse_proxy_node(proxy)?);
    }
    Ok(Some(nodes))
}

/// A remote refresh owns only proxy lines. Keep rules, groups and other local
/// profile customizations from the committed file, including their ordering.
/// This is a semantic YAML merge; comments in the committed file are not retained.
pub fn replace_subscription_lines(previous: &str, incoming: &str) -> Result<String, DataError> {
    let mut old: serde_yaml_ng::Mapping = serde_yaml_ng::from_str(previous).map_err(|_| DataError::InvalidInput)?;
    let next: serde_yaml_ng::Mapping = serde_yaml_ng::from_str(incoming).map_err(|_| DataError::InvalidInput)?;
    let proxies = field(&next, "proxies");
    let providers = field(&next, "proxy-providers");
    if proxies.is_none() && providers.is_none() {
        return Err(DataError::InvalidInput);
    }
    if proxies.is_some_and(|value| !value.is_sequence()) || providers.is_some_and(|value| !value.is_mapping()) {
        return Err(DataError::InvalidInput);
    }
    for key in ["proxies", "proxy-providers"] {
        let key = Value::String(key.into());
        old.remove(&key);
        if let Some(value) = next.get(&key) {
            old.insert(key, value.clone());
        }
    }
    serde_yaml_ng::to_string(&old).map_err(|_| DataError::InvalidInput)
}
