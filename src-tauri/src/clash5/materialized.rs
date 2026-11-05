use crate::utils::{dirs, help};
use anyhow::{Result, anyhow};
use clash5_data::{IdentityKey, NodeRef, Source, parse_proxy_node};
use serde::{Deserialize, Serialize};
use serde_yaml_ng::{Mapping, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

const FILE_NAME: &str = "clash5-materialized-proxies.yaml";
const MAX_STORED: usize = 4_096;

/// A credential-bearing proxy definition retained after a subscription refresh/removal.
/// The mapping is encrypted at rest; the stable ID is deliberately derived with the current
/// installation key when the file is read so identity-key rotation does not strand snapshots.
#[derive(Clone, Deserialize, Serialize)]
struct MaterializedProxyEntry {
    source: Source,
    #[serde(
        serialize_with = "crate::config::serialize_encrypted",
        deserialize_with = "crate::config::deserialize_encrypted"
    )]
    proxy: Mapping,
}

#[derive(Default, Deserialize, Serialize)]
struct MaterializedProxyFile {
    entries: Vec<MaterializedProxyEntry>,
}

fn path() -> Result<PathBuf> {
    Ok(dirs::app_home_dir()?.join(FILE_NAME))
}

fn source_key(source: &Source) -> String {
    match source {
        Source::Manual => "manual".into(),
        Source::Subscription { provider_id } => format!("subscription:{provider_id}"),
    }
}

fn entry_key(entry: &MaterializedProxyEntry, key: &IdentityKey) -> Result<String> {
    let node = parse_proxy_node(&Value::Mapping(entry.proxy.clone()))
        .map_err(|_| anyhow!("materialized proxy identity is invalid"))?;
    Ok(format!("{}:{}", source_key(&entry.source), node.stable_id_v2(key)?))
}

fn merge_yaml(file: &mut MaterializedProxyFile, source: Source, yaml: &str, key: &IdentityKey) -> Result<()> {
    let root: Value = serde_yaml_ng::from_str(yaml).map_err(|_| anyhow!("subscription YAML is invalid"))?;
    let Some(proxies) = root
        .as_mapping()
        .and_then(|mapping| mapping.get("proxies"))
        .and_then(Value::as_sequence)
    else {
        return Ok(());
    };

    let mut positions = BTreeMap::new();
    for (index, entry) in file.entries.iter().enumerate() {
        if let Ok(id) = entry_key(entry, key) {
            positions.insert(id, index);
        }
    }
    for proxy in proxies {
        let Some(mapping) = proxy.as_mapping() else {
            continue;
        };
        if parse_proxy_node(proxy).is_err() {
            continue;
        }
        let entry = MaterializedProxyEntry {
            source: source.clone(),
            proxy: mapping.clone(),
        };
        let id = entry_key(&entry, key)?;
        if let Some(index) = positions.get(&id).copied() {
            file.entries[index] = entry;
        } else {
            positions.insert(id, file.entries.len());
            file.entries.push(entry);
        }
    }
    if file.entries.len() > MAX_STORED {
        let remove = file.entries.len() - MAX_STORED;
        file.entries.drain(..remove);
    }
    Ok(())
}

pub(crate) async fn snapshot_profile_yaml(provider_id: String, yaml: String) -> Result<()> {
    if provider_id.trim().is_empty() {
        return Ok(());
    }
    let key = crate::config::profiles::clash5_identity_key()
        .await
        .map_err(|error| anyhow!("failed to load clash5 identity key: {error}"))?;
    let file_path = path()?;
    let mut file = if tokio::fs::try_exists(&file_path).await? {
        help::read_yaml(&file_path).await?
    } else {
        MaterializedProxyFile::default()
    };
    merge_yaml(&mut file, Source::Subscription { provider_id }, &yaml, &key)?;
    help::save_yaml(&file_path, &file, None).await
}

/// Load snapshots for a profile. The caller adds them to the catalog only; they are not copied
/// into the runtime config unless a chain or route actually references the stable identity.
pub(crate) async fn load_for_profile(profile_id: &str, key: &IdentityKey) -> Result<Vec<(NodeRef, Mapping)>> {
    let file_path = path()?;
    if !tokio::fs::try_exists(&file_path).await? {
        return Ok(vec![]);
    }
    let file: MaterializedProxyFile = help::read_yaml(&file_path).await?;
    let source = Source::Subscription {
        provider_id: profile_id.to_owned(),
    };
    let mut loaded = Vec::new();
    for entry in file.entries {
        if entry.source != source {
            continue;
        }
        let Ok(node) = parse_proxy_node(&Value::Mapping(entry.proxy.clone())) else {
            continue;
        };
        let Ok(stable_id) = node.stable_id_v2(key) else {
            continue;
        };
        loaded.push((
            NodeRef {
                source: source.clone(),
                stable_id,
            },
            entry.proxy,
        ));
    }
    Ok(loaded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml(name: &str, password: &str) -> String {
        format!(
            "proxies:\n  - name: {name}\n    type: trojan\n    server: edge.example.invalid\n    port: 443\n    password: {password}\n"
        )
    }

    #[test]
    fn merge_keeps_removed_nodes_and_replaces_matching_id() -> Result<()> {
        let key = IdentityKey::from_bytes(&[3; 32])?;
        let source = Source::Subscription {
            provider_id: "p".into(),
        };
        let mut file = MaterializedProxyFile::default();
        merge_yaml(&mut file, source.clone(), &yaml("old", "one"), &key)?;
        merge_yaml(&mut file, source.clone(), &yaml("renamed", "one"), &key)?;
        assert_eq!(file.entries.len(), 1);
        assert_eq!(file.entries[0].proxy["name"], "renamed");
        merge_yaml(&mut file, source, &yaml("new", "two"), &key)?;
        assert_eq!(file.entries.len(), 2);
        assert!(file.entries.iter().any(|entry| entry.proxy["name"] == "renamed"));
        assert!(file.entries.iter().any(|entry| entry.proxy["name"] == "new"));
        Ok(())
    }

    #[test]
    fn snapshots_are_scoped_to_their_subscription_source() -> Result<()> {
        let key = IdentityKey::from_bytes(&[4; 32])?;
        let mut file = MaterializedProxyFile::default();
        merge_yaml(
            &mut file,
            Source::Subscription {
                provider_id: "a".into(),
            },
            &yaml("node", "one"),
            &key,
        )?;
        merge_yaml(
            &mut file,
            Source::Subscription {
                provider_id: "b".into(),
            },
            &yaml("node", "one"),
            &key,
        )?;
        assert_eq!(file.entries.len(), 2);
        Ok(())
    }
}
