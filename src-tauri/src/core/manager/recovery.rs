use super::{CoreFailure, CoreManager, RunningMode};
use crate::utils::retry::{RetryError, RetryPolicy, retry_with_state};
use crate::{
    core::{
        handle::Handle,
        notify::NoticeStatus,
        proxy_control,
        restart_budget::{MAX_RESTARTS, RecoveryRequest},
    },
    process::AsyncHandler,
};
use clash_verge_logging::{Type, logging};
use scopeguard::defer;
use std::{
    num::NonZeroUsize,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

impl CoreManager {
    pub(super) fn cancel_sidecar_recovery(&self) {
        self.recovery_generation.fetch_add(1, Ordering::AcqRel);
        self.recovery_request.lock().take();
    }

    pub(super) fn request_sidecar_recovery(&self) {
        let ticket = self.recovery_generation.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        *self.recovery_request.lock() = Some(RecoveryRequest {
            ticket,
            readiness: self.core_readiness_state.load(Ordering::Acquire),
        });
        self.recovery_notify.notify_one();
    }

    pub(super) fn spawn_sidecar_recovery_supervisor() {
        let manager = Self::global();
        if manager.recovery_supervisor_running.swap(true, Ordering::AcqRel) {
            return;
        }
        AsyncHandler::spawn(|| async {
            let manager = Self::global();
            loop {
                manager.recovery_notify.notified().await;
                let request = manager.recovery_request.lock().take();
                let Some(request) = request else {
                    continue;
                };
                manager.recover_sidecar(request).await;
            }
        });
    }

    fn recovery_is_current(&self, request: RecoveryRequest) -> bool {
        request.is_current(
            self.recovery_generation.load(Ordering::Acquire),
            self.core_readiness_state.load(Ordering::Acquire),
            matches!(*self.get_running_mode(), RunningMode::NotRunning),
            Handle::global().is_exiting(),
        )
    }

    async fn recover_sidecar(&'static self, request: RecoveryRequest) {
        let policy = RetryPolicy::fixed(
            NonZeroUsize::MIN.saturating_add(MAX_RESTARTS - 1),
            Duration::from_secs(1),
        );
        let result = retry_with_state(policy, request, |request, _| {
            Box::pin(async move {
                if !self.recovery_is_current(*request) {
                    return Ok(());
                }
                if !self.try_start_config_update() {
                    return Err(RetryError::Retry(anyhow::anyhow!(
                        "Configuration update prevented core recovery"
                    )));
                }
                defer! { self.finish_config_update(); }
                let _lifecycle = self.lifecycle_lock.lock().await;
                if !self.recovery_is_current(*request) {
                    return Ok(());
                }
                if !self.recovery_budget.lock().try_start(Instant::now()) {
                    return Err(RetryError::Stop(anyhow::anyhow!(
                        "Core restart limit reached: three attempts in five minutes"
                    )));
                }
                let started = async {
                    self.start_core_inner().await?;
                    anyhow::ensure!(
                        self.current_core_readiness_generation().is_some()
                            && !matches!(*self.get_running_mode(), RunningMode::NotRunning),
                        "Core recovery did not become ready"
                    );
                    if Handle::global().is_exiting()
                        || self.recovery_generation.load(Ordering::Acquire) != request.ticket
                    {
                        return Ok(());
                    }
                    self.apply_proxy_after_start().await
                }
                .await;
                match started {
                    Ok(()) => {
                        Handle::refresh_clash();
                        Ok(())
                    }
                    Err(error) => {
                        // No new attempt may start while an earlier process or proxy remains active.
                        if let Err(cleanup) = self.controlled_stop_core_inner().await {
                            return Err(RetryError::Stop(proxy_control::rollback_failure(error, cleanup)));
                        }
                        request.readiness = self.core_readiness_state.load(Ordering::Acquire);
                        Err(RetryError::Retry(error))
                    }
                }
            })
        })
        .await;
        if let Err(error) = result {
            if self.recovery_generation.load(Ordering::Acquire) != request.ticket {
                return;
            }
            let _lifecycle = self.lifecycle_lock.lock().await;
            if self.recovery_generation.load(Ordering::Acquire) != request.ticket
                || !matches!(*self.get_running_mode(), RunningMode::NotRunning)
            {
                return;
            }
            let cleanup = proxy_control::clear_after_unexpected_exit().await;
            let error = match cleanup {
                Ok(()) => error,
                Err(cleanup) => proxy_control::rollback_failure(error, cleanup),
            };
            let error = error.context("Automatic core recovery stopped");
            logging!(error, Type::Core, "{error:#}");
            self.record_startup_error(CoreFailure::StartFailed(format!("{error:#}")));
            Handle::notice(NoticeStatus::CoreStartError, "");
        }
    }
}
