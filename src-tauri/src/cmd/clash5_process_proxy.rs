use super::CmdResult;
use crate::clash5::process_proxy::{self, ProcessProxyStatus};

#[tauri::command]
#[specta::specta]
pub async fn clash5_process_proxy_status() -> CmdResult<ProcessProxyStatus> {
    process_proxy::status().await
}

#[tauri::command]
#[specta::specta]
pub async fn clash5_attach_process_proxy(chain_id: String, pid: u32) -> CmdResult<ProcessProxyStatus> {
    process_proxy::attach(chain_id, pid).await
}

#[tauri::command]
#[specta::specta]
pub async fn clash5_detach_process_proxy() -> CmdResult<ProcessProxyStatus> {
    process_proxy::detach().await
}
