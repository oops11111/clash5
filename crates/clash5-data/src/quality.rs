use serde::{Deserialize, Serialize};

/// One long-lived connection quality measurement for a chain.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct QualitySample {
    pub chain_id: String,
    pub measured_at: String,
    pub status: String,
    pub throughput_median_bps: u64,
    pub throughput_p10_bps: u64,
    pub first_byte_ms: u64,
    pub jitter_ms: u64,
    pub rtt_p95_ms: u64,
    pub disconnects: u32,
    pub bytes_used: u64,
}

pub const QUALITY_STATUSES: [&str; 3] = ["healthy", "warning", "failed"];
