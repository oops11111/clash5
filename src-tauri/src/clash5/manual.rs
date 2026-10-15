use crate::utils::{dirs, help};
use anyhow::{Result, anyhow, bail};
use clash5_data::{Node, StableId, parse_proxy_node};
use serde::{Deserialize, Serialize};
use serde_yaml_ng::{Mapping, Sequence, Value};
use std::{
    collections::{BTreeSet, HashMap},
    io::ErrorKind,
    path::PathBuf,
};

const FILE_NAME: &str = "clash5-manual-proxies.yaml";
const MAX_IMPORT: usize = 256;
const MAX_STORED: usize = 4_096;

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct ManualProxyEntry {
    display_name: String,
    #[serde(
        serialize_with = "crate::config::serialize_encrypted",
        deserialize_with = "crate::config::deserialize_encrypted"
    )]
    proxy: Mapping,
}

#[derive(Default, Deserialize, Serialize)]
pub(crate) struct ManualProxyFile {
    entries: Vec<ManualProxyEntry>,
}

pub(crate) struct ManualImport {
    pub nodes: Vec<Node>,
    pub changed_ids: Vec<StableId>,
}

pub(crate) fn runtime_name(stable_id: &str) -> String {
    format!("clash5-{}", stable_id.replace(':', "-"))
}

fn entry_node(entry: &ManualProxyEntry) -> Result<Node> {
    let mut display_proxy = entry.proxy.clone();
    display_proxy.insert("name".into(), entry.display_name.clone().into());
    let node =
        parse_proxy_node(&Value::Mapping(display_proxy)).map_err(|_| anyhow!("invalid manual proxy identity"))?;
    let expected_name = runtime_name(&node.stable_id()?);
    if entry.proxy.get("name").and_then(Value::as_str) != Some(expected_name.as_str()) {
        bail!("manual proxy runtime name does not match its identity");
    }
    Ok(node)
}

impl ManualProxyFile {
    pub(crate) fn add_import(&mut self, proxies: Vec<Value>) -> Result<ManualImport> {
        if proxies.is_empty() || proxies.len() > MAX_IMPORT {
            bail!("manual proxy import size is out of range");
        }
        let mut entries = self.entries.clone();
        let mut positions = HashMap::new();
        for (index, entry) in entries.iter().enumerate() {
            positions.insert(entry_node(entry)?.stable_id()?, index);
        }
        let mut imported = Vec::with_capacity(proxies.len());
        let mut batch = HashMap::new();
        let mut changed_ids = BTreeSet::new();
        for proxy in proxies {
            let mut mapping = proxy
                .as_mapping()
                .ok_or_else(|| anyhow!("invalid manual proxy"))?
                .clone();
            let node = parse_proxy_node(&Value::Mapping(mapping.clone()))
                .map_err(|_| anyhow!("invalid manual proxy identity"))?;
            if node.display_name.trim().is_empty() || node.display_name.contains('\0') {
                bail!("invalid manual proxy name");
            }
            let stable_id = node.stable_id()?;
            mapping.insert("name".into(), runtime_name(&stable_id).into());
            let entry = ManualProxyEntry {
                display_name: node.display_name.clone(),
                proxy: mapping,
            };
            if let Some(previous) = batch.insert(stable_id.clone(), entry.proxy.clone())
                && previous != entry.proxy
            {
                bail!("ambiguous manual proxy identity in import");
            }
            if let Some(&index) = positions.get(&stable_id) {
                if entries[index].proxy != entry.proxy {
                    changed_ids.insert(stable_id.clone());
                }
                entries[index] = entry;
            } else {
                changed_ids.insert(stable_id.clone());
                positions.insert(stable_id, entries.len());
                entries.push(entry);
            }
            imported.push(node);
        }
        if entries.len() > MAX_STORED {
            bail!("manual proxy inventory is full");
        }
        self.entries = entries;
        Ok(ManualImport {
            nodes: imported,
            changed_ids: changed_ids.into_iter().collect(),
        })
    }

    pub(crate) fn runtime_proxies(&self) -> Result<Sequence> {
        let mut names = std::collections::HashSet::new();
        let mut proxies = Sequence::new();
        for entry in &self.entries {
            let node = entry_node(entry)?;
            if !names.insert(node.stable_id()?) {
                bail!("duplicate manual proxy identity");
            }
            proxies.push(Value::Mapping(entry.proxy.clone()));
        }
        Ok(proxies)
    }
}

pub(crate) fn path() -> Result<PathBuf> {
    Ok(dirs::app_home_dir()?.join(FILE_NAME))
}

pub(crate) async fn load() -> Result<ManualProxyFile> {
    let path = path()?;
    if !tokio::fs::try_exists(&path).await? {
        return Ok(ManualProxyFile::default());
    }
    help::read_yaml(&path).await
}

pub(crate) async fn save(file: &ManualProxyFile) -> Result<()> {
    help::save_yaml(&path()?, file, None).await
}

pub(crate) async fn restore(previous: Option<&str>) -> Result<()> {
    let path = path()?;
    match previous {
        Some(yaml) => help::save_yaml_str(&path, yaml).await,
        None => match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn proxy(name: &str, network: &str) -> Value {
        serde_yaml_ng::from_str(&format!(
            "name: {name}\ntype: ss\nserver: edge.example.invalid\nport: 8388\ncipher: aes-128-gcm\npassword: test-only-secret\nnetwork: {network}\n"
        ))
        .unwrap()
    }

    #[test]
    fn manual_import_uses_stable_runtime_names_and_rejects_ambiguous_batch() {
        let mut file = ManualProxyFile::default();
        let node = file.add_import(vec![proxy("Original", "tcp")]).unwrap().nodes.remove(0);
        let name = runtime_name(&node.stable_id().unwrap());
        assert_eq!(file.runtime_proxies().unwrap()[0]["name"], name);

        let renamed = file.add_import(vec![proxy("Renamed", "tcp")]).unwrap();
        assert!(renamed.changed_ids.is_empty());
        assert_eq!(file.entries.len(), 1);
        assert_eq!(entry_node(&file.entries[0]).unwrap().display_name, "Renamed");

        let changed = file.add_import(vec![proxy("Renamed", "ws")]).unwrap();
        assert_eq!(changed.changed_ids, vec![node.stable_id().unwrap()]);

        assert!(file.add_import(vec![proxy("One", "tcp"), proxy("Two", "ws")]).is_err());
        assert_eq!(entry_node(&file.entries[0]).unwrap().display_name, "Renamed");
    }
}
