#[cfg(windows)]
use super::{chains, exit_ip::ChainListener, process_tree};
use crate::cmd::CmdResult;
#[cfg(not(windows))]
use crate::cmd::coded_error;
#[cfg(windows)]
use crate::{
    cmd::coded_error,
    config::{Config, profiles::PROFILE_WRITE_LOCK},
    core::{owner_identity::current_owner_credentials, service},
};
use serde::Serialize;
#[cfg(windows)]
use std::{
    sync::{
        OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
#[cfg(windows)]
use tokio::sync::{Mutex, oneshot};

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "status", rename_all = "snake_case")]
#[allow(dead_code)]
pub enum ProcessProxyStatus {
    Unavailable,
    Idle,
    Active {
        chain_id: String,
        relay_port: u16,
        target_count: u32,
    },
    RecoveryRequired {
        detail: String,
    },
}

#[cfg(windows)]
struct ActiveProcessProxy {
    chain_id: String,
    relay_port: u16,
    generation: u64,
    shutdown: Option<oneshot::Sender<()>>,
    relay_task: tokio::task::JoinHandle<std::io::Result<()>>,
    listener: ChainListener,
}

#[cfg(windows)]
static ACTIVE: OnceLock<Mutex<Option<ActiveProcessProxy>>> = OnceLock::new();

#[cfg(windows)]
static PENDING_CLEANUPS: AtomicUsize = AtomicUsize::new(0);

#[cfg(windows)]
fn active() -> &'static Mutex<Option<ActiveProcessProxy>> {
    ACTIVE.get_or_init(|| Mutex::new(None))
}

#[cfg(windows)]
fn process_proxy_error(code: &str, detail: impl std::fmt::Display) -> crate::cmd::CommandFailure {
    coded_error(code, detail)
}

#[cfg(windows)]
fn start_cleanup(mut session: ActiveProcessProxy) -> oneshot::Receiver<anyhow::Result<()>> {
    PENDING_CLEANUPS.fetch_add(1, Ordering::SeqCst);
    let (send, receive) = oneshot::channel();
    tokio::spawn(async move {
        if let Some(shutdown) = session.shutdown.take() {
            let _ = shutdown.send(());
        }
        let relay_stopped = match tokio::time::timeout(Duration::from_secs(5), &mut session.relay_task).await {
            Ok(Ok(Ok(()))) => true,
            Err(_) => {
                session.relay_task.abort();
                false
            }
            _ => false,
        };
        let listener_closed = close_listener(session.listener).await.is_ok();
        let result = if relay_stopped && listener_closed {
            Ok(())
        } else {
            Err(anyhow::anyhow!("C5-PROXY-004"))
        };
        let _ = send.send(result);
        if !session.relay_task.is_finished() {
            let _ = session.relay_task.await;
        }
        PENDING_CLEANUPS.fetch_sub(1, Ordering::SeqCst);
    });
    receive
}

#[cfg(windows)]
async fn cleanup(session: ActiveProcessProxy) -> anyhow::Result<()> {
    start_cleanup(session)
        .await
        .map_err(|_| anyhow::anyhow!("C5-PROXY-004"))?
}

#[cfg(windows)]
trait CleanupListener: Send + 'static {
    fn try_close(&mut self) -> impl std::future::Future<Output = bool> + Send;
}

#[cfg(windows)]
impl CleanupListener for ChainListener {
    async fn try_close(&mut self) -> bool {
        self.close().await.is_ok()
    }
}

#[cfg(windows)]
async fn close_listener(mut listener: impl CleanupListener) -> anyhow::Result<()> {
    PENDING_CLEANUPS.fetch_add(1, Ordering::SeqCst);
    let (send, receive) = oneshot::channel();
    tokio::spawn(async move {
        let mut closed = listener.try_close().await;
        let _ = send.send(if closed {
            Ok(())
        } else {
            Err(anyhow::anyhow!("C5-PROXY-004"))
        });
        while !closed {
            tokio::time::sleep(Duration::from_millis(250)).await;
            closed = listener.try_close().await;
        }
        PENDING_CLEANUPS.fetch_sub(1, Ordering::SeqCst);
    });
    receive.await.map_err(|_| anyhow::anyhow!("C5-PROXY-004"))?
}

#[cfg(windows)]
fn status_from_service(status: clash_verge_service_ipc::ProcessProxySessionStatus) -> ProcessProxyStatus {
    match status {
        clash_verge_service_ipc::ProcessProxySessionStatus::Idle => ProcessProxyStatus::Idle,
        clash_verge_service_ipc::ProcessProxySessionStatus::RecoveryRequired { detail } => {
            ProcessProxyStatus::RecoveryRequired { detail }
        }
        clash_verge_service_ipc::ProcessProxySessionStatus::Active { session } => ProcessProxyStatus::Active {
            chain_id: "service-recovered".into(),
            relay_port: session.relay_port,
            target_count: session.targets.len() as u32,
        },
    }
}

#[cfg(windows)]
fn service_session_matches(
    session: &clash_verge_service_ipc::ProcessProxySession,
    local_generation: u64,
    local_relay_port: u16,
    proof_generation: u64,
) -> bool {
    session.session_id == proof_generation
        && session.session_id == local_generation
        && session.relay_pid == std::process::id()
        && session.relay_port == local_relay_port
}

#[cfg(windows)]
async fn service_status(local: Option<&ActiveProcessProxy>) -> anyhow::Result<ProcessProxyStatus> {
    let proof = service::active_service_session()?;
    let credentials = current_owner_credentials()?;
    let response = clash_verge_service_ipc::get_process_proxy_status(&credentials, &proof).await?;
    if response.code != 0 {
        return Err(anyhow::anyhow!("C5-PROXY-005: {}", response.message));
    }
    let status = response.data.ok_or_else(|| anyhow::anyhow!("C5-PROXY-005"))?;
    Ok(match status {
        clash_verge_service_ipc::ProcessProxySessionStatus::Active { session } => match local {
            Some(local) if service_session_matches(&session, local.generation, local.relay_port, proof.generation) => {
                ProcessProxyStatus::Active {
                    chain_id: local.chain_id.clone(),
                    relay_port: session.relay_port,
                    target_count: session.targets.len() as u32,
                }
            }
            _ => ProcessProxyStatus::RecoveryRequired {
                detail: "the service session does not match the local redirect relay".into(),
            },
        },
        clash_verge_service_ipc::ProcessProxySessionStatus::Idle if local.is_some() => {
            ProcessProxyStatus::RecoveryRequired {
                detail: "the service no longer has the local redirect session".into(),
            }
        }
        other => status_from_service(other),
    })
}

#[cfg(windows)]
pub(crate) async fn status() -> CmdResult<ProcessProxyStatus> {
    let session = active().lock().await;
    if PENDING_CLEANUPS.load(Ordering::SeqCst) != 0 {
        return Ok(ProcessProxyStatus::RecoveryRequired {
            detail: "local redirect cleanup has not completed".into(),
        });
    }
    if let Some(session) = session.as_ref()
        && session.relay_task.is_finished()
    {
        return Ok(ProcessProxyStatus::RecoveryRequired {
            detail: "the local redirect relay stopped unexpectedly".into(),
        });
    }
    let result = service_status(session.as_ref())
        .await
        .map_err(|error| process_proxy_error("C5-PROXY-005", error));
    drop(session);
    if PENDING_CLEANUPS.load(Ordering::SeqCst) != 0 {
        return Ok(ProcessProxyStatus::RecoveryRequired {
            detail: "local redirect cleanup has not completed".into(),
        });
    }
    result
}

#[cfg(not(windows))]
#[allow(
    clippy::unused_async,
    reason = "command handlers keep the cross-platform async interface"
)]
pub(crate) async fn status() -> CmdResult<ProcessProxyStatus> {
    Ok(ProcessProxyStatus::Unavailable)
}

#[cfg(windows)]
pub(crate) async fn attach(chain_id: String, pid: u32) -> CmdResult<ProcessProxyStatus> {
    let _guard = PROFILE_WRITE_LOCK.lock().await;
    if PENDING_CLEANUPS.load(Ordering::SeqCst) != 0 {
        return Err(process_proxy_error(
            "C5-PROXY-004",
            "local redirect cleanup has not completed",
        ));
    }
    if active().lock().await.is_some() {
        return Err(process_proxy_error(
            "C5-PROXY-003",
            "a process proxy session is already active",
        ));
    }
    if Config::verge().await.latest_arc().enable_tun_mode.unwrap_or(false) {
        return Err(process_proxy_error(
            "C5-PROXY-006",
            "process proxy cannot be enabled while TUN mode is active",
        ));
    }
    let profile_id = Config::profiles()
        .await
        .latest_arc()
        .current
        .as_ref()
        .map(ToString::to_string);
    let chain = chains::definitions()
        .await
        .map_err(|_| process_proxy_error("C5-PROXY-007", "chain storage is unavailable"))?
        .into_iter()
        .find(|chain| chain.id == chain_id && chain.enabled && chain.profile_id == profile_id)
        .ok_or_else(|| {
            process_proxy_error(
                "C5-PROXY-007",
                "the selected chain is not enabled in the current profile",
            )
        })?;
    let target = process_tree::selected_process_target(pid)
        .map_err(|_| process_proxy_error("C5-PROXY-002", "the selected process is no longer available"))?;
    let proof = service::active_service_session()
        .map_err(|_| process_proxy_error("C5-PROXY-005", "the privileged service session is not active"))?;
    let credentials = current_owner_credentials()
        .map_err(|_| process_proxy_error("C5-PROXY-005", "owner credentials are unavailable"))?;
    let mut listener = ChainListener::new(&chain.group_name(), "clash5-process-proxy")
        .map_err(|_| process_proxy_error("C5-PROXY-008", "the chain listener could not be reserved"))?;
    if listener.activate().await.is_err() {
        let _ = close_listener(listener).await;
        return Err(process_proxy_error(
            "C5-PROXY-008",
            "the chain listener could not be activated",
        ));
    }
    let relay = match clash_verge_service_ipc::TcpRedirectRelay::bind(
        proof.generation,
        listener.port(),
        listener.token(),
    )
    .await
    {
        Ok(relay) => relay,
        Err(error) => {
            let _ = close_listener(listener).await;
            return Err(process_proxy_error("C5-PROXY-008", error));
        }
    };
    let relay_port = match relay.port() {
        Ok(port) => port,
        Err(error) => {
            let _ = close_listener(listener).await;
            return Err(process_proxy_error("C5-PROXY-008", error));
        }
    };
    let (shutdown, receiver) = oneshot::channel();
    let relay_task = tokio::spawn(relay.run(receiver));
    let request = clash_verge_service_ipc::ProcessProxyAttachRequest {
        relay_pid: std::process::id(),
        relay_port,
        targets: vec![target.clone()],
    };
    let response = match clash_verge_service_ipc::attach_process_proxy(&credentials, &proof, &request).await {
        Ok(response) => response,
        Err(error) => {
            let _ = cleanup(ActiveProcessProxy {
                chain_id,
                relay_port,
                generation: proof.generation,
                shutdown: Some(shutdown),
                relay_task,
                listener,
            })
            .await;
            return Err(process_proxy_error("C5-PROXY-005", error));
        }
    };
    if response.code != 0 {
        let _ = cleanup(ActiveProcessProxy {
            chain_id,
            relay_port,
            generation: proof.generation,
            shutdown: Some(shutdown),
            relay_task,
            listener,
        })
        .await;
        return Err(process_proxy_error("C5-PROXY-005", response.message));
    }
    let status = match response.data.map(status_from_service) {
        Some(status) => status,
        None => {
            let _ = cleanup(ActiveProcessProxy {
                chain_id,
                relay_port,
                generation: proof.generation,
                shutdown: Some(shutdown),
                relay_task,
                listener,
            })
            .await;
            return Err(process_proxy_error(
                "C5-PROXY-005",
                "the service omitted process proxy status",
            ));
        }
    };
    *active().lock().await = Some(ActiveProcessProxy {
        chain_id,
        relay_port,
        generation: proof.generation,
        shutdown: Some(shutdown),
        relay_task,
        listener,
    });
    Ok(status)
}

#[cfg(not(windows))]
#[allow(
    clippy::unused_async,
    reason = "command handlers keep the cross-platform async interface"
)]
pub(crate) async fn attach(_chain_id: String, _pid: u32) -> CmdResult<ProcessProxyStatus> {
    Err(coded_error(
        "C5-PROXY-001",
        "process proxy is only available on Windows",
    ))
}

#[cfg(windows)]
pub(crate) async fn detach() -> CmdResult<ProcessProxyStatus> {
    let _guard = PROFILE_WRITE_LOCK.lock().await;
    if PENDING_CLEANUPS.load(Ordering::SeqCst) != 0 {
        return Err(process_proxy_error(
            "C5-PROXY-004",
            "local redirect cleanup has not completed",
        ));
    }
    let proof = service::active_service_session()
        .map_err(|_| process_proxy_error("C5-PROXY-005", "the privileged service session is not active"))?;
    let credentials = current_owner_credentials()
        .map_err(|_| process_proxy_error("C5-PROXY-005", "owner credentials are unavailable"))?;
    let mut active_session = active().lock().await;
    let response = match clash_verge_service_ipc::detach_process_proxy(&credentials, &proof).await {
        Ok(response) => response,
        Err(error) => return Err(process_proxy_error("C5-PROXY-005", error)),
    };
    if response.code != 0 {
        return Err(process_proxy_error("C5-PROXY-005", response.message));
    }
    let cleanup = active_session.take().map(start_cleanup);
    drop(active_session);
    if let Some(cleanup) = cleanup {
        cleanup
            .await
            .map_err(|_| process_proxy_error("C5-PROXY-004", "cleanup task stopped"))?
            .map_err(|error| process_proxy_error("C5-PROXY-004", error))?;
    }
    Ok(ProcessProxyStatus::Idle)
}

#[cfg(all(test, windows))]
mod tests {
    use super::{CleanupListener, PENDING_CLEANUPS, close_listener, service_session_matches};
    use clash_verge_service_ipc::ProcessProxySession;
    use std::{
        sync::{Arc, atomic::Ordering},
        time::Duration,
    };
    use tokio::sync::Notify;

    struct RetryListener {
        attempts: usize,
        started: Arc<Notify>,
        proceed: Arc<Notify>,
        closed: Arc<Notify>,
    }

    impl CleanupListener for RetryListener {
        async fn try_close(&mut self) -> bool {
            self.attempts += 1;
            if self.attempts == 1 {
                self.started.notify_one();
                self.proceed.notified().await;
                false
            } else {
                self.closed.notify_one();
                true
            }
        }
    }

    #[tokio::test]
    async fn listener_cleanup_survives_cancellation_and_retries_failure() -> anyhow::Result<()> {
        let started = Arc::new(Notify::new());
        let proceed = Arc::new(Notify::new());
        let closed = Arc::new(Notify::new());
        let caller = tokio::spawn(close_listener(RetryListener {
            attempts: 0,
            started: Arc::clone(&started),
            proceed: Arc::clone(&proceed),
            closed: Arc::clone(&closed),
        }));
        tokio::time::timeout(Duration::from_secs(3), started.notified()).await?;
        assert_ne!(PENDING_CLEANUPS.load(Ordering::SeqCst), 0);
        caller.abort();
        proceed.notify_one();
        tokio::time::timeout(Duration::from_secs(3), closed.notified()).await?;
        tokio::time::timeout(Duration::from_secs(3), async {
            while PENDING_CLEANUPS.load(Ordering::SeqCst) != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        Ok(())
    }

    #[test]
    fn active_status_requires_the_matching_service_and_local_relay() {
        let mut session = ProcessProxySession {
            owner_key: "fixture".into(),
            session_id: 42,
            relay_pid: std::process::id(),
            relay_port: 18081,
            targets: vec![],
        };
        assert!(service_session_matches(&session, 42, 18081, 42));
        assert!(!service_session_matches(&session, 41, 18081, 42));
        assert!(!service_session_matches(&session, 42, 18082, 42));
        assert!(!service_session_matches(&session, 42, 18081, 43));
        session.relay_pid = 0;
        assert!(!service_session_matches(&session, 42, 18081, 42));
    }
}

#[cfg(not(windows))]
#[allow(
    clippy::unused_async,
    reason = "command handlers keep the cross-platform async interface"
)]
pub(crate) async fn detach() -> CmdResult<ProcessProxyStatus> {
    Err(coded_error(
        "C5-PROXY-001",
        "process proxy is only available on Windows",
    ))
}
