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
    collections::{HashMap, VecDeque},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const INTERVAL: Duration = Duration::from_secs(10 * 60);
const LOG_CAPACITY: usize = 20;

static SWITCHERS: Mutex<Option<HashMap<String, PoolSwitcher>>> = Mutex::new(None);
static EVENTS: Mutex<VecDeque<SwitchEvent>> = Mutex::new(VecDeque::new());

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

pub fn events() -> Vec<SwitchEvent> {
    EVENTS.lock().iter().cloned().collect()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
}

fn member_key(member: &NodeRef) -> String {
    serde_json::to_string(member).unwrap_or_default()
}

async fn run_once() {
    // Probing refreshes the node quality the scores are built from. A probe already running is fine.
    let _ = crate::cmd::clash5_probe_nodes().await;

    let loaded = with_clash5_binding_store(|store| Ok((store.node_pools()?, store.node_records(false)?))).await;
    let Ok((pools, records)) = loaded else {
        return;
    };
    for pool in pools.into_iter().filter(|pool| pool.mode == PoolMode::Quality) {
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
        let scores = pool_score::score(&refs);
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
            if let Err(error) = Handle::mihomo().select_node_for_group(&group, &proxy).await {
                logging!(warn, Type::Config, "pool switch to {proxy} was not applied: {error:#}");
            }
        }
    }

    let mut updated = pool.clone();
    updated.pinned = Some(member.clone());
    let _ = with_clash5_binding_store(move |store| store.save_node_pool(&updated)).await;

    let event = SwitchEvent {
        pool_id: pool.id.clone(),
        pool_name: pool.name.clone(),
        member: records
            .iter()
            .find(|record| record.source == member.source && record.stable_id == member.stable_id)
            .map_or_else(|| "—".to_owned(), |record| record.display_name.clone()),
        reason: match switch.reason {
            SwitchReason::Initial => "initial",
            SwitchReason::Failure => "failure",
            SwitchReason::Score => "score",
        }
        .to_owned(),
        at_ms: now_ms(),
    };
    let mut events = EVENTS.lock();
    if events.len() == LOG_CAPACITY {
        events.pop_front();
    }
    events.push_back(event);
}
