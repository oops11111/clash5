use super::manual;
use crate::{config::profiles::with_clash5_binding_store, utils::dirs};
use anyhow::{Result, anyhow};
use clash5_data::{
    CompiledChain, IdentityKey, Node, NodeRecord, NodeRef, ProxyChain, ScopedProxy, Source, compile_chains,
    parse_proxy_node,
};
use serde_yaml_ng::{Mapping, Value};
use std::path::Path;

const PROVIDER_OVERRIDES: &[&str] = &[
    "tfo",
    "mptcp",
    "udp",
    "udp-over-tcp",
    "down",
    "up",
    "skip-cert-verify",
    "name-cert-verify",
    "dialer-proxy",
    "interface-name",
    "routing-mark",
    "ip-version",
];

fn add_proxy(
    catalog: &mut Vec<ScopedProxy>,
    source: Source,
    proxy: &Value,
    overrides: Option<&Mapping>,
    stable_id: Option<String>,
) {
    let Ok(node) = parse_proxy_node(proxy) else {
        return;
    };
    let stable_id = stable_id.or_else(|| node.stable_id().ok());
    let Some(stable_id) = stable_id else {
        return;
    };
    let Some(mut proxy) = proxy.as_mapping().cloned() else {
        return;
    };
    if let Some(overrides) = overrides {
        for field in PROVIDER_OVERRIDES {
            if let Some(value) = overrides.get(*field) {
                proxy.insert((*field).into(), value.clone());
            }
        }
    }
    catalog.push(ScopedProxy {
        reference: NodeRef { source, stable_id },
        proxy,
    });
}

fn matching_manual_record<'a>(
    proxy: &Value,
    node: &Node,
    identity_key: &IdentityKey,
    records: &'a [NodeRecord],
) -> Option<&'a NodeRecord> {
    let runtime_name = proxy.get("name").and_then(Value::as_str)?;
    let stable_id = node.stable_id_v2(identity_key).ok()?;
    records.iter().find(|record| {
        record.source == Source::Manual
            && record.stable_id == stable_id
            && manual::runtime_name(&record.stable_id) == runtime_name
    })
}

pub(crate) async fn catalog(config: &Mapping, profile_id: Option<&str>) -> Result<Vec<ScopedProxy>> {
    let mut catalog = vec![];
    let records = with_clash5_binding_store(|store| store.node_records(false)).await?;
    let manual_identity_key = if records.iter().any(|record| record.source == Source::Manual) {
        Some(crate::config::profiles::clash5_identity_key().await?)
    } else {
        None
    };
    if let Some(proxies) = config.get("proxies").and_then(Value::as_sequence) {
        for proxy in proxies {
            let Ok(node) = parse_proxy_node(proxy) else {
                continue;
            };
            let manual_record = manual_identity_key
                .as_ref()
                .and_then(|key| matching_manual_record(proxy, &node, key, &records));
            let source = if manual_record.is_some() {
                Source::Manual
            } else if let Some(profile_id) = profile_id {
                if proxy
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| name.starts_with("clash5-chain-"))
                {
                    continue;
                }
                Source::Subscription {
                    provider_id: profile_id.into(),
                }
            } else {
                continue;
            };
            add_proxy(
                &mut catalog,
                source,
                proxy,
                None,
                manual_record.map(|record| record.stable_id.clone()),
            );
        }
    }
    let Some(profile_id) = profile_id else {
        return Ok(catalog);
    };
    let Some(providers) = config.get("proxy-providers").and_then(Value::as_mapping) else {
        return Ok(catalog);
    };
    let root = tokio::fs::canonicalize(dirs::app_home_dir()?).await?;
    for (name, provider) in providers {
        let Some(name) = name.as_str() else { continue };
        let payload = match provider.get("type").and_then(Value::as_str) {
            Some("inline") => provider.get("payload").and_then(Value::as_sequence).cloned(),
            Some("http" | "file") => {
                let Some(path) = provider.get("path").and_then(Value::as_str) else {
                    continue;
                };
                read_provider_payload(&root, path).await
            }
            _ => None,
        };
        let source = Source::Subscription {
            provider_id: format!("proxy-provider:{profile_id}:{name}"),
        };
        for proxy in payload.unwrap_or_default() {
            add_proxy(
                &mut catalog,
                source.clone(),
                &proxy,
                provider.get("override").and_then(Value::as_mapping),
                None,
            );
        }
    }
    Ok(catalog)
}

async fn read_provider_payload(root: &Path, raw: &str) -> Option<Vec<Value>> {
    let path = tokio::fs::canonicalize(root.join(raw)).await.ok()?;
    if !path.starts_with(root) {
        return None;
    }
    let yaml = tokio::fs::read_to_string(path).await.ok()?;
    let value: Value = serde_yaml_ng::from_str(&yaml).ok()?;
    value.get("proxies")?.as_sequence().cloned()
}

pub(crate) async fn definitions() -> Result<Vec<ProxyChain>> {
    with_clash5_binding_store(|store| store.proxy_chains())
        .await
        .map_err(|_| anyhow!("C5-CHAIN-006"))
}

pub(crate) async fn generate(config: &mut Mapping, profile_id: Option<&str>) -> Result<Vec<CompiledChain>> {
    let chains: Vec<_> = definitions()
        .await?
        .into_iter()
        .filter(|chain| chain.profile_id.as_deref() == profile_id && chain.enabled)
        .collect();
    if chains.is_empty() {
        return Ok(vec![]);
    }
    let pools = with_clash5_binding_store(|store| store.node_pools())
        .await
        .map_err(|_| anyhow!("C5-CHAIN-006"))?;
    let catalog = catalog(config, profile_id).await?;
    compile_chains(config, &chains, &pools, &catalog).map_err(anyhow::Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_override_affects_the_copy_without_changing_raw_identity() -> Result<()> {
        let proxy: Value = serde_yaml_ng::from_str("name: fixture\ntype: http\nserver: 127.0.0.1\nport: 18082\n")?;
        let expected = parse_proxy_node(&proxy)?.stable_id()?;
        let overrides: Mapping = serde_yaml_ng::from_str(
            "dialer-proxy: user-front\nudp: true\nname: forbidden-rename\nserver: forbidden-endpoint\n",
        )?;
        let mut catalog = vec![];
        add_proxy(
            &mut catalog,
            Source::Subscription {
                provider_id: "provider-a".into(),
            },
            &proxy,
            Some(&overrides),
            None,
        );
        assert_eq!(catalog[0].reference.stable_id, expected);
        assert_eq!(catalog[0].proxy["dialer-proxy"], "user-front");
        assert_eq!(catalog[0].proxy["name"], "fixture");
        assert_eq!(catalog[0].proxy["server"], "127.0.0.1");
        Ok(())
    }

    #[test]
    fn manual_runtime_name_resolves_to_its_v2_identity() -> Result<()> {
        let key = IdentityKey::from_bytes(&[7; 32])?;
        let proxy: Value = serde_yaml_ng::from_str(
            "name: fixture\ntype: socks5\nserver: 127.0.0.1\nport: 18081\nusername: user\npassword: secret\n",
        )?;
        let node = parse_proxy_node(&proxy)?;
        let stable_id = node.stable_id_v2(&key)?;
        let runtime_name = manual::runtime_name(&stable_id);
        let mut runtime_proxy = proxy
            .as_mapping()
            .cloned()
            .ok_or_else(|| anyhow!("proxy fixture is not a mapping"))?;
        runtime_proxy.insert("name".into(), runtime_name.into());
        let record = NodeRecord {
            stable_id: stable_id.clone(),
            source: Source::Manual,
            display_name: "rate-limited manual node".into(),
            kind: node.kind.clone(),
            server: node.server.clone(),
            port: node.port,
            active: true,
            duplicate_count: 0,
            quality: None,
        };

        let records = [record];
        let found = matching_manual_record(&Value::Mapping(runtime_proxy), &node, &key, &records);
        assert_eq!(found.map(|record| record.stable_id.as_str()), Some(stable_id.as_str()));
        Ok(())
    }

    #[test]
    fn copied_manual_runtime_name_with_changed_credentials_is_not_manual() -> Result<()> {
        let key = IdentityKey::from_bytes(&[7; 32])?;
        let original: Value = serde_yaml_ng::from_str(
            "name: fixture\ntype: socks5\nserver: 127.0.0.1\nport: 18081\nusername: user\npassword: original-secret\n",
        )?;
        let original_node = parse_proxy_node(&original)?;
        let stable_id = original_node.stable_id_v2(&key)?;
        let runtime_name = manual::runtime_name(&stable_id);
        let record = NodeRecord {
            stable_id,
            source: Source::Manual,
            display_name: "manual node".into(),
            kind: original_node.kind.clone(),
            server: original_node.server.clone(),
            port: original_node.port,
            active: true,
            duplicate_count: 0,
            quality: None,
        };

        let subscription: Value = serde_yaml_ng::from_str(&format!(
            "name: {runtime_name}\ntype: socks5\nserver: 127.0.0.1\nport: 18081\nusername: user\npassword: substituted-secret\n"
        ))?;
        let subscription_node = parse_proxy_node(&subscription)?;

        assert!(matching_manual_record(&subscription, &subscription_node, &key, &[record]).is_none());
        Ok(())
    }
}
