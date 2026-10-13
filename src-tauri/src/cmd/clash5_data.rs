use super::{CmdResult, coded_error};
use clash5_data::{Binding, DataError, Node, NodeQuality, NodeRecord, QualityStatus, Source, parse_inline_nodes};
use futures::{StreamExt as _, stream};
use serde::Serialize;
use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    config::Config,
    core::{handle::Handle, proxy_view::ProxyNodeSource, runtime_bundle::remote_providers_of},
    utils::dirs,
};

const SHORT_PROBE_MS: u32 = 1_500;
const HANDSHAKE_PROBE_MS: u32 = 10_000;
const PROBE_CONCURRENCY: usize = 16;
const SHORT_PROBE_URL: &str = "http://cp.cloudflare.com/generate_204";
const HANDSHAKE_PROBE_URL: &str = "https://cp.cloudflare.com/generate_204";
static PROBE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn failure(error: DataError) -> super::CommandFailure {
    coded_error(&error.to_string(), "clash5 node data operation failed")
}

#[derive(Serialize)]
pub struct ManualNodeIdentity {
    stable_id: String,
}

#[tauri::command]
pub async fn clash5_register_manual_node(node: Node) -> CmdResult<ManualNodeIdentity> {
    crate::config::profiles::with_clash5_binding_store(move |store| {
        let stable_id = store.upsert_manual(&node)?;
        Ok(ManualNodeIdentity { stable_id })
    })
    .await
    .map_err(failure)
}

#[tauri::command]
pub async fn clash5_bind_node(binding: Binding) -> CmdResult {
    crate::config::profiles::with_clash5_binding_store(move |store| store.upsert_binding(&binding))
        .await
        .map_err(failure)
}

#[tauri::command]
pub async fn clash5_list_bindings(source: Option<Source>) -> CmdResult<Vec<Binding>> {
    crate::config::profiles::with_clash5_binding_store(move |store| store.bindings(source.as_ref()))
        .await
        .map_err(failure)
}

#[tauri::command]
pub async fn clash5_list_nodes(include_inactive: Option<bool>) -> CmdResult<Vec<NodeRecord>> {
    refresh_runtime_proxy_providers().await?;
    crate::config::profiles::with_clash5_binding_store(move |store| {
        store.node_records(include_inactive.unwrap_or(false))
    })
    .await
    .map_err(failure)
}

fn proxy_provider_source_id(profile_id: &str, provider_name: &str) -> String {
    format!("proxy-provider:{profile_id}:{provider_name}")
}

async fn refresh_runtime_proxy_providers() -> CmdResult {
    let Some(profile_id) = Config::profiles().await.latest_arc().current.clone() else {
        return Ok(());
    };
    let runtime = Config::runtime().await;
    let applied = runtime.data_arc();
    let Some(config) = applied.config.as_ref() else {
        return Ok(());
    };
    let app_dir = dirs::app_home_dir().map_err(super::CommandFailure::plain)?;
    let providers = remote_providers_of(config, &app_dir).map_err(super::CommandFailure::plain)?;
    let declared: std::collections::HashSet<_> = providers
        .iter()
        .filter(|provider| provider.section == "proxy-providers")
        .map(|provider| proxy_provider_source_id(&profile_id, &provider.name))
        .collect();
    let mut snapshots = Vec::new();
    for provider in providers
        .into_iter()
        .filter(|provider| provider.section == "proxy-providers")
    {
        let path = app_dir.join(&provider.provider.destination);
        let Ok(yaml) = tokio::fs::read_to_string(path).await else {
            continue;
        };
        let Ok(Some(nodes)) = parse_inline_nodes(&yaml) else {
            continue;
        };
        snapshots.push((proxy_provider_source_id(&profile_id, &provider.name), nodes));
    }
    crate::config::profiles::with_clash5_binding_store(move |store| {
        let stale: std::collections::HashSet<_> = store
            .node_records(true)?
            .into_iter()
            .filter_map(|record| match record.source {
                Source::Subscription { provider_id }
                    if provider_id.starts_with("proxy-provider:") && !declared.contains(&provider_id) =>
                {
                    Some(provider_id)
                }
                _ => None,
            })
            .collect();
        for source_id in stale {
            store.refresh_provider(&source_id, &[])?;
        }
        for (source_id, nodes) in &snapshots {
            store.refresh_provider(source_id, nodes)?;
        }
        Ok(())
    })
    .await
    .map_err(failure)
}

#[derive(Serialize)]
pub struct ProbeBatch {
    pub probed: usize,
    pub cached: usize,
    pub unavailable: usize,
    pub failed: usize,
}

async fn measure(name: &str, url: &str, timeout_ms: u32) -> Option<u32> {
    let api = Handle::mihomo();
    tokio::time::timeout(
        Duration::from_millis(u64::from(timeout_ms) + 1_500),
        api.delay_proxy_by_name(name, url, timeout_ms),
    )
    .await
    .ok()?
    .ok()
    .and_then(|result| (result.delay > 0).then_some(result.delay))
}

#[tauri::command]
pub async fn clash5_probe_nodes() -> CmdResult<ProbeBatch> {
    let _guard = PROBE_LOCK
        .try_lock()
        .map_err(|_| super::coded_error("PROBE_BUSY", "a node probe is already running"))?;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| super::coded_error("PROBE_CLOCK", "system clock is unavailable"))?
        .as_millis() as u64;
    refresh_runtime_proxy_providers().await?;
    let records = crate::config::profiles::with_clash5_binding_store(|store| store.node_records(false))
        .await
        .map_err(failure)?;
    let current = Config::profiles().await.latest_arc().current.clone();
    let view = super::proxy::get_proxy_view().await?;
    let mut name_count = HashMap::new();
    let mut core_names = std::collections::HashSet::new();
    for node in view.records.values() {
        *name_count.entry(node.name.clone()).or_insert(0usize) += 1;
        if matches!(node.source, ProxyNodeSource::Core { .. }) {
            core_names.insert(node.name.clone());
        }
    }
    let mut batch = ProbeBatch {
        probed: 0,
        cached: 0,
        unavailable: 0,
        failed: 0,
    };
    let mut targets = Vec::new();
    for record in records {
        if record.quality.as_ref().is_some_and(|quality| quality.is_fresh(now_ms)) {
            batch.cached += 1;
            continue;
        }
        let runtime_name = match &record.source {
            Source::Subscription { provider_id } if current.as_deref() == Some(provider_id.as_str()) => {
                core_names.contains(&record.display_name).then(|| record.display_name.clone())
            }
            Source::Subscription { provider_id } => provider_id
                .strip_prefix(&format!("proxy-provider:{}:", current.as_deref().unwrap_or_default()))
                .and_then(|provider_name| {
                    view.records.values().any(|node| {
                        node.name == record.display_name
                            && matches!(&node.source, ProxyNodeSource::Provider { provider_name: owner, .. } if owner == provider_name)
                    }).then(|| record.display_name.clone())
                }),
            Source::Manual => None,
        };
        if name_count.get(&record.display_name) != Some(&1) {
            batch.unavailable += 1;
            continue;
        }
        let Some(runtime_name) = runtime_name else {
            batch.unavailable += 1;
            continue;
        };
        targets.push((record, runtime_name));
    }
    let results: Vec<_> = stream::iter(targets.into_iter().map(|(record, runtime_name)| async move {
        let short = measure(&runtime_name, SHORT_PROBE_URL, SHORT_PROBE_MS).await;
        let handshake = measure(&runtime_name, HANDSHAKE_PROBE_URL, HANDSHAKE_PROBE_MS).await;
        let quality = NodeQuality::from_probe(short, handshake, now_ms);
        (record.source, record.stable_id, quality)
    }))
    .buffer_unordered(PROBE_CONCURRENCY)
    .collect()
    .await;
    batch.probed = results.len();
    batch.failed = results
        .iter()
        .filter(|(_, _, quality)| quality.status == QualityStatus::Failed)
        .count();
    crate::config::profiles::with_clash5_binding_store(move |store| {
        for (source, stable_id, quality) in &results {
            store.save_quality(source, stable_id, quality)?;
        }
        Ok(())
    })
    .await
    .map_err(failure)?;
    Ok(batch)
}

#[tauri::command]
pub async fn clash5_purge_orphans(source: Option<Source>) -> CmdResult<usize> {
    crate::config::profiles::with_clash5_binding_store(move |store| store.purge_orphans(source.as_ref()))
        .await
        .map_err(failure)
}
