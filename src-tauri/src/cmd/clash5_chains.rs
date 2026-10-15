use super::{CmdResult, CommandFailure, coded_error};
use crate::{
    clash5::{chains, exit_ip},
    config::{
        Config,
        profiles::{PROFILE_WRITE_LOCK, with_clash5_binding_store},
    },
    core::{CoreManager, handle::Handle},
};
use clash5_data::{NodeRef, ProxyChain, parse_proxy_node};
use serde::Serialize;
use serde_yaml_ng::Value;

fn storage_failure() -> CommandFailure {
    coded_error("C5-CHAIN-006", "chain storage is unavailable")
}

#[derive(Serialize)]
pub struct ChainNodeOption {
    reference: NodeRef,
    display_name: String,
    kind: String,
    server: String,
    port: u16,
}

#[derive(Serialize)]
pub struct ChainWorkspace {
    profile_id: Option<String>,
    chains: Vec<ProxyChain>,
    nodes: Vec<ChainNodeOption>,
    policy_groups: Vec<String>,
}

#[tauri::command]
pub async fn clash5_chain_workspace() -> CmdResult<ChainWorkspace> {
    let profile_id = Config::profiles()
        .await
        .latest_arc()
        .current
        .as_ref()
        .map(ToString::to_string);
    let definitions = chains::definitions().await.map_err(|_| storage_failure())?;
    let runtime = Config::runtime().await.data_arc();
    let config = runtime
        .config
        .as_ref()
        .ok_or_else(|| coded_error("C5-CHAIN-005", "runtime configuration is unavailable"))?;
    let catalog = chains::catalog(config, profile_id.as_deref())
        .await
        .map_err(|_| storage_failure())?;
    let records = with_clash5_binding_store(|store| store.node_records(false))
        .await
        .map_err(|_| storage_failure())?;
    let mut nodes: Vec<ChainNodeOption> = vec![];
    for entry in catalog {
        if nodes.iter().any(|node| node.reference == entry.reference) {
            continue;
        }
        let Ok(node) = parse_proxy_node(&Value::Mapping(entry.proxy)) else {
            continue;
        };
        let display_name = records
            .iter()
            .find(|record| record.source == entry.reference.source && record.stable_id == entry.reference.stable_id)
            .map_or(node.display_name, |record| record.display_name.clone());
        nodes.push(ChainNodeOption {
            reference: entry.reference,
            display_name,
            kind: node.kind,
            server: node.server,
            port: node.port,
        });
    }
    nodes.sort_by(|left, right| left.display_name.cmp(&right.display_name));
    Ok(ChainWorkspace {
        chains: definitions
            .into_iter()
            .filter(|chain| chain.profile_id == profile_id)
            .collect(),
        profile_id,
        nodes,
        policy_groups: config
            .get("proxy-groups")
            .and_then(Value::as_sequence)
            .into_iter()
            .flatten()
            .filter(|group| group.get("type").and_then(Value::as_str) == Some("select"))
            .filter_map(|group| group.get("name").and_then(Value::as_str))
            .filter(|name| !name.starts_with("clash5-chain-"))
            .map(str::to_owned)
            .collect(),
    })
}

async fn apply_saved_chains() -> CmdResult {
    let result = CoreManager::global().update_config_forced().await;
    if result.as_ref().is_ok_and(|outcome| outcome.is_valid()) {
        Handle::refresh_clash();
        return Ok(());
    }
    Err(coded_error(
        "C5-CHAIN-007",
        "chain configuration was rejected; the previous definition will be restored",
    ))
}

async fn restore_definition(id: String, previous: Option<ProxyChain>) -> CmdResult {
    with_clash5_binding_store(move |store| match previous {
        Some(chain) => store.save_proxy_chain(&chain),
        None => store.delete_proxy_chain(&id),
    })
    .await
    .map_err(|_| coded_error("C5-CHAIN-008", "the previous chain definition could not be restored"))?;
    apply_saved_chains()
        .await
        .map_err(|_| coded_error("C5-CHAIN-008", "the previous runtime could not be confirmed"))
}

#[tauri::command]
pub async fn clash5_save_chain(chain: ProxyChain) -> CmdResult {
    let _guard = PROFILE_WRITE_LOCK.lock().await;
    chain
        .validate()
        .map_err(|_| coded_error("C5-CHAIN-001", "chain definition is invalid"))?;
    let profile_id = Config::profiles()
        .await
        .latest_arc()
        .current
        .as_ref()
        .map(ToString::to_string);
    if chain.profile_id != profile_id {
        return Err(coded_error(
            "C5-CHAIN-009",
            "select the chain's profile before editing it",
        ));
    }
    let definitions = chains::definitions().await.map_err(|_| storage_failure())?;
    if let clash5_data::ExitPolicy::Switch { fallback_chain_id } = &chain.exit_policy
        && !definitions
            .iter()
            .any(|fallback| fallback.id == *fallback_chain_id && fallback.enabled && fallback.profile_id == profile_id)
    {
        return Err(coded_error(
            "C5-CHAIN-001",
            "the fallback must be an enabled chain in the same profile",
        ));
    }
    let previous = definitions.into_iter().find(|saved| saved.id == chain.id);
    if previous
        .as_ref()
        .is_some_and(|saved| saved.profile_id != chain.profile_id)
    {
        return Err(coded_error("C5-CHAIN-009", "a chain cannot move between profiles"));
    }
    let saved = chain.clone();
    with_clash5_binding_store(move |store| store.save_proxy_chain(&saved))
        .await
        .map_err(|_| storage_failure())?;
    if let Err(failure) = apply_saved_chains().await {
        restore_definition(chain.id, previous).await?;
        return Err(failure);
    }
    Ok(())
}

#[tauri::command]
pub async fn clash5_delete_chain(id: String) -> CmdResult {
    let _guard = PROFILE_WRITE_LOCK.lock().await;
    let definitions = chains::definitions().await.map_err(|_| storage_failure())?;
    let previous = definitions
        .iter()
        .find(|chain| chain.id == id)
        .cloned()
        .ok_or_else(|| coded_error("C5-CHAIN-002", "chain was not found"))?;
    let profile_id = Config::profiles()
        .await
        .latest_arc()
        .current
        .as_ref()
        .map(ToString::to_string);
    if previous.profile_id != profile_id {
        return Err(coded_error(
            "C5-CHAIN-009",
            "select the chain's profile before deleting it",
        ));
    }
    if definitions.iter().any(|chain| matches!(&chain.exit_policy, clash5_data::ExitPolicy::Switch { fallback_chain_id } if fallback_chain_id == &id)) {
        return Err(coded_error("C5-CHAIN-010", "another chain uses this fallback"));
    }
    let deleted = id.clone();
    with_clash5_binding_store(move |store| store.delete_proxy_chain(&deleted))
        .await
        .map_err(|_| storage_failure())?;
    if let Err(failure) = apply_saved_chains().await {
        restore_definition(id, Some(previous)).await?;
        return Err(failure);
    }
    Ok(())
}

#[derive(Serialize)]
pub struct ChainTestResult {
    delay_ms: u32,
    group_name: String,
}

#[tauri::command]
pub async fn clash5_test_chain(id: String) -> CmdResult<ChainTestResult> {
    let _guard = PROFILE_WRITE_LOCK.lock().await;
    let chain = chains::definitions()
        .await
        .map_err(|_| storage_failure())?
        .into_iter()
        .find(|chain| chain.id == id && chain.enabled)
        .ok_or_else(|| coded_error("C5-CHAIN-002", "an enabled chain was not found"))?;
    let group_name = chain.group_name();
    let profile_id = Config::profiles()
        .await
        .latest_arc()
        .current
        .as_ref()
        .map(ToString::to_string);
    if chain.profile_id != profile_id {
        return Err(coded_error(
            "C5-CHAIN-009",
            "select the chain's profile before testing it",
        ));
    }
    let result = Handle::mihomo()
        .delay_proxy_by_name(&group_name, "https://cp.cloudflare.com/generate_204", 10_000)
        .await
        .map_err(|_| coded_error("C5-CHAIN-011", "chain connectivity test failed"))?;
    if result.delay == 0 {
        return Err(coded_error("C5-CHAIN-011", "chain connectivity test failed"));
    }
    Ok(ChainTestResult {
        delay_ms: result.delay,
        group_name,
    })
}

#[derive(Serialize)]
pub struct ChainActivation {
    selection_persisted: bool,
}

#[tauri::command]
pub async fn clash5_activate_chain(id: String, policy_group: String) -> CmdResult<ChainActivation> {
    let _guard = PROFILE_WRITE_LOCK.lock().await;
    let profile_id = Config::profiles()
        .await
        .latest_arc()
        .current
        .as_ref()
        .map(ToString::to_string);
    let chain = chains::definitions()
        .await
        .map_err(|_| storage_failure())?
        .into_iter()
        .find(|chain| chain.id == id && chain.enabled && chain.profile_id == profile_id)
        .ok_or_else(|| coded_error("C5-CHAIN-002", "an enabled chain in the current profile was not found"))?;
    Handle::mihomo()
        .select_node_for_group(&policy_group, &chain.group_name())
        .await
        .map_err(|_| coded_error("C5-CHAIN-012", "the policy group could not select this chain"))?;
    let selection_persisted = crate::config::profiles::record_selected_node(&policy_group, &chain.group_name())
        .await
        .is_ok();
    Handle::refresh_clash();
    Ok(ChainActivation { selection_persisted })
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ExitIpResult {
    Disabled,
    Healthy {
        observation: exit_ip::ExitObservation,
    },
    Warning {
        code: String,
    },
    Switched {
        fallback_chain_id: String,
        observation: exit_ip::ExitObservation,
        selection_persisted: bool,
    },
}

#[tauri::command]
pub async fn clash5_detect_exit_ip(id: String, service_url: Option<String>) -> CmdResult<ExitIpResult> {
    let Some(service_url) = service_url else {
        return Ok(ExitIpResult::Disabled);
    };
    let url = exit_ip::service_url(&service_url)
        .map_err(|_| coded_error("C5-EXIT-001", "use an HTTPS exit-IP service or a loopback fixture"))?;
    let _guard = PROFILE_WRITE_LOCK.lock().await;
    let profile_id = Config::profiles()
        .await
        .latest_arc()
        .current
        .as_ref()
        .map(ToString::to_string);
    let definitions = chains::definitions().await.map_err(|_| storage_failure())?;
    let chain = definitions
        .iter()
        .find(|chain| chain.id == id && chain.enabled && chain.profile_id == profile_id)
        .ok_or_else(|| coded_error("C5-CHAIN-002", "an enabled chain in the current profile was not found"))?;
    match exit_ip::detect(&chain.group_name(), url.clone()).await {
        Ok(observation) => return Ok(ExitIpResult::Healthy { observation }),
        Err(error) if error.to_string() == "C5-EXIT-004" => {
            return Err(coded_error(
                "C5-EXIT-004",
                "the temporary exit-IP listener could not be removed",
            ));
        }
        Err(_) => {}
    }
    let clash5_data::ExitPolicy::Switch { fallback_chain_id } = &chain.exit_policy else {
        return Ok(ExitIpResult::Warning {
            code: "C5-EXIT-003".into(),
        });
    };
    let Some(fallback) = definitions.iter().find(|candidate| {
        candidate.id == *fallback_chain_id && candidate.enabled && candidate.profile_id == profile_id
    }) else {
        return Ok(ExitIpResult::Warning {
            code: "C5-CHAIN-002".into(),
        });
    };
    let observation = match exit_ip::detect(&fallback.group_name(), url).await {
        Ok(observation) => observation,
        Err(error) if error.to_string() == "C5-EXIT-004" => {
            return Err(coded_error(
                "C5-EXIT-004",
                "the temporary exit-IP listener could not be removed",
            ));
        }
        Err(_) => {
            return Ok(ExitIpResult::Warning {
                code: "C5-EXIT-003".into(),
            });
        }
    };
    Handle::mihomo()
        .select_node_for_group(&chain.group_name(), &fallback.group_name())
        .await
        .map_err(|_| coded_error("C5-CHAIN-012", "the verified fallback could not be selected"))?;
    let selection_persisted =
        crate::config::profiles::record_selected_node(&chain.group_name(), &fallback.group_name())
            .await
            .is_ok();
    Handle::refresh_clash();
    Ok(ExitIpResult::Switched {
        fallback_chain_id: fallback.id.clone(),
        observation,
        selection_persisted,
    })
}
