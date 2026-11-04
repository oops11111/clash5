//! Scoring for node pool members. Lower scores are better.
//!
//! M1 scores from the recent latency median and failure rate. Throughput and exit drift are
//! chain-level, so they do not participate in node-pool selection.

use clash5_data::{NodeQuality, QualityAttempt, QualityStatus};

const LATENCY_CAP_MS: f64 = 1_000.0;
const RECENT_LATENCY_SAMPLES: usize = 5;

/// One score per member, aligned with the input. `None` when the member has no usable score:
/// it has never been probed, or its last probe failed.
pub fn score(members: &[Option<&NodeQuality>]) -> Vec<Option<f64>> {
    let now_ms = members
        .iter()
        .filter_map(|quality| quality.map(|quality| quality.checked_at_ms))
        .max()
        .unwrap_or_default();
    score_at(members, now_ms)
}

/// Score at a shared clock so all members use the same 30-minute failure window.
pub fn score_at(members: &[Option<&NodeQuality>], now_ms: u64) -> Vec<Option<f64>> {
    members
        .iter()
        .map(|quality| quality.and_then(|quality| score_one(quality, now_ms)))
        .collect()
}

fn score_one(quality: &NodeQuality, now_ms: u64) -> Option<f64> {
    if quality.status == QualityStatus::Failed {
        return None;
    }
    let mut attempts = if quality.recent_attempts.is_empty() {
        vec![QualityAttempt {
            checked_at_ms: quality.checked_at_ms,
            delay_ms: quality.short_delay_ms,
        }]
    } else {
        quality
            .recent_attempts
            .iter()
            .filter(|attempt| {
                now_ms >= attempt.checked_at_ms && now_ms - attempt.checked_at_ms <= NodeQuality::FAILURE_WINDOW_MS
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    if attempts.is_empty() {
        attempts.push(QualityAttempt {
            checked_at_ms: quality.checked_at_ms,
            delay_ms: quality.short_delay_ms,
        });
    }
    let delays = attempts
        .iter()
        .filter_map(|attempt| attempt.delay_ms)
        .collect::<Vec<_>>();
    let mut recent = delays
        .into_iter()
        .rev()
        .take(RECENT_LATENCY_SAMPLES)
        .collect::<Vec<_>>();
    recent.sort_unstable();
    let latency_ms = if recent.is_empty() {
        1_000.0
    } else {
        let middle = recent.len() / 2;
        if recent.len() % 2 == 0 {
            (f64::from(recent[middle - 1]) + f64::from(recent[middle])) / 2.0
        } else {
            f64::from(recent[middle])
        }
    };
    let failures = attempts.iter().filter(|attempt| attempt.delay_ms.is_none()).count();
    let latency_term = (latency_ms / LATENCY_CAP_MS).min(1.0);
    let failure_term = failures as f64 / attempts.len() as f64;
    Some(latency_term + failure_term)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn quality(status: QualityStatus, delay: Option<u32>, streak: u8) -> NodeQuality {
        let mut recent_attempts = (0..streak)
            .map(|_| clash5_data::QualityAttempt {
                checked_at_ms: 0,
                delay_ms: None,
            })
            .collect::<Vec<_>>();
        recent_attempts.push(clash5_data::QualityAttempt {
            checked_at_ms: 0,
            delay_ms: delay,
        });
        NodeQuality {
            status,
            short_delay_ms: delay,
            handshake_delay_ms: None,
            checked_at_ms: 0,
            failure_streak: streak,
            recent_attempts,
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

    #[test]
    fn score_uses_the_recent_five_median_and_caps_latency_at_one_second() {
        let quality = NodeQuality {
            status: QualityStatus::Healthy,
            short_delay_ms: Some(1_500),
            handshake_delay_ms: Some(1_500),
            checked_at_ms: 1_800_000,
            failure_streak: 0,
            recent_attempts: vec![
                clash5_data::QualityAttempt {
                    checked_at_ms: 0,
                    delay_ms: Some(10),
                },
                clash5_data::QualityAttempt {
                    checked_at_ms: 1_700_000,
                    delay_ms: Some(100),
                },
                clash5_data::QualityAttempt {
                    checked_at_ms: 1_710_000,
                    delay_ms: Some(200),
                },
                clash5_data::QualityAttempt {
                    checked_at_ms: 1_720_000,
                    delay_ms: Some(300),
                },
                clash5_data::QualityAttempt {
                    checked_at_ms: 1_730_000,
                    delay_ms: Some(400),
                },
                clash5_data::QualityAttempt {
                    checked_at_ms: 1_740_000,
                    delay_ms: Some(2_000),
                },
            ],
        };
        let score = score_at(&[Some(&quality)], 1_800_000)[0].unwrap();
        assert!((score - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn failures_older_than_thirty_minutes_do_not_penalize_the_score() {
        let quality = NodeQuality {
            status: QualityStatus::Healthy,
            short_delay_ms: Some(100),
            handshake_delay_ms: Some(100),
            checked_at_ms: 1_800_000,
            failure_streak: 0,
            recent_attempts: vec![
                clash5_data::QualityAttempt {
                    checked_at_ms: 0,
                    delay_ms: None,
                },
                clash5_data::QualityAttempt {
                    checked_at_ms: 1_800_000,
                    delay_ms: Some(100),
                },
            ],
        };
        let score = score_at(&[Some(&quality)], 1_800_001)[0].unwrap();
        assert!((score - 0.1).abs() < f64::EPSILON);
    }
}

/// Rules that keep a pool from flapping between members.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwitchRules {
    /// Consecutive rounds a candidate must beat the current member.
    pub rounds: u8,
    /// Share of the current score a candidate must save, e.g. 0.15 for 15%.
    pub margin: f64,
    /// Time after a score-based switch during which no other score-based switch happens.
    pub cooldown_ms: u64,
    /// Consecutive unscored rounds that trigger an immediate switch.
    pub failure_limit: u8,
}

impl Default for SwitchRules {
    fn default() -> Self {
        Self {
            rounds: 3,
            margin: 0.15,
            cooldown_ms: 30 * 60 * 1000,
            failure_limit: 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchReason {
    /// The first member the pool has picked.
    Initial,
    /// The current member failed repeatedly.
    Failure,
    /// A candidate kept beating the current member by the margin.
    Score,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Switch {
    pub to: String,
    pub reason: SwitchReason,
}

/// Tracks one pool's current member across probe rounds.
#[derive(Clone, Debug, Default)]
pub struct PoolSwitcher {
    pub current: Option<String>,
    candidate: Option<(String, u8)>,
    failed_rounds: u8,
    last_score_switch_ms: Option<u64>,
}

impl PoolSwitcher {
    /// A switcher that starts on `current`, typically the member a pool was pinned to.
    pub fn new(current: Option<String>) -> Self {
        Self {
            current,
            ..Self::default()
        }
    }

    /// Feeds one round of scores (lower is better; `None` is unusable) and returns a switch if
    /// the rules call for one.
    pub fn observe(&mut self, scores: &[(String, Option<f64>)], now_ms: u64, rules: SwitchRules) -> Option<Switch> {
        let best = scores
            .iter()
            .filter_map(|(key, score)| score.map(|score| (key, score)))
            .min_by(|left, right| left.1.total_cmp(&right.1).then(left.0.cmp(right.0)))
            .map(|(key, score)| (key.clone(), score));
        let current_score = self.current.as_ref().and_then(|current| {
            scores
                .iter()
                .find(|(key, _)| key == current)
                .and_then(|(_, score)| *score)
        });

        let Some(current) = self.current.clone() else {
            let (to, _) = best?;
            return Some(self.switch(to, SwitchReason::Initial, now_ms));
        };

        if current_score.is_some() {
            self.failed_rounds = 0;
        } else {
            self.failed_rounds = self.failed_rounds.saturating_add(1);
            if self.failed_rounds >= rules.failure_limit
                && let Some((to, _)) = best.filter(|(key, _)| *key != current)
            {
                return Some(self.switch(to, SwitchReason::Failure, now_ms));
            }
            return None;
        }

        let (best_key, best_score) = best?;
        let current_score = current_score?;
        let saves_enough = best_key != current && best_score < current_score * (1.0 - rules.margin);
        let cooling = self
            .last_score_switch_ms
            .is_some_and(|at| now_ms.saturating_sub(at) < rules.cooldown_ms);
        if !saves_enough || cooling {
            self.candidate = None;
            return None;
        }

        let rounds = match &self.candidate {
            Some((key, rounds)) if *key == best_key => rounds.saturating_add(1),
            _ => 1,
        };
        if rounds >= rules.rounds {
            return Some(self.switch(best_key, SwitchReason::Score, now_ms));
        }
        self.candidate = Some((best_key, rounds));
        None
    }

    fn switch(&mut self, to: String, reason: SwitchReason, now_ms: u64) -> Switch {
        self.current = Some(to.clone());
        self.candidate = None;
        self.failed_rounds = 0;
        if reason == SwitchReason::Score {
            self.last_score_switch_ms = Some(now_ms);
        }
        Switch { to, reason }
    }
}

#[cfg(test)]
mod switcher_tests {
    use super::*;

    fn round(a: Option<f64>, b: Option<f64>) -> Vec<(String, Option<f64>)> {
        vec![("a".to_owned(), a), ("b".to_owned(), b)]
    }

    #[test]
    fn the_first_round_picks_the_best_usable_member() {
        let mut switcher = PoolSwitcher::default();
        let switch = switcher.observe(&round(Some(0.5), Some(0.2)), 0, SwitchRules::default());
        assert_eq!(
            switch,
            Some(Switch {
                to: "b".into(),
                reason: SwitchReason::Initial
            })
        );
        assert_eq!(switcher.current.as_deref(), Some("b"));
    }

    #[test]
    fn a_better_member_must_win_several_rounds_by_the_margin() {
        let rules = SwitchRules::default();
        let mut switcher = PoolSwitcher {
            current: Some("a".into()),
            ..Default::default()
        };
        assert_eq!(
            switcher.observe(&round(Some(0.5), Some(0.45)), 0, rules),
            None,
            "10% is not enough"
        );
        assert_eq!(switcher.observe(&round(Some(0.5), Some(0.1)), 1, rules), None);
        assert_eq!(switcher.observe(&round(Some(0.5), Some(0.1)), 2, rules), None);
        let switch = switcher.observe(&round(Some(0.5), Some(0.1)), 3, rules);
        assert_eq!(
            switch,
            Some(Switch {
                to: "b".into(),
                reason: SwitchReason::Score
            })
        );
    }

    #[test]
    fn a_dip_in_the_candidate_restarts_the_count() {
        let rules = SwitchRules::default();
        let mut switcher = PoolSwitcher {
            current: Some("a".into()),
            ..Default::default()
        };
        switcher.observe(&round(Some(0.5), Some(0.1)), 0, rules);
        switcher.observe(&round(Some(0.5), Some(0.5)), 1, rules);
        assert_eq!(switcher.observe(&round(Some(0.5), Some(0.1)), 2, rules), None);
    }

    #[test]
    fn score_switches_wait_out_the_cooldown_but_failures_do_not() {
        let rules = SwitchRules::default();
        let mut switcher = PoolSwitcher {
            current: Some("a".into()),
            ..Default::default()
        };
        for now in 0..3 {
            switcher.observe(&round(Some(0.5), Some(0.1)), now, rules);
        }
        assert_eq!(switcher.current.as_deref(), Some("b"));

        // Now "b" is better than "a" by the margin again, but the cooldown is running.
        for now in 10..20 {
            assert_eq!(switcher.observe(&round(Some(0.1), Some(0.5)), now, rules), None);
        }
        // "b" fails twice in a row: switch at once, even inside the cooldown.
        assert_eq!(switcher.observe(&round(Some(0.1), None), 21, rules), None);
        let switch = switcher.observe(&round(Some(0.1), None), 22, rules);
        assert_eq!(
            switch,
            Some(Switch {
                to: "a".into(),
                reason: SwitchReason::Failure
            })
        );
    }
}
