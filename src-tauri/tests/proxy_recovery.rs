#![allow(clippy::unwrap_used, clippy::panic)]

#[path = "../src/core/proxy_lease.rs"]
mod proxy_lease;
#[path = "../src/core/restart_budget.rs"]
mod restart_budget;

use proxy_lease::{ForeignProxyPolicy, LeaseStore, Ownership, ProxyTarget, recover_previous, windows_targets};
use restart_budget::{RESTART_WINDOW, RecoveryRequest, RestartBudget};
use std::{
    cell::Cell,
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

struct Home(PathBuf);
impl Home {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("clash5-proxy-recovery-{}", nanoid::nanoid!()));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn target(port: u16) -> ProxyTarget {
    ProxyTarget::global(&format!("127.0.0.1:{port}")).unwrap()
}

#[test]
fn write_ahead_marker_survives_endpoint_switch_until_settled() {
    let home = Home::new();
    let store = LeaseStore::new(&home.0);
    store.begin("previous-process", &target(7890)).unwrap();
    store.begin("previous-process", &target(7891)).unwrap();
    assert_eq!(
        store.read().unwrap().unwrap().ownership(&[target(7890)]),
        Ownership::Ours
    );
    assert_eq!(
        store.read().unwrap().unwrap().ownership(&[target(7891)]),
        Ownership::Ours
    );
    assert!(store.begin("replacement-process", &target(7892)).is_err());
    store.settle("previous-process", target(7891)).unwrap();
    assert_eq!(
        store.read().unwrap().unwrap().ownership(&[target(7890)]),
        Ownership::Other
    );
    assert_eq!(
        store.read().unwrap().unwrap().ownership(&[target(7891)]),
        Ownership::Ours
    );
}

#[tokio::test]
async fn abnormal_exit_clears_own_proxy_and_retires_marker_only_after_success() {
    let home = Home::new();
    let store = LeaseStore::new(&home.0);
    store.begin("previous-process", &target(7890)).unwrap();
    let cleared = Cell::new(false);
    recover_previous(
        &store,
        Some("new-process"),
        ForeignProxyPolicy::PreserveMarker,
        || async { Ok(vec![target(7890)]) },
        || async {
            assert!(store.read()?.is_some());
            cleared.set(true);
            Ok(())
        },
    )
    .await
    .unwrap();
    assert!(cleared.get());
    assert!(store.read().unwrap().is_none());
}

#[tokio::test]
async fn failed_cleanup_or_failed_os_read_keeps_marker() {
    let home = Home::new();
    let store = LeaseStore::new(&home.0);
    store.begin("previous-process", &target(7890)).unwrap();
    let failed = recover_previous(
        &store,
        Some("new-process"),
        ForeignProxyPolicy::PreserveMarker,
        || async { Ok(vec![target(7890)]) },
        || async { anyhow::bail!("injected cleanup failure") },
    )
    .await;
    assert!(failed.is_err());
    assert!(store.read().unwrap().is_some());
    let unreadable = recover_previous(
        &store,
        None,
        ForeignProxyPolicy::PreserveMarker,
        || async { anyhow::bail!("injected read failure") },
        || async { panic!("Unreadable state must not be cleared") },
    )
    .await;
    assert!(unreadable.is_err());
    assert!(store.read().unwrap().is_some());
}

#[tokio::test]
async fn mixed_protocol_proxy_does_not_clear_another_application() {
    let home = Home::new();
    let store = LeaseStore::new(&home.0);
    store.begin("previous-process", &target(7890)).unwrap();
    let actual = windows_targets(true, "http=127.0.0.1:7890;https=127.0.0.1:9999", "").unwrap();
    let result = recover_previous(
        &store,
        None,
        ForeignProxyPolicy::PreserveMarker,
        || async { Ok(actual) },
        || async { panic!("Foreign protocol must not be cleared") },
    )
    .await;
    assert!(result.is_err());
    assert!(store.read().unwrap().is_some());
}

#[tokio::test]
async fn explicit_disable_retires_foreign_marker_without_os_write() {
    let home = Home::new();
    let store = LeaseStore::new(&home.0);
    for actual in [vec![target(9999)], Vec::new()] {
        store.begin("previous-process", &target(7890)).unwrap();
        recover_previous(
            &store,
            Some("new-process"),
            ForeignProxyPolicy::RetireMarker,
            || async { Ok(actual) },
            || async { panic!("Unowned state must not be cleared") },
        )
        .await
        .unwrap();
        assert!(store.read().unwrap().is_none());
    }
}

#[tokio::test]
async fn foreign_proxy_blocks_automatic_recovery_until_direct_or_explicitly_disabled() {
    let home = Home::new();
    let store = LeaseStore::new(&home.0);
    store.begin("previous-process", &target(7890)).unwrap();
    let result = recover_previous(
        &store,
        Some("new-process"),
        ForeignProxyPolicy::PreserveMarker,
        || async { Ok(vec![target(9999)]) },
        || async { panic!("Foreign proxy must remain unchanged") },
    )
    .await;
    assert!(result.is_err());
    assert!(store.read().unwrap().is_some());
    recover_previous(
        &store,
        Some("new-process"),
        ForeignProxyPolicy::PreserveMarker,
        || async { Ok(Vec::new()) },
        || async { panic!("Already direct") },
    )
    .await
    .unwrap();
    assert!(store.read().unwrap().is_none());
}

#[tokio::test]
async fn normal_current_process_is_not_treated_as_a_crash() {
    let home = Home::new();
    let store = LeaseStore::new(&home.0);
    store.begin("current-process", &target(7890)).unwrap();
    recover_previous(
        &store,
        Some("current-process"),
        ForeignProxyPolicy::PreserveMarker,
        || async { panic!("No startup recovery read required") },
        || async { panic!("Active lease must not be cleared") },
    )
    .await
    .unwrap();
    assert!(store.read().unwrap().is_some());
}

#[tokio::test]
async fn pac_ownership_and_empty_marker_cleanup_are_supported() {
    let home = Home::new();
    let store = LeaseStore::new(&home.0);
    recover_previous(
        &store,
        None,
        ForeignProxyPolicy::PreserveMarker,
        || async { panic!("No marker") },
        || async { panic!("No marker") },
    )
    .await
    .unwrap();
    let pac = ProxyTarget::Pac {
        url: "http://127.0.0.1:33331/commands/pac".into(),
    };
    store.begin("previous-process", &pac).unwrap();
    recover_previous(
        &store,
        None,
        ForeignProxyPolicy::PreserveMarker,
        || async { windows_targets(false, "", "http://127.0.0.1:33331/commands/pac") },
        || async { Ok(()) },
    )
    .await
    .unwrap();
    assert!(store.read().unwrap().is_none());
}

#[test]
fn corrupt_marker_and_unreadable_enabled_endpoint_are_rejected() {
    let home = Home::new();
    fs::write(home.0.join(".proxy-lease.json"), b"corrupt").unwrap();
    assert!(LeaseStore::new(&home.0).read().is_err());
    assert!(windows_targets(true, "", "").is_err());
    assert!(windows_targets(true, "http=invalid", "").is_err());
}

#[test]
fn persistent_budget_allows_only_three_attempts_in_five_minutes() {
    let mut budget = RestartBudget::default();
    let start = Instant::now();
    for second in 0..3 {
        assert!(budget.try_start(start + Duration::from_secs(second)));
    }
    assert_eq!(budget.attempt_count(), 3);
    assert!(!budget.try_start(start + RESTART_WINDOW - Duration::from_millis(1)));
    assert!(budget.try_start(start + RESTART_WINDOW));
    assert!(!budget.try_start(start + RESTART_WINDOW));
}

#[test]
fn stale_recovery_never_overrides_a_stop_replacement_or_exit() {
    let request = RecoveryRequest {
        ticket: 7,
        readiness: 10,
    };
    assert!(request.is_current(7, 10, true, false));
    assert!(!request.is_current(8, 10, true, false));
    assert!(!request.is_current(7, 12, true, false));
    assert!(!request.is_current(7, 10, false, false));
    assert!(!request.is_current(7, 10, true, true));
}
