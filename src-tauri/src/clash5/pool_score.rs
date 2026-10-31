//! Scoring for node pool members. Lower scores are better.
//!
//! M1 scores from latency and recent failures, the only node-level signals stored today.
//! Throughput and exit drift are chain-level, so they join the score once node samples carry them.

use clash5_data::{NodeQuality, QualityStatus};

/// Share of the score taken by latency; the rest comes from failures.
const DELAY_WEIGHT: f64 = 0.8;
const FAILURE_WEIGHT: f64 = 0.2;
/// A streak at this length counts as the worst recent failure record.
const FAILURE_STREAK_CAP: f64 = 3.0;

/// One score per member, aligned with the input. `None` when the member has no usable score:
/// it has never been probed, or its last probe failed.
pub fn score(members: &[Option<&NodeQuality>]) -> Vec<Option<f64>> {
    let usable: Vec<(usize, &NodeQuality)> = members
        .iter()
        .enumerate()
        .filter_map(|(index, quality)| {
            quality
                .filter(|quality| quality.status != QualityStatus::Failed)
                .map(|quality| (index, quality))
        })
        .collect();
    let max_delay = usable
        .iter()
        .filter_map(|(_, quality)| quality.short_delay_ms)
        .max()
        .map_or(0.0, f64::from);

    let mut scores = vec![None; members.len()];
    for (index, quality) in usable {
        // Measured delays stay below 1; a member without a delay sample is the worst at 1.
        let delay_term = quality
            .short_delay_ms
            .map_or(1.0, |delay| f64::from(delay) / (max_delay + 1.0));
        let failure_term = f64::from(quality.failure_streak).min(FAILURE_STREAK_CAP) / FAILURE_STREAK_CAP;
        scores[index] = Some(DELAY_WEIGHT * delay_term + FAILURE_WEIGHT * failure_term);
    }
    scores
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn quality(status: QualityStatus, delay: Option<u32>, streak: u8) -> NodeQuality {
        NodeQuality {
            status,
            short_delay_ms: delay,
            handshake_delay_ms: None,
            checked_at_ms: 0,
            failure_streak: streak,
        }
    }

    #[test]
    fn lower_latency_scores_better_and_failures_are_unscored() {
        let fast = quality(QualityStatus::Healthy, Some(40), 0);
        let slow = quality(QualityStatus::Healthy, Some(120), 0);
        let failed = quality(QualityStatus::Failed, Some(10), 0);
        let scores = score(&[Some(&slow), Some(&fast), Some(&failed), None]);
        assert!(scores[1].unwrap() < scores[0].unwrap());
        assert_eq!(scores[2], None);
        assert_eq!(scores[3], None);
    }

    #[test]
    fn recent_failures_and_missing_delay_count_against_a_member() {
        let steady = quality(QualityStatus::Healthy, Some(80), 0);
        let flaky = quality(QualityStatus::Slow, Some(80), 3);
        let untimed = quality(QualityStatus::Slow, None, 0);
        let scores = score(&[Some(&steady), Some(&flaky), Some(&untimed)]);
        assert!(scores[0].unwrap() < scores[1].unwrap());
        assert!(scores[0].unwrap() < scores[2].unwrap());
    }
}
