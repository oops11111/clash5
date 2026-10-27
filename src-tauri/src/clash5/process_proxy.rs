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
use std::{sync::OnceLock, time::Duration};
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
    targets: Vec<clash_verge_service_ipc::ProcessProxyTarget>,
    shutdown: Option<oneshot::Sender<()>>,
    relay_task: tokio::task::JoinHandle<std::io::Result<()>>,
    listener: ChainListener,
}

#[cfg(windows)]
static ACTIVE: OnceLock<Mutex<Option<ActiveProcessProxy>>> = OnceLock::new();

#[cfg(windows)]
fn active() -> &'static Mutex<Option<ActiveProcessProxy>> {
    ACTIVE.get_or_init(|| Mutex::new(None))
}

#[cfg(windows)]
fn process_proxy_error(code: &str, detail: impl std::fmt::Display) -> crate::cmd::CommandFailure {
    coded_error(code, detail)
}

#[cfg(windows)]
async fn cleanup(mut session: ActiveProcessProxy) -> anyhow::Result<()> {
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
    let listener_closed = session.listener.close().await.is_ok();
    if relay_stopped && listener_closed {
        Ok(())
    } else {
        Err(anyhow::anyhow!("C5-PROXY-004"))
    }
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
async fn service_status() -> anyhow::Result<ProcessProxyStatus> {
    let proof = service::active_service_session()?;
    let credentials = current_owner_credentials()?;
    let response = clash_verge_service_ipc::get_process_proxy_status(&credentials, &proof).await?;
    if response.code != 0 {
        return Err(anyhow::anyhow!("C5-PROXY-005: {}", response.message));
    }
    let status = response
        .data
        .map(status_from_service)
        .ok_or_else(|| anyhow::anyhow!("C5-PROXY-005"))?;
    Ok(match status {
        ProcessProxyStatus::Active { .. } => ProcessProxyStatus::RecoveryRequired {
            detail: "the service retained an active session but this app has no local relay".into(),
        },
        other => other,
    })
}

#[cfg(windows)]
pub(crate) async fn status() -> CmdResult<ProcessProxyStatus> {
    if let Some(session) = active().lock().await.as_ref() {
        if session.relay_task.is_finished() {
            return Ok(ProcessProxyStatus::RecoveryRequired {
                detail: "the local redirect relay stopped unexpectedly".into(),
            });
        }
        return Ok(ProcessProxyStatus::Active {
            chain_id: session.chain_id.clone(),
            relay_port: session.relay_port,
            target_count: session.targets.len() as u32,
        });
    }
    service_status()
        .await
        .map_err(|error| process_proxy_error("C5-PROXY-005", error))
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
    listener
        .activate()
        .await
        .map_err(|_| process_proxy_error("C5-PROXY-008", "the chain listener could not be activated"))?;
    let relay = match clash_verge_service_ipc::TcpRedirectRelay::bind(
        proof.generation,
        listener.port(),
        listener.token(),
    )
    .await
    {
        Ok(relay) => relay,
        Err(error) => {
            let _ = listener.close().await;
            return Err(process_proxy_error("C5-PROXY-008", error));
        }
    };
    let relay_port = relay
        .port()
        .map_err(|error| process_proxy_error("C5-PROXY-008", error))?;
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
                targets: vec![target],
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
            targets: vec![target],
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
                targets: vec![target],
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
        targets: vec![target],
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
    let session = active_session.take();
    drop(active_session);
    if let Some(session) = session {
        cleanup(session)
            .await
            .map_err(|error| process_proxy_error("C5-PROXY-004", error))?;
    }
    Ok(ProcessProxyStatus::Idle)
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
