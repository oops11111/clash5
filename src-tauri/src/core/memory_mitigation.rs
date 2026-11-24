use std::{
    sync::{
        LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use parking_lot::RwLock;
use serde::Serialize;
use tokio::time::MissedTickBehavior;

use crate::{
    config::PerformancePreset,
    core::{manager::RunningMode, runstate::RUN_STATE},
    process::AsyncHandler,
};

const MIB: u64 = 1024 * 1024;
const SAMPLE_INTERVAL: Duration = Duration::from_secs(15);
const ENTER_SAMPLES: u8 = 3;
const EXIT_SAMPLES: u8 = 8;
const STALE_SAMPLES: u8 = 6;

#[derive(Clone, Copy, Debug)]
struct MemorySample {
    total: u64,
    available: u64,
}

impl MemorySample {
    fn new(total: u64, available: u64) -> Option<Self> {
        (total > 0 && available <= total).then_some(Self { total, available })
    }

    const fn is_high(self) -> bool {
        self.available <= 768 * MIB || (self.available <= 2048 * MIB && self.available <= self.total / 10)
    }

    const fn is_recovered(self) -> bool {
        self.available >= 1536 * MIB && (self.available >= self.total / 5 || self.available >= 4096 * MIB)
    }

    fn available_percent(self) -> u8 {
        ((u128::from(self.available) * 100 / u128::from(self.total)).min(100)) as u8
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct PressurePolicy {
    active: bool,
    high_samples: u8,
    recovered_samples: u8,
    missing_samples: u8,
    last_sample: Option<MemorySample>,
}

impl PressurePolicy {
    const fn effective_preset(self, configured: PerformancePreset) -> PerformancePreset {
        if self.active {
            PerformancePreset::PowerSaving
        } else {
            configured
        }
    }

    fn observe(&mut self, sample: Option<MemorySample>, core_running: bool) {
        if !core_running {
            *self = Self::default();
            return;
        }
        let Some(sample) = sample else {
            self.last_sample = None;
            self.high_samples = 0;
            self.recovered_samples = 0;
            self.missing_samples = self.missing_samples.saturating_add(1);
            if self.missing_samples >= STALE_SAMPLES {
                self.active = false;
            }
            return;
        };
        self.last_sample = Some(sample);
        self.missing_samples = 0;
        if sample.is_high() {
            self.high_samples = self.high_samples.saturating_add(1);
            self.recovered_samples = 0;
            if self.high_samples >= ENTER_SAMPLES {
                self.active = true;
            }
        } else if sample.is_recovered() {
            self.recovered_samples = self.recovered_samples.saturating_add(1);
            self.high_samples = 0;
            if self.recovered_samples >= EXIT_SAMPLES {
                self.active = false;
            }
        } else {
            self.high_samples = 0;
            self.recovered_samples = 0;
        }
    }

    fn status(self) -> MemoryMitigationStatus {
        MemoryMitigationStatus {
            active: self.active,
            monitoring: self.last_sample.is_some(),
            available_percent: self.last_sample.map(MemorySample::available_percent),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMitigationStatus {
    pub active: bool,
    pub monitoring: bool,
    pub available_percent: Option<u8>,
}

static POLICY: LazyLock<RwLock<PressurePolicy>> = LazyLock::new(|| RwLock::new(PressurePolicy::default()));
static STARTED: AtomicBool = AtomicBool::new(false);

pub fn status() -> MemoryMitigationStatus {
    POLICY.read().status()
}

pub fn effective_preset(configured: PerformancePreset) -> PerformancePreset {
    POLICY.read().effective_preset(configured)
}

pub fn reset() {
    *POLICY.write() = PressurePolicy::default();
}

pub fn start_monitor() {
    if STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    AsyncHandler::spawn(|| async {
        let mut interval = tokio::time::interval(SAMPLE_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if crate::core::handle::Handle::global().is_exiting() {
                reset();
                break;
            }
            let running = !matches!(*RUN_STATE.mode_arc(), RunningMode::NotRunning);
            let sample = if running {
                tokio::task::spawn_blocking(tauri_plugin_clash_verge_sysinfo::memory_capacity)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|(total, available)| MemorySample::new(total, available))
            } else {
                None
            };
            POLICY.write().observe(sample, running);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gib(total: u64, available: u64) -> MemorySample {
        MemorySample {
            total: total * 1024 * MIB,
            available: available * MIB,
        }
    }

    #[test]
    fn pressure_is_sustained_and_recovery_has_hysteresis() {
        let mut policy = PressurePolicy::default();
        for _ in 0..ENTER_SAMPLES - 1 {
            policy.observe(Some(gib(4, 400)), true);
            assert!(!policy.active);
        }
        policy.observe(Some(gib(4, 400)), true);
        assert!(policy.active);
        for _ in 0..EXIT_SAMPLES - 1 {
            policy.observe(Some(gib(4, 2048)), true);
            assert!(policy.active);
        }
        policy.observe(Some(gib(4, 2048)), true);
        assert!(!policy.active);
    }

    #[test]
    fn short_recovery_and_neutral_samples_do_not_oscillate() {
        let mut policy = PressurePolicy::default();
        for _ in 0..ENTER_SAMPLES {
            policy.observe(Some(gib(16, 1500)), true);
        }
        assert!(policy.active);
        policy.observe(Some(gib(16, 4000)), true);
        policy.observe(Some(gib(16, 2500)), true);
        assert!(policy.active);
        assert_eq!(policy.recovered_samples, 0);
    }

    #[test]
    fn unavailable_telemetry_and_core_stop_clear_runtime_state() {
        let mut policy = PressurePolicy::default();
        for _ in 0..ENTER_SAMPLES {
            policy.observe(Some(gib(4, 400)), true);
        }
        for _ in 0..STALE_SAMPLES - 1 {
            policy.observe(None, true);
            assert!(policy.active);
            assert!(!policy.status().monitoring);
        }
        policy.observe(None, true);
        assert!(!policy.active);
        policy.observe(Some(gib(4, 400)), true);
        policy.observe(None, false);
        assert!(!policy.active);
        assert!(!policy.status().monitoring);
    }

    #[test]
    fn invalid_samples_are_not_used_as_memory_capacity() {
        assert!(MemorySample::new(0, 0).is_none());
        assert!(MemorySample::new(1024, 2048).is_none());
        assert!(!gib(64, 8192).is_high());
    }

    #[test]
    fn configured_preset_survives_pressure_recovery_and_restart() {
        let mut policy = PressurePolicy::default();
        for _ in 0..ENTER_SAMPLES {
            policy.observe(Some(gib(8, 400)), true);
        }
        assert_eq!(
            policy.effective_preset(PerformancePreset::Balanced),
            PerformancePreset::PowerSaving
        );
        assert_eq!(
            policy.effective_preset(PerformancePreset::Performance),
            PerformancePreset::PowerSaving
        );
        for _ in 0..EXIT_SAMPLES {
            policy.observe(Some(gib(8, 4096)), true);
        }
        assert_eq!(
            policy.effective_preset(PerformancePreset::Performance),
            PerformancePreset::Performance
        );
        for _ in 0..ENTER_SAMPLES {
            policy.observe(Some(gib(8, 400)), true);
        }
        policy.observe(None, false);
        assert_eq!(
            policy.effective_preset(PerformancePreset::Balanced),
            PerformancePreset::Balanced
        );
        policy.observe(Some(gib(8, 400)), true);
        assert_eq!(
            policy.effective_preset(PerformancePreset::Balanced),
            PerformancePreset::Balanced
        );
    }
}
