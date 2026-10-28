use clash5_data::{BindingStore, QualitySample};

fn sample(chain_id: &str, status: &str) -> QualitySample {
    QualitySample {
        chain_id: chain_id.into(),
        measured_at: String::new(),
        status: status.into(),
        throughput_median_bps: 5_000_000,
        throughput_p10_bps: 2_000_000,
        first_byte_ms: 120,
        jitter_ms: 8,
        rtt_p95_ms: 180,
        disconnects: 0,
        bytes_used: 5_000_000,
    }
}

#[test]
fn quality_samples_round_trip_newest_first() {
    let mut store = BindingStore::memory().unwrap();
    store.record_quality_sample(&sample("chain-a", "healthy")).unwrap();
    store.record_quality_sample(&sample("chain-a", "warning")).unwrap();
    store.record_quality_sample(&sample("chain-b", "failed")).unwrap();

    let history = store.quality_samples("chain-a", 10).unwrap();

    assert_eq!(history.len(), 2);
    assert_eq!(history[0].status, "warning");
    assert_eq!(history[1].throughput_p10_bps, 2_000_000);
    assert!(!history[0].measured_at.is_empty());
}

#[test]
fn quality_history_keeps_only_the_most_recent_hundred_per_chain() {
    let mut store = BindingStore::memory().unwrap();
    for _ in 0..105 {
        store.record_quality_sample(&sample("chain-a", "healthy")).unwrap();
    }

    assert_eq!(store.quality_samples("chain-a", 100).unwrap().len(), 100);
}

#[test]
fn quality_samples_reject_unknown_status_and_out_of_range_reads() {
    let mut store = BindingStore::memory().unwrap();

    assert!(store.record_quality_sample(&sample("chain-a", "drift")).is_err());
    assert!(store.quality_samples("chain-a", 0).is_err());
    assert!(store.quality_samples("chain-a", 101).is_err());
}
