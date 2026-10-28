use crate::clash5::exit_ip::{pinned_client, with_pinned_proxy};
use anyhow::{Result, anyhow};
use reqwest::{Client, Url};
use serde::Serialize;
use std::time::{Duration, Instant};

pub(crate) const DEFAULT_DOWNLOAD_BYTES: u64 = 5_000_000;
pub(crate) const MIN_DOWNLOAD_BYTES: u64 = 1_000_000;
pub(crate) const MAX_DOWNLOAD_BYTES: u64 = 20_000_000;
const DOWNLOAD_URL: &str = "https://speed.cloudflare.com/__down";
const PROBE_URL: &str = "http://cp.cloudflare.com/generate_204";
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(40);
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_COUNT: usize = 10;
const TOTAL_TIMEOUT: Duration = Duration::from_secs(60);
/// A chain whose slowest-decile second is below this rate is reported as `warning`.
const WARNING_P10_BPS: f64 = 1_000_000.0;
/// Connection drops during the run at or above this count make the chain `failed`.
const FAILED_DISCONNECTS: u32 = 3;

#[derive(Clone, Debug, Serialize, specta::Type)]
pub struct QualityReport {
    pub status: String,
    #[specta(type = specta_typescript::Number)]
    pub throughput_median_bps: u64,
    #[specta(type = specta_typescript::Number)]
    pub throughput_p10_bps: u64,
    #[specta(type = specta_typescript::Number)]
    pub first_byte_ms: u64,
    #[specta(type = specta_typescript::Number)]
    pub jitter_ms: u64,
    #[specta(type = specta_typescript::Number)]
    pub rtt_p95_ms: u64,
    pub disconnects: u32,
    #[specta(type = specta_typescript::Number)]
    pub bytes_used: u64,
}

/// Nearest-rank percentile; `None` when there are no values.
pub(crate) fn percentile(values: &[f64], fraction: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = (fraction * sorted.len() as f64).ceil() as usize;
    sorted.get(rank.clamp(1, sorted.len()) - 1).copied()
}

/// Population standard deviation; `None` when there are no values.
pub(crate) fn stddev(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values.iter().map(|value| (value - mean).powi(2)).sum::<f64>() / values.len() as f64;
    Some(variance.sqrt())
}

pub(crate) fn classify(throughput_p10_bps: f64, disconnects: u32, delivered: bool) -> &'static str {
    if !delivered || disconnects >= FAILED_DISCONNECTS {
        "failed"
    } else if throughput_p10_bps < WARNING_P10_BPS {
        "warning"
    } else {
        "healthy"
    }
}

/// Bytes received in each whole second of the download. The last second is partial and dropped
/// when there is more than one, so a short tail does not read as a slowdown.
pub(crate) fn throughput_seconds(bins: &[u64]) -> Vec<f64> {
    let complete = if bins.len() > 1 { &bins[..bins.len() - 1] } else { bins };
    complete.iter().map(|bytes| *bytes as f64).collect()
}

struct Download {
    first_byte_ms: u64,
    bins: Vec<u64>,
    received: u64,
    delivered: bool,
    failed: bool,
}

async fn download(client: &Client, url: Url, max_bytes: u64) -> Download {
    let started = Instant::now();
    let mut result = Download {
        first_byte_ms: 0,
        bins: Vec::new(),
        received: 0,
        delivered: false,
        failed: false,
    };
    let Ok(mut response) = client
        .get(url)
        .send()
        .await
        .and_then(|response| response.error_for_status())
    else {
        result.failed = true;
        return result;
    };
    let transfer = async {
        while result.received < max_bytes {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    let elapsed = started.elapsed();
                    if result.received == 0 {
                        result.first_byte_ms = elapsed.as_millis() as u64;
                    }
                    let second = elapsed.as_secs() as usize;
                    if result.bins.len() <= second {
                        result.bins.resize(second + 1, 0);
                    }
                    result.bins[second] += chunk.len() as u64;
                    result.received += chunk.len() as u64;
                }
                Ok(None) => {
                    result.delivered = result.received > 0;
                    return;
                }
                Err(_) => {
                    result.failed = true;
                    return;
                }
            }
        }
        result.delivered = result.received > 0;
    };
    if tokio::time::timeout(DOWNLOAD_TIMEOUT, transfer).await.is_err() {
        result.failed = true;
    }
    result
}

async fn probe(client: &Client, url: Url) -> Option<u64> {
    let started = Instant::now();
    let response = client.get(url).send().await.ok()?;
    response
        .status()
        .is_success()
        .then(|| started.elapsed().as_millis() as u64)
}

async fn run(port: u16, token: &str, download_bytes: u64) -> Result<QualityReport> {
    let download_client = pinned_client(port, token, DOWNLOAD_TIMEOUT)?;
    let probe_client = pinned_client(port, token, PROBE_TIMEOUT)?;
    let download_url = Url::parse_with_params(DOWNLOAD_URL, &[("bytes", download_bytes.to_string())])?;
    let transfer = download(&download_client, download_url, download_bytes).await;

    let mut disconnects = u32::from(transfer.failed);
    let mut round_trips = Vec::with_capacity(PROBE_COUNT);
    let probe_url = Url::parse(PROBE_URL)?;
    for index in 0..PROBE_COUNT {
        if index > 0 {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        match probe(&probe_client, probe_url.clone()).await {
            Some(ms) => round_trips.push(ms as f64),
            None => disconnects += 1,
        }
    }

    let seconds = throughput_seconds(&transfer.bins);
    let p10 = percentile(&seconds, 0.10).unwrap_or(0.0);
    let median = percentile(&seconds, 0.50).unwrap_or(0.0);
    let round_trip_p95 = percentile(&round_trips, 0.95).unwrap_or(0.0);
    let jitter = stddev(&round_trips).unwrap_or(0.0);
    Ok(QualityReport {
        status: classify(p10, disconnects, transfer.delivered).to_owned(),
        throughput_median_bps: median as u64,
        throughput_p10_bps: p10 as u64,
        first_byte_ms: transfer.first_byte_ms,
        jitter_ms: jitter as u64,
        rtt_p95_ms: round_trip_p95 as u64,
        disconnects,
        bytes_used: transfer.received,
    })
}

/// Runs the full measurement through one chain. The whole run is bounded by `TOTAL_TIMEOUT`.
pub(crate) async fn measure(proxy_name: &str, download_bytes: u64) -> Result<QualityReport> {
    let bytes = download_bytes.clamp(MIN_DOWNLOAD_BYTES, MAX_DOWNLOAD_BYTES);
    tokio::time::timeout(
        TOTAL_TIMEOUT,
        with_pinned_proxy(proxy_name, |port, token| async move { run(port, &token, bytes).await }),
    )
    .await
    .map_err(|_| anyhow!("C5-QUALITY-003"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_use_nearest_rank_and_ignore_input_order() {
        let values = [9.0, 1.0, 5.0, 3.0, 7.0];

        assert_eq!(percentile(&values, 0.5), Some(5.0));
        assert_eq!(percentile(&values, 0.10), Some(1.0));
        assert_eq!(percentile(&values, 0.95), Some(9.0));
        assert_eq!(percentile(&[], 0.5), None);
    }

    #[test]
    fn stddev_is_the_population_deviation() {
        assert_eq!(stddev(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]), Some(2.0));
        assert_eq!(stddev(&[]), None);
    }

    #[test]
    fn classification_follows_disconnects_delivery_and_slow_tail() {
        assert_eq!(classify(5_000_000.0, 0, true), "healthy");
        assert_eq!(classify(500_000.0, 0, true), "warning");
        assert_eq!(classify(5_000_000.0, 2, true), "healthy");
        assert_eq!(classify(5_000_000.0, 3, true), "failed");
        assert_eq!(classify(5_000_000.0, 0, false), "failed");
    }

    #[test]
    fn the_partial_last_second_is_not_counted_as_a_slowdown() {
        assert_eq!(
            throughput_seconds(&[3_000_000, 3_000_000, 400_000]),
            vec![3_000_000.0, 3_000_000.0]
        );
        assert_eq!(throughput_seconds(&[700_000]), vec![700_000.0]);
        assert!(throughput_seconds(&[]).is_empty());
    }
}
