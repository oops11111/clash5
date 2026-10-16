use super::manual;
use crate::{config::profiles::with_clash5_binding_store, utils::dirs};
use anyhow::{Result, anyhow};
use clash5_data::{CompiledChain, NodeRef, ProxyChain, ScopedProxy, Source, compile_chains, parse_proxy_node};
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

fn add_proxy(catalog: &mut Vec<ScopedProxy>, source: Source, proxy: &Value, overrides: Option<&Mapping>) {
    let Ok(node) = parse_proxy_node(proxy) else {
        return;
    };
    let Ok(stable_id) = node.stable_id() else {
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

pub(crate) async fn catalog(config: &Mapping, profile_id: Option<&str>) -> Result<Vec<ScopedProxy>> {
    let mut catalog = vec![];
    if let Some(proxies) = config.get("proxies").and_then(Value::as_sequence) {
        for proxy in proxies {
            let Ok(node) = parse_proxy_node(proxy) else {
                continue;
            };
            let id = node.stable_id()?;
            let source = if proxy.get("name").and_then(Value::as_str) == Some(manual::runtime_name(&id).as_str()) {
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
            add_proxy(&mut catalog, source, proxy, None);
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
    let catalog = catalog(config, profile_id).await?;
    compile_chains(config, &chains, &catalog).map_err(anyhow::Error::from)
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
        );
        assert_eq!(catalog[0].reference.stable_id, expected);
        assert_eq!(catalog[0].proxy["dialer-proxy"], "user-front");
        assert_eq!(catalog[0].proxy["name"], "fixture");
        assert_eq!(catalog[0].proxy["server"], "127.0.0.1");
        Ok(())
    }
}
