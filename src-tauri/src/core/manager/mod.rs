mod config;
pub(crate) use config::ConfigUpdateGuard;
mod lifecycle;
mod state;

use crate::{
    config::Config,
    core::{handle::Handle, singbox_binary, singbox_config, singbox_runtime::SingboxRoute},
};
use anyhow::{Context as _, Result};
use arc_swap::{ArcSwap, ArcSwapOption};
use clash_verge_logging::{LogRing, Type, logging};
use std::{
    fmt,
    process::Command,
    sync::{
        Arc, LazyLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tauri_plugin_shell::process::CommandChild;

use crate::core::runstate::{RUN_STATE, RealEnv, RunStateStore};
use crate::singleton;
#[cfg(target_os = "windows")]
use std::os::windows::io::OwnedHandle;

pub(crate) static CLASH_LOGGER: LazyLock<Arc<LogRing>> = LazyLock::new(|| Arc::new(LogRing::new()));

const SINGBOX_IDLE_SHUTDOWN: Duration = Duration::from_secs(120);
const SINGBOX_SUPERVISION_INTERVAL: Duration = Duration::from_secs(30);

tokio::task_local! {
    static PROFILE_SELECTIONS_PENDING_COMMIT: bool;
}

const CORE_READINESS_ACTIVE_BIT: u64 = 1;
const CORE_READINESS_GENERATION_STEP: u64 = 1 << 1;

const fn next_active_core_readiness_state(state: u64) -> u64 {
    (state.wrapping_add(CORE_READINESS_GENERATION_STEP) & !CORE_READINESS_ACTIVE_BIT) | CORE_READINESS_ACTIVE_BIT
}

const fn inactive_core_readiness_state(state: u64) -> u64 {
    state.wrapping_add(CORE_READINESS_GENERATION_STEP) & !CORE_READINESS_ACTIVE_BIT
}

const fn active_core_readiness_generation(state: u64) -> Option<u64> {
    if state & CORE_READINESS_ACTIVE_BIT != 0 {
        Some(state)
    } else {
        None
    }
}

fn claim_core_readiness_generation(state: &AtomicU64, captured_generation: u64) -> bool {
    state
        .compare_exchange(
            captured_generation,
            inactive_core_readiness_state(captured_generation),
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_ok()
}

#[derive(Clone, Copy, Debug, serde::Serialize, PartialEq, Eq, specta::Type)]
pub enum RunningMode {
    Service,
    Sidecar,
    NotRunning,
}

impl fmt::Display for RunningMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Service => write!(f, "Service"),
            Self::Sidecar => write!(f, "Sidecar"),
            Self::NotRunning => write!(f, "NotRunning"),
        }
    }
}

/// Why the Core is not running, kept until a Core starts again.
#[derive(Clone, Debug, serde::Serialize, PartialEq, Eq, specta::Type)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
pub enum CoreFailure {
    StartFailed(String),
    ServiceCoreStopped(String),
}

#[derive(Debug)]
pub struct CoreManager {
    /// The Run State this manager reports transitions to.
    ///
    /// A reference rather than the global directly, so a test can supervise a Core without
    /// racing every other test for one process-wide Running Mode.
    run_state: &'static RunStateStore<RealEnv>,
    state: ArcSwap<State>,
    last_update: ArcSwapOption<Instant>,
    #[cfg(target_os = "windows")]
    job_handle: ArcSwapOption<OwnedHandle>,
    config_update_in_progress: AtomicBool,
    core_readiness_state: AtomicU64,
    singbox_degraded: AtomicBool,
    startup_error: parking_lot::Mutex<Option<CoreFailure>>,
    sidecar_exit: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// The optional sing-box route currently replacing one selected generated landing node.
    singbox_route: tokio::sync::Mutex<Option<SingboxRoute>>,
    // 串行化 start/stop/restart 和 sidecar→service 交接。
    // 锁序固定为 config_update_in_progress → lifecycle_lock。
    pub(crate) lifecycle_lock: tokio::sync::Mutex<()>,
    // sidecar→service 交接 watcher 单实例标志。
    #[cfg(target_os = "windows")]
    handoff_watcher_running: AtomicBool,
}

/// Process-level state owned by `CoreManager`.
///
/// Running Mode deliberately does *not* live here — it belongs to `core::runstate`, which
/// keeps it consistent with Service Health and derives PAC availability from it.
#[derive(Debug, Default)]
struct State {
    child_sidecar: ArcSwapOption<CommandChild>,
}

impl Default for CoreManager {
    fn default() -> Self {
        Self {
            run_state: &RUN_STATE,
            state: ArcSwap::new(Arc::new(State::default())),
            last_update: ArcSwapOption::new(None),
            #[cfg(target_os = "windows")]
            job_handle: ArcSwapOption::new(None),
            config_update_in_progress: AtomicBool::new(false),
            core_readiness_state: AtomicU64::new(0),
            singbox_degraded: AtomicBool::new(false),
            startup_error: parking_lot::Mutex::new(None),
            sidecar_exit: tokio::sync::Mutex::new(None),
            singbox_route: tokio::sync::Mutex::new(None),
            lifecycle_lock: tokio::sync::Mutex::new(()),
            #[cfg(target_os = "windows")]
            handoff_watcher_running: AtomicBool::new(false),
        }
    }
}

impl CoreManager {
    fn new() -> Self {
        Self::default()
    }

    /// A manager with its own Run State, for tests that must not disturb the process-wide one.
    #[cfg(test)]
    fn isolated() -> Self {
        Self {
            run_state: Box::leak(Box::new(RunStateStore::new(RealEnv))),
            ..Self::default()
        }
    }

    pub fn get_running_mode(&self) -> Arc<RunningMode> {
        self.run_state.mode_arc()
    }

    pub(crate) fn record_startup_error(&self, failure: CoreFailure) {
        let mut startup_error = self.startup_error.lock();
        if matches!(*self.get_running_mode(), RunningMode::NotRunning) {
            *startup_error = Some(failure);
        }
    }

    pub(crate) fn get_startup_error(&self) -> Option<CoreFailure> {
        self.startup_error.lock().clone()
    }

    pub fn take_child_sidecar(&self) -> Option<CommandChild> {
        self.state
            .load()
            .child_sidecar
            .swap(None)
            .and_then(|arc| Arc::try_unwrap(arc).ok())
    }

    pub fn get_running_sidecar_pid(&self) -> Option<u32> {
        self.state.load().child_sidecar.load_full().map(|child| child.pid())
    }

    pub fn get_last_update(&self) -> Option<Arc<Instant>> {
        self.last_update.load_full()
    }

    /// The Core is now running in `mode`.
    ///
    /// Run State derives PAC availability and the outward mode mirror from this; callers must
    /// not set those alongside.
    pub fn core_started(&self, mode: RunningMode) {
        let previous = *self.get_running_mode();
        if previous != mode {
            logging!(info, Type::Core, "Core running mode changed: {previous} -> {mode}");
        }
        let mut startup_error = self.startup_error.lock();
        self.run_state.core_started(mode);
        *startup_error = None;
    }

    /// The Core is no longer running, for any reason.
    ///
    /// Core readiness is invalidated here rather than inside Run State because readiness
    /// belongs to the process this manager supervises.
    pub fn core_stopped(&self) {
        let previous = *self.get_running_mode();
        if !matches!(previous, RunningMode::NotRunning) {
            logging!(info, Type::Core, "Core running mode changed: {previous} -> NotRunning");
        }
        self.invalidate_core_readiness();
        self.run_state.core_stopped();
    }

    /// A start attempt is under way and the Core is not serving yet.
    ///
    /// Must be paired with [`Self::core_start_settled`] on every path out, including the ones
    /// where the start never happened.
    pub fn core_starting(&self) {
        self.run_state.core_starting();
    }

    /// The start attempt is over: PAC goes back to following the Running Mode.
    pub fn core_start_settled(&self) {
        self.run_state.core_start_settled();
    }

    /// Selects a sing-box landing route when the generated proxy can be mapped, or restores
    /// Mihomo's generated config when the selection should use Mihomo directly.
    pub async fn set_singbox_landing(&self, landing_name: Option<&str>) -> Result<()> {
        let mihomo = Handle::mihomo();
        let mut active = self.singbox_route.lock().await;
        if active
            .as_ref()
            .is_some_and(|route| Some(route.landing_name()) == landing_name)
        {
            return Ok(());
        }
        if let Some(route) = active.as_mut() {
            let result = route.stop(mihomo).await;
            if route.is_stopped() {
                *active = None;
            }
            result?;
        }
        let Some(landing_name) = landing_name else {
            self.singbox_degraded.store(false, Ordering::Release);
            return Ok(());
        };

        let runtime = Config::runtime().await.latest_arc();
        let config = runtime
            .config
            .as_ref()
            .context("generated Mihomo runtime config is unavailable")?;
        let landing = config
            .get("proxies")
            .and_then(serde_yaml_ng::Value::as_sequence)
            .into_iter()
            .flatten()
            .find(|proxy| proxy.get("name").and_then(serde_yaml_ng::Value::as_str) == Some(landing_name))
            .context("generated chain landing proxy is missing")?;
        if singbox_config::map_landing_outbound(landing, "clash5-exit").is_err() {
            // Unsupported protocols retain the ordinary Mihomo route.
            self.singbox_degraded.store(false, Ordering::Release);
            return Ok(());
        }
        let yaml = serde_yaml_ng::to_string(config).context("failed to serialize the generated Mihomo config")?;
        let command = Command::new(singbox_binary::resolve()?);
        let route = SingboxRoute::start(mihomo, command, yaml, landing_name).await?;
        *active = Some(route);
        self.singbox_degraded.store(false, Ordering::Release);
        drop(active);
        Self::spawn_singbox_supervisor(landing_name.to_owned());
        Ok(())
    }

    pub fn singbox_degraded(&self) -> bool {
        self.singbox_degraded.load(Ordering::Acquire)
    }

    pub async fn singbox_route_status_for(&self, landing_name: &str) -> Result<(bool, bool, String)> {
        let active = self.singbox_route.lock().await;
        let sidecar_running = match active.as_ref() {
            Some(route) if route.landing_name() == landing_name => route.is_running().await?,
            _ => false,
        };
        drop(active);
        let proxy = Handle::mihomo()
            .get_proxy_by_name(landing_name)
            .await
            .context("failed to read the Mihomo landing proxy")?;
        let proxy_type = format!("{:?}", proxy.proxy_type);
        let route_active = sidecar_running && proxy_type == "Socks5";
        let degraded = self.singbox_degraded.load(Ordering::Acquire) || (sidecar_running && !route_active);
        Ok((route_active, degraded, proxy_type))
    }

    fn spawn_singbox_supervisor(landing_name: String) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(SINGBOX_SUPERVISION_INTERVAL).await;
                if !Self::global().supervise_singbox_route_once(&landing_name).await {
                    break;
                }
            }
        });
    }

    async fn singbox_runtime_matches(route: &SingboxRoute) -> bool {
        let runtime = Config::runtime().await.latest_arc();
        let Some(config) = runtime.config.as_ref() else {
            return false;
        };
        serde_yaml_ng::to_string(config)
            .ok()
            .is_some_and(|runtime_yaml| route.belongs_to_runtime(&runtime_yaml))
    }

    async fn rebuild_singbox_route(
        &self,
        mut active: tokio::sync::MutexGuard<'_, Option<SingboxRoute>>,
        landing_name: &str,
    ) -> bool {
        logging!(
            warn,
            Type::Core,
            "Mihomo runtime changed while a sing-box route was active; rebuilding the route"
        );
        let Some(route) = active.as_mut() else {
            return false;
        };
        let cleanup = route.stop_resources().await;
        if !route.is_stopped() {
            if let Err(error) = cleanup {
                logging!(error, Type::Core, "old sing-box route cleanup failed: {error:#}");
            }
            return true;
        }

        *active = None;
        self.singbox_degraded.store(true, Ordering::Release);
        Handle::refresh_clash();
        drop(active);
        if let Err(error) = cleanup {
            logging!(warn, Type::Core, "old sing-box route cleanup failed: {error:#}");
        }
        match self.set_singbox_landing(Some(landing_name)).await {
            Ok(()) => false,
            Err(error) => {
                self.singbox_degraded.store(true, Ordering::Release);
                logging!(error, Type::Core, "sing-box route rebuild failed: {error:#}");
                false
            }
        }
    }

    async fn ensure_singbox_mihomo_route(&self, route: &SingboxRoute) {
        match route.ensure_mihomo_route(Handle::mihomo()).await {
            Ok(true) => {
                logging!(
                    warn,
                    Type::Core,
                    "Mihomo landing route drifted; sing-box relay route was restored"
                );
                self.singbox_degraded.store(false, Ordering::Release);
                Handle::refresh_clash();
            }
            Ok(false) => self.singbox_degraded.store(false, Ordering::Release),
            Err(error) => {
                self.singbox_degraded.store(true, Ordering::Release);
                logging!(
                    warn,
                    Type::Core,
                    "failed to verify or restore Mihomo sing-box route: {error:#}"
                );
            }
        }
    }

    async fn restart_singbox_if_exited(&self, active: &mut Option<SingboxRoute>) -> bool {
        let Some(route) = active.as_mut() else {
            return false;
        };
        match route.restart_if_exited().await {
            Ok(true) => logging!(warn, Type::Core, "sing-box sidecar restarted after exiting"),
            Ok(false) => {}
            Err(error) if error.to_string() == "sing-box restart limit reached" => {
                logging!(
                    error,
                    Type::Core,
                    "sing-box restart limit reached; restoring Mihomo route"
                );
                self.singbox_degraded.store(true, Ordering::Release);
                let result = route.stop(Handle::mihomo()).await;
                if route.is_stopped() {
                    *active = None;
                    Handle::refresh_clash();
                    if let Err(error) = result {
                        logging!(error, Type::Core, "sing-box degraded-route cleanup failed: {error:#}");
                    }
                    return false;
                }
                if let Err(error) = result {
                    logging!(error, Type::Core, "sing-box degraded-route rollback failed: {error:#}");
                }
            }
            Err(error) => logging!(warn, Type::Core, "sing-box sidecar restart attempt failed: {error:#}"),
        }
        true
    }

    async fn supervise_singbox_route_once(&self, landing_name: &str) -> bool {
        let mut active = self.singbox_route.lock().await;
        let Some(route) = active.as_mut() else {
            drop(active);
            return false;
        };
        if route.landing_name() != landing_name {
            drop(active);
            return false;
        }

        if !Self::singbox_runtime_matches(route).await {
            return self.rebuild_singbox_route(active, landing_name).await;
        }

        if !self.restart_singbox_if_exited(&mut active).await {
            return false;
        }

        let Some(route) = active.as_mut() else {
            return false;
        };
        self.ensure_singbox_mihomo_route(route).await;

        let persistent = Config::verge().await.latest_arc().singbox_persistent.unwrap_or(false);
        if !persistent && route.idle_for(SINGBOX_IDLE_SHUTDOWN).await {
            logging!(
                info,
                Type::Core,
                "sing-box relay idle timeout reached; stopping sidecar"
            );
            let result = route.stop(Handle::mihomo()).await;
            if route.is_stopped() {
                *active = None;
                Handle::refresh_clash();
                if let Err(error) = result {
                    logging!(error, Type::Core, "sing-box idle cleanup failed: {error:#}");
                }
                drop(active);
                return false;
            }
            if let Err(error) = result {
                logging!(
                    error,
                    Type::Core,
                    "sing-box idle rollback failed; keeping route resources alive: {error:#}"
                );
            }
        }
        drop(active);
        true
    }

    pub fn set_running_child_sidecar(&self, child: CommandChild) {
        let state = self.state.load();
        state.child_sidecar.store(Some(Arc::new(child)));
    }

    fn mark_core_ready(&self) -> u64 {
        let mut current = self.core_readiness_state.load(Ordering::Acquire);
        loop {
            let next = next_active_core_readiness_state(current);
            match self
                .core_readiness_state
                .compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return next,
                Err(changed) => current = changed,
            }
        }
    }

    pub(crate) fn current_core_readiness_generation(&self) -> Option<u64> {
        active_core_readiness_generation(self.core_readiness_state.load(Ordering::Acquire))
    }

    pub(crate) fn invalidate_core_readiness(&self) {
        let _ = self
            .core_readiness_state
            .try_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                active_core_readiness_generation(current).map(inactive_core_readiness_state)
            });
    }

    fn invalidate_core_readiness_if(&self, captured_generation: u64) -> bool {
        claim_core_readiness_generation(&self.core_readiness_state, captured_generation)
    }

    pub fn set_last_update(&self, time: Instant) {
        self.last_update.store(Some(Arc::new(time)));
    }

    /// Replaces the Windows Job Object handle owned by the core manager
    ///
    /// Passing `None` drops the current handle, which closes the Job Object
    /// and terminates its assigned processes due to `KILL_ON_JOB_CLOSE`.
    ///
    /// This method is currently only used on Windows.
    #[cfg(target_os = "windows")]
    fn set_job_handle(&self, handle: Option<OwnedHandle>) {
        self.job_handle.store(handle.map(Arc::new));
    }

    pub(crate) fn try_start_config_update(&self) -> bool {
        !self.config_update_in_progress.swap(true, Ordering::AcqRel)
    }

    pub(crate) fn finish_config_update(&self) {
        self.config_update_in_progress.store(false, Ordering::Release);
    }

    #[tracing::instrument(skip_all, level = "info", fields(retries = 0))]
    pub async fn init(&self) -> Result<bool> {
        const MAX_PORT_FALLBACK_RETRIES: usize = 3;

        if let Some(reason) = crate::config::Config::startup_core_block_reason() {
            anyhow::bail!("core startup blocked after mixed proxy port fallback failure: {reason}");
        }

        use crate::utils::retry::{RetryError, RetryPolicy, retry};
        retry(
            RetryPolicy::fixed(
                std::num::NonZeroUsize::MIN.saturating_add(MAX_PORT_FALLBACK_RETRIES),
                std::time::Duration::ZERO,
            ),
            |retries| async move {
                match self.start_core().await {
                    Ok(()) => {
                        crate::config::Config::notify_startup_mixed_port_fallback();
                        Ok(!matches!(*self.get_running_mode(), RunningMode::NotRunning))
                    }
                    Err(start_error) if retries < MAX_PORT_FALLBACK_RETRIES => {
                        if !matches!(*self.get_running_mode(), RunningMode::NotRunning) {
                            crate::config::Config::notify_startup_mixed_port_fallback();
                            return Err(RetryError::Stop(start_error));
                        }
                        match crate::config::Config::resolve_startup_mixed_port().await {
                            Ok(true) => {
                                let retries = retries + 1;
                                tracing::Span::current().record("retries", retries);
                                logging!(
                                    warn,
                                    Type::Core,
                                    "Retrying core startup after mixed proxy port fallback ({}/{}): {start_error:#}",
                                    retries,
                                    MAX_PORT_FALLBACK_RETRIES
                                );
                                Err(RetryError::Retry(start_error))
                            }
                            Ok(false) => {
                                crate::config::Config::notify_startup_mixed_port_fallback();
                                Err(RetryError::Stop(start_error))
                            }
                            Err(fallback_error) => {
                                crate::config::Config::block_startup_core(&fallback_error);
                                Err(RetryError::Stop(start_error.context(format!(
                                    "the mixed proxy port fallback did not rescue core startup: {fallback_error:#}"
                                ))))
                            }
                        }
                    }
                    Err(error) => {
                        crate::config::Config::notify_startup_mixed_port_fallback();
                        Err(RetryError::Stop(error))
                    }
                }
            },
        )
        .await
    }
}

singleton!(CoreManager, CORE_MANAGER);

#[cfg(test)]
mod startup_error_tests {
    use super::{CoreFailure, CoreManager, RunningMode};

    #[test]
    fn recovery_invalidates_an_unread_startup_error_even_after_a_later_exit() {
        for mode in [RunningMode::Sidecar, RunningMode::Service] {
            let manager = CoreManager::isolated();
            manager.record_startup_error(CoreFailure::StartFailed("startup failed".into()));
            manager.core_started(mode);
            manager.core_stopped();
            assert_eq!(manager.get_startup_error(), None);
        }
    }

    #[test]
    fn unresolved_startup_error_survives_repeated_reads() {
        let manager = CoreManager::isolated();
        let failure = CoreFailure::StartFailed("startup failed".into());
        manager.record_startup_error(failure.clone());
        for _ in 0..2 {
            assert_eq!(manager.get_startup_error().as_ref(), Some(&failure));
        }
    }
}
