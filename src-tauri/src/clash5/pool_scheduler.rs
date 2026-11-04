//! Keeps quality-mode pools on their best member.
//!
//! Every interval the nodes are probed, each quality pool is scored and its switcher is fed the
//! round. A switch is applied to every enabled chain that uses the pool, and the chosen member is
//! stored as the pool's `pinned` member so a rebuilt config starts from it.

use crate::{
    clash5::{
        chains,
        pool_score::{self, PoolSwitcher, Switch, SwitchReason, SwitchRules},
    },
    config::{Config, profiles::with_clash5_binding_store},
    core::handle::Handle,
    process::AsyncHandler,
};
use clash_verge_logging::{Type, logging};
use clash5_data::{NodePool, NodeQuality, NodeRecord, NodeRef, PoolMode};
use parking_lot::Mutex;
use serde::Serialize;
use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const INTERVAL: Duration = Duration::from_secs(10 * 60);
const LOG_SHOWN: usize = 20;

static SWITCHERS: Mutex<Option<HashMap<String, PoolSwitcher>>> = Mutex::new(None);

#[derive(Clone, Debug, Serialize, specta::Type)]
pub struct SwitchEvent {
    pub pool_id: std::string::String,
    pub pool_name: std::string::String,
    pub member: std::string::String,
    pub reason: std::string::String,
    #[specta(type = specta_typescript::Number)]
    pub at_ms: u64,
}

pub fn spawn() {
    AsyncHandler::spawn(|| async {
        loop {
            tokio::time::sleep(INTERVAL).await;
            run_once().await;
        }
    });
}

/// The newest switches, with pool names filled in.
pub async fn recent_events() -> Vec<SwitchEvent> {
    let loaded = with_clash5_binding_store(|store| Ok((store.pool_switches(LOG_SHOWN)?, store.node_pools()?))).await;
    let Ok((records, pools)) = loaded else {
        return Vec::new();
    };
    records
        .into_iter()
        .map(|record| SwitchEvent {
            pool_name: pools
                .iter()
                .find(|pool| pool.id == record.pool_id)
                .map_or_else(|| "—".to_owned(), |pool| pool.name.clone()),
            pool_id: record.pool_id,
            member: record.member,
            reason: record.reason,
            at_ms: record.at_ms,
        })
        .collect()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
}

fn member_key(member: &NodeRef) -> String {
    serde_json::to_string(member).unwrap_or_default()
}

fn schedulable(pool: &NodePool) -> bool {
    pool.mode == PoolMode::Quality && !pool.user_locked
}

async fn run_once() {
    let Ok(pools) = with_clash5_binding_store(|store| store.node_pools()).await else {
        return;
    };
    let pools: Vec<NodePool> = pools.into_iter().filter(schedulable).collect();
    let members: Vec<NodeRef> = pools.iter().flat_map(|pool| pool.members.clone()).collect();
    if members.is_empty() {
        return;
    }
    // Only the members of quality pools are probed. A probe already running is fine.
    let _ = crate::cmd::clash5_data::probe_nodes(Some(members)).await;

    let Ok(records) = with_clash5_binding_store(|store| store.node_records(false)).await else {
        return;
    };
    for pool in pools {
        let qualities: Vec<Option<NodeQuality>> = pool
            .members
            .iter()
            .map(|member| {
                records
                    .iter()
                    .find(|record| record.source == member.source && record.stable_id == member.stable_id)
                    .and_then(|record| record.quality.clone())
            })
            .collect();
        let refs: Vec<Option<&NodeQuality>> = qualities.iter().map(Option::as_ref).collect();
        let scores = pool_score::score_at(&refs, now_ms());
        let round: Vec<(String, Option<f64>)> = pool
            .members
            .iter()
            .zip(scores)
            .map(|(member, score)| (member_key(member), score))
            .collect();

        let switch = observe(&pool, &round);
        if let Some(switch) = switch {
            apply(&pool, &switch, &records).await;
        }
    }
}

fn observe(pool: &NodePool, round: &[(String, Option<f64>)]) -> Option<Switch> {
    let pinned = pool.pinned.as_ref().map(member_key);
    // A member pinned by hand since the last round takes over from the remembered one.
    let resync = |switcher: &mut PoolSwitcher| {
        if pinned.is_some() && switcher.current != pinned {
            *switcher = PoolSwitcher::new(pinned.clone());
        }
        switcher.observe(round, now_ms(), SwitchRules::default())
    };
    resync(
        SWITCHERS
            .lock()
            .get_or_insert_with(HashMap::new)
            .entry(pool.id.clone())
            .or_insert_with(|| PoolSwitcher::new(pool.pinned.as_ref().map(member_key))),
    )
}

async fn apply(pool: &NodePool, switch: &Switch, records: &[NodeRecord]) {
    let Some(index) = pool.members.iter().position(|member| member_key(member) == switch.to) else {
        return;
    };
    let member = &pool.members[index];
    let profile_id = Config::profiles()
        .await
        .latest_arc()
        .current
        .as_ref()
        .map(ToString::to_string);
    let Ok(definitions) = chains::definitions().await else {
        return;
    };
    let mut selected_any = false;
    for chain in definitions
        .iter()
        .filter(|chain| chain.enabled && chain.profile_id == profile_id)
    {
        for (hop_index, hop) in chain.hops.iter().enumerate() {
            let clash5_data::Hop::Pool { pool_id } = hop else {
                continue;
            };
            if *pool_id != pool.id {
                continue;
            }
            let group = format!("{}-h{hop_index}", chain.group_name());
            let proxy = format!("{group}-m{index}");
            match Handle::mihomo().select_node_for_group(&group, &proxy).await {
                Ok(()) => selected_any = true,
                Err(error) => {
                    logging!(warn, Type::Config, "pool switch to {proxy} was not applied: {error:#}");
                    return;
                }
            }
        }
    }

    // Persist the selected member only after every affected runtime group accepted it. This
    // prevents a failed select from making the next scheduler round start from a false state.
    if !selected_any {
        return;
    }

    let mut updated = pool.clone();
    updated.pinned = Some(member.clone());
    let _ = with_clash5_binding_store(move |store| store.save_node_pool(&updated)).await;

    let reason = match switch.reason {
        SwitchReason::Initial => "initial",
        SwitchReason::Failure => "failure",
        SwitchReason::Score => "score",
    };
    let member_name = records
        .iter()
        .find(|record| record.source == member.source && record.stable_id == member.stable_id)
        .map_or_else(|| "—".to_owned(), |record| record.display_name.clone());
    let pool_id = pool.id.clone();
    let at_ms = now_ms();
    let recorded =
        with_clash5_binding_store(move |store| store.record_pool_switch(&pool_id, &member_name, reason, at_ms)).await;
    if recorded.is_err() {
        logging!(warn, Type::Config, "pool switch was not recorded");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clash5_data::Source;

    fn pool(mode: PoolMode, user_locked: bool) -> NodePool {
        NodePool {
            schema_version: 1,
            id: "00000000-0000-4000-8000-000000000001".into(),
            name: "test".into(),
            members: vec![NodeRef {
                source: Source::Manual,
                stable_id: "v2:test".into(),
            }],
            mode,
            pinned: None,
            user_locked,
        }
    }

    #[test]
    fn quality_scheduler_skips_user_locked_pools() {
        assert!(schedulable(&pool(PoolMode::Quality, false)));
        assert!(!schedulable(&pool(PoolMode::Quality, true)));
        assert!(!schedulable(&pool(PoolMode::Pinned, false)));
        assert!(!schedulable(&pool(PoolMode::Delay, false)));
    }
}
