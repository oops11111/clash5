use anyhow::{Context as _, Result};
use serde_json::{Map, Value as JsonValue, json};
use serde_yaml_ng::{Mapping, Value as YamlValue};

fn field<'a>(proxy: &'a YamlValue, name: &str) -> Option<&'a YamlValue> {
    proxy.as_mapping()?.get(YamlValue::String(name.to_owned()))
}

fn text(proxy: &YamlValue, name: &str) -> Result<String> {
    field(proxy, name)
        .and_then(YamlValue::as_str)
        .map(str::to_owned)
        .with_context(|| format!("sing-box outbound is missing {name}"))
}

fn number(proxy: &YamlValue, name: &str) -> Result<u64> {
    field(proxy, name)
        .and_then(YamlValue::as_u64)
        .with_context(|| format!("sing-box outbound has an invalid {name}"))
}

fn port(proxy: &YamlValue) -> Result<u16> {
    let value = number(proxy, "port")?;
    u16::try_from(value)
        .ok()
        .filter(|value| *value != 0)
        .context("sing-box outbound has an invalid port")
}

fn optional_text(proxy: &YamlValue, name: &str) -> Option<String> {
    text(proxy, name).ok().filter(|value| !value.is_empty())
}

fn put_text(target: &mut Map<String, JsonValue>, proxy: &YamlValue, source: &str, destination: &str) {
    if let Some(value) = optional_text(proxy, source) {
        target.insert(destination.to_owned(), JsonValue::String(value));
    }
}

fn optional_mapping<'a>(proxy: &'a YamlValue, name: &str) -> Option<&'a Mapping> {
    field(proxy, name).and_then(YamlValue::as_mapping)
}

fn tls_config(proxy: &YamlValue) -> Result<Option<JsonValue>> {
    let reality = optional_mapping(proxy, "reality-opts");
    let enabled = field(proxy, "tls").and_then(YamlValue::as_bool).unwrap_or(false) || reality.is_some();
    if !enabled {
        return Ok(None);
    }

    let mut tls = Map::new();
    tls.insert("enabled".into(), JsonValue::Bool(true));
    put_text(&mut tls, proxy, "servername", "server_name");
    if let Some(true) = field(proxy, "skip-cert-verify").and_then(YamlValue::as_bool) {
        tls.insert("insecure".into(), JsonValue::Bool(true));
    }
    if let Some(YamlValue::Sequence(alpn)) = field(proxy, "alpn") {
        tls.insert(
            "alpn".into(),
            JsonValue::Array(
                alpn.iter()
                    .filter_map(YamlValue::as_str)
                    .map(|value| JsonValue::String(value.to_owned()))
                    .collect(),
            ),
        );
    }
    if let Some(value) = optional_text(proxy, "client-fingerprint") {
        tls.insert("utls".into(), json!({"enabled": true, "fingerprint": value}));
    }
    if let Some(reality) = reality {
        let public_key = reality
            .get(YamlValue::String("public-key".to_owned()))
            .and_then(YamlValue::as_str)
            .context("sing-box Reality TLS is missing its public key")?;
        let short_id = reality
            .get(YamlValue::String("short-id".to_owned()))
            .and_then(YamlValue::as_str)
            .unwrap_or_default();
        tls.insert(
            "reality".into(),
            json!({"enabled": true, "public_key": public_key, "short_id": short_id}),
        );
    }
    Ok(Some(JsonValue::Object(tls)))
}

fn transport_config(proxy: &YamlValue) -> Result<Option<JsonValue>> {
    let Some(network) = field(proxy, "network").and_then(YamlValue::as_str) else {
        return Ok(None);
    };
    match network {
        "ws" => {
            let ws = optional_mapping(proxy, "ws-opts");
            let mut transport = Map::new();
            transport.insert("type".into(), JsonValue::String("ws".into()));
            if let Some(path) = ws
                .and_then(|value| value.get(YamlValue::String("path".to_owned())))
                .and_then(YamlValue::as_str)
            {
                transport.insert("path".into(), JsonValue::String(path.to_owned()));
            }
            if let Some(headers) = ws
                .and_then(|value| value.get(YamlValue::String("headers".to_owned())))
                .and_then(YamlValue::as_mapping)
            {
                let headers = headers
                    .iter()
                    .filter_map(|(key, value)| {
                        Some((key.as_str()?.to_owned(), JsonValue::String(value.as_str()?.to_owned())))
                    })
                    .collect();
                transport.insert("headers".into(), JsonValue::Object(headers));
            }
            Ok(Some(JsonValue::Object(transport)))
        }
        "grpc" => {
            let service_name = optional_mapping(proxy, "grpc-opts")
                .and_then(|value| value.get(YamlValue::String("grpc-service-name".to_owned())))
                .and_then(YamlValue::as_str)
                .unwrap_or_default();
            Ok(Some(json!({"type": "grpc", "service_name": service_name})))
        }
        "tcp" | "udp" => Ok(None),
        _ => anyhow::bail!("unsupported sing-box transport"),
    }
}

/// Converts a supported Mihomo landing proxy into a sing-box outbound.
/// Unsupported types fail closed rather than quietly changing the selected route.
pub fn map_landing_outbound(proxy: &YamlValue, tag: &str) -> Result<JsonValue> {
    let source_kind = text(proxy, "type")?;
    let kind = if source_kind == "ss" {
        "shadowsocks"
    } else {
        source_kind.as_str()
    };
    let mut outbound = Map::new();
    outbound.insert("type".into(), JsonValue::String(kind.to_owned()));
    outbound.insert("tag".into(), JsonValue::String(tag.to_owned()));
    outbound.insert("server".into(), JsonValue::String(text(proxy, "server")?));
    outbound.insert("server_port".into(), json!(port(proxy)?));

    match source_kind.as_str() {
        "ss" => {
            anyhow::ensure!(
                optional_text(proxy, "plugin").is_none(),
                "sing-box does not support this Shadowsocks plugin"
            );
            outbound.insert("method".into(), JsonValue::String(text(proxy, "cipher")?));
            outbound.insert("password".into(), JsonValue::String(text(proxy, "password")?));
            if field(proxy, "udp").and_then(YamlValue::as_bool) == Some(false) {
                outbound.insert("network".into(), JsonValue::String("tcp".into()));
            }
        }
        "trojan" | "hysteria2" => {
            outbound.insert("password".into(), JsonValue::String(text(proxy, "password")?));
        }
        "vmess" | "vless" | "tuic" => {
            outbound.insert("uuid".into(), JsonValue::String(text(proxy, "uuid")?));
            if kind == "vmess" {
                outbound.insert("alter_id".into(), json!(number(proxy, "alterId").unwrap_or(0)));
                put_text(&mut outbound, proxy, "cipher", "security");
            } else if kind == "vless" {
                put_text(&mut outbound, proxy, "flow", "flow");
            } else {
                outbound.insert("password".into(), JsonValue::String(text(proxy, "password")?));
                put_text(&mut outbound, proxy, "congestion-controller", "congestion_control");
                put_text(&mut outbound, proxy, "udp-relay-mode", "udp_relay_mode");
            }
        }
        "hysteria" => {
            put_text(&mut outbound, proxy, "auth", "auth");
            put_text(&mut outbound, proxy, "auth-str", "auth_str");
            put_text(&mut outbound, proxy, "obfs", "obfs");
        }
        _ => anyhow::bail!("unsupported sing-box landing outbound type"),
    }

    let tls = tls_config(proxy)?;
    if matches!(source_kind.as_str(), "trojan" | "tuic" | "hysteria" | "hysteria2") && tls.is_none() {
        anyhow::bail!("sing-box outbound requires TLS");
    }
    if source_kind == "hysteria2"
        && let Some(obfs_type) = optional_text(proxy, "obfs")
    {
        anyhow::ensure!(obfs_type == "salamander", "unsupported sing-box Hysteria2 obfuscation");
        outbound.insert(
            "obfs".into(),
            json!({"type": obfs_type, "password": text(proxy, "obfs-password")?}),
        );
    }
    if let Some(tls) = tls {
        outbound.insert("tls".into(), tls);
    }
    if let Some(transport) = transport_config(proxy)? {
        outbound.insert("transport".into(), transport);
    }
    Ok(JsonValue::Object(outbound))
}

/// Produces an isolated sing-box configuration with only a loopback SOCKS inbound and one exit.
pub fn singbox_config(proxy: &YamlValue, inbound_port: u16, tag: &str) -> Result<JsonValue> {
    anyhow::ensure!(inbound_port != 0, "sing-box inbound port must be nonzero");
    Ok(json!({
        "log": {"disabled": true},
        "inbounds": [{
            "type": "socks",
            "tag": "clash5-local",
            "listen": "127.0.0.1",
            "listen_port": inbound_port
        }],
        "outbounds": [map_landing_outbound(proxy, tag)?, {"type": "direct", "tag": "direct"}],
        "route": {"final": tag}
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_tls_vless_reality_and_websocket_transport() -> Result<()> {
        let proxy: YamlValue = serde_yaml_ng::from_str(
            "type: vless\nserver: edge.example\nport: 443\nuuid: 00000000-0000-0000-0000-000000000001\nnetwork: ws\ntls: true\nservername: cdn.example\nreality-opts:\n  public-key: public\n  short-id: abcd\nws-opts:\n  path: /tunnel\n  headers:\n    Host: cdn.example\n",
        )?;
        let outbound = map_landing_outbound(&proxy, "exit")?;
        assert_eq!(outbound["tls"]["server_name"], "cdn.example");
        assert_eq!(outbound["tls"]["reality"]["public_key"], "public");
        assert_eq!(outbound["transport"]["path"], "/tunnel");
        assert_eq!(outbound["transport"]["headers"]["Host"], "cdn.example");
        let config = singbox_config(&proxy, 18080, "exit")?;
        assert_eq!(config["inbounds"][0]["listen"], "127.0.0.1");
        assert!(config["inbounds"][0].get("tun").is_none());
        assert_eq!(config["route"]["final"], "exit");
        Ok(())
    }

    #[test]
    fn maps_shadowsocks_and_rejects_unsupported_types_without_secret_errors() -> Result<()> {
        let proxy: YamlValue = serde_yaml_ng::from_str(
            "type: ss\nserver: edge.example\nport: 8388\ncipher: aes-128-gcm\npassword: test-secret\n",
        )?;
        let outbound = map_landing_outbound(&proxy, "exit")?;
        assert_eq!(outbound["type"], "shadowsocks");
        assert_eq!(outbound["method"], "aes-128-gcm");
        assert_eq!(outbound["password"], "test-secret");

        let unsupported: YamlValue =
            serde_yaml_ng::from_str("type: ssr\nserver: edge.example\nport: 8388\npassword: do-not-leak\n")?;
        let error = map_landing_outbound(&unsupported, "exit").unwrap_err().to_string();
        assert!(!error.contains("do-not-leak"));

        let trojan_without_tls: YamlValue =
            serde_yaml_ng::from_str("type: trojan\nserver: edge.example\nport: 443\npassword: do-not-leak\n")?;
        let error = map_landing_outbound(&trojan_without_tls, "exit")
            .unwrap_err()
            .to_string();
        assert!(error.contains("requires TLS"));
        assert!(!error.contains("do-not-leak"));

        let unsupported_transport: YamlValue = serde_yaml_ng::from_str(
            "type: vless\nserver: edge.example\nport: 443\nuuid: 00000000-0000-0000-0000-000000000001\nnetwork: h2\n",
        )?;
        assert!(map_landing_outbound(&unsupported_transport, "exit").is_err());
        Ok(())
    }
}
