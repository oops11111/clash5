use super::{CmdResult, CommandFailure, coded_error};
use crate::{clash5::pool_score, config::profiles::with_clash5_binding_store};
use clash5_data::{NodeQuality, NodeRecord, NodeRef, QualityStatus};
use serde::Serialize;

fn storage_failure() -> CommandFailure {
    coded_error("C5-POOL-002", "node pool storage is unavailable")
}

#[derive(Serialize, specta::Type)]
pub struct PoolMemberScore {
    pub reference: NodeRef,
    pub display_name: String,
    /// Position among scored members, best first; `None` when the member has no usable score.
    pub rank: Option<u32>,
    pub score: Option<f64>,
    pub delay_ms: Option<u32>,
    pub failure_streak: u8,
    pub failed: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn clash5_pool_scores(id: String) -> CmdResult<Vec<PoolMemberScore>> {
    let (pool, records) = with_clash5_binding_store(move |store| {
        let pool = store.node_pools()?.into_iter().find(|pool| pool.id == id);
        Ok((pool, store.node_records(true)?))
    })
    .await
    .map_err(|_| storage_failure())?;
    let pool = pool.ok_or_else(|| coded_error("C5-POOL-001", "node pool does not exist"))?;

    let matched: Vec<Option<NodeRecord>> = pool
        .members
        .iter()
        .map(|member| {
            records
                .iter()
                .find(|record| record.source == member.source && record.stable_id == member.stable_id)
                .cloned()
        })
        .collect();
    let qualities: Vec<Option<&NodeQuality>> = matched
        .iter()
        .map(|record| record.as_ref().and_then(|record| record.quality.as_ref()))
        .collect();
    let scores = pool_score::score(&qualities);

    let mut ranked: Vec<usize> = (0..scores.len()).filter(|&index| scores[index].is_some()).collect();
    let key = |index: usize| scores[index].unwrap_or(f64::INFINITY);
    ranked.sort_by(|&left, &right| key(left).total_cmp(&key(right)).then(left.cmp(&right)));
    let unranked = (0..scores.len()).filter(|&index| scores[index].is_none());
    Ok(ranked
        .into_iter()
        .chain(unranked)
        .enumerate()
        .map(|(position, index)| {
            let quality = qualities[index];
            PoolMemberScore {
                reference: pool.members[index].clone(),
                display_name: matched[index]
                    .as_ref()
                    .map_or_else(|| "—".to_owned(), |record| record.display_name.clone()),
                rank: scores[index]
                    .is_some()
                    .then(|| u32::try_from(position + 1).unwrap_or(u32::MAX)),
                score: scores[index],
                delay_ms: quality.and_then(|quality| quality.short_delay_ms),
                failure_streak: quality.map_or(0, |quality| quality.failure_streak),
                failed: quality.is_some_and(|quality| quality.status == QualityStatus::Failed),
            }
        })
        .collect())
}
