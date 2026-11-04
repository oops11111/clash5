//! Reconciles TUN with current Core and Service availability.

use crate::core::notify::NoticeStatus;

use std::sync::atomic::{AtomicBool, Ordering};

use clash_verge_logging::{Type, logging};
use scopeguard::defer;

use crate::{
    config::Config,
    core::{handle::Handle, manager::RunningMode, runstate::RUN_STATE},
};

/// Prevents recursive runtime disable writes.
static DISABLING_TUN: AtomicBool = AtomicBool::new(false);

/// Project TUN off for this session when startup confirms the Service is absent.
///
/// The persisted preference remains enabled; configuration generation applies the
/// session capability projection after this observation.
pub async fn reconcile_startup_tun_availability() {
    // Read fresh state after prior config writes complete.
    let _config_write = Config::lock_config_write().await;
    let tun_enabled = Config::verge().await.data_arc().enable_tun_mode.unwrap_or(false);
    if !RUN_STATE.state().startup_tun_should_be_disabled(tun_enabled) {
        return;
    }

    logging!(
        info,
        Type::Setup,
        "Service is not installed; suspending TUN for this session without changing the saved preference"
    );
}

/// Suspend or restore TUN when the current settled Run State changes capability.
pub async fn reconcile_tun_availability() {
    if DISABLING_TUN.load(Ordering::Acquire) {
        return;
    }

    // Re-read state and committed config after prior writes complete.
    let config_write = Config::lock_config_write().await;
    let state = RUN_STATE.state();

    // Startup uses the narrower path above, before a Core is running.
    if matches!(state.mode, RunningMode::NotRunning) {
        return;
    }

    // Legacy writers may still hold an uncommitted draft.
    let tun_enabled = Config::verge().await.data_arc().enable_tun_mode.unwrap_or(false);
    let runtime_tun_enabled = Config::runtime()
        .await
        .latest_arc()
        .config
        .as_ref()
        .and_then(|config| config.get("tun"))
        .and_then(|tun| tun.get("enable"))
        .and_then(serde_yaml_ng::Value::as_bool)
        .unwrap_or(false);
    let should_suspend = state.tun_should_be_disabled(tun_enabled) && runtime_tun_enabled;
    let should_restore =
        state.tun_capable() && tun_enabled && !runtime_tun_enabled && matches!(state.mode, RunningMode::Service);
    if !should_suspend && !should_restore {
        return;
    }

    // The flag coalesces newly spawned tasks while the config lock serializes writers.
    DISABLING_TUN.store(true, Ordering::Release);
    defer! {
        DISABLING_TUN.store(false, Ordering::Release);
    }

    if should_suspend {
        logging!(
            info,
            Type::Core,
            "TUN mode cannot work in the current run state; suspending it for this session"
        );
    } else {
        logging!(
            info,
            Type::Core,
            "Service is ready again; restoring the saved TUN preference for this session"
        );
    }

    // Regenerate and apply the runtime config with the session capability projection. The
    // persisted preference stays enabled, so a later Service-ready observation can restore it.
    drop(config_write);
    match crate::core::manager::CoreManager::global()
        .update_config_checked()
        .await
    {
        Ok(()) if should_suspend => Handle::notice(NoticeStatus::TunModeAutoDisabled, ""),
        Ok(()) => Handle::refresh_clash(),
        Err(error) => {
            logging!(error, Type::Core, "failed to suspend TUN for this session: {error:#}");
            Handle::notice(NoticeStatus::TunModeAutoDisableFailed, "");
        }
    }
}
