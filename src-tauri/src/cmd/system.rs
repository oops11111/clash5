use crate::core::{
    notification::{self, PendingFailure},
    runstate::{RUN_STATE, RunStateView},
};

/// Returns one coherent core/service snapshot instead of independently refreshed state.
#[tauri::command]
#[specta::specta]
pub async fn get_runtime_state() -> Result<RunStateView, String> {
    Ok(RUN_STATE.settled().await.to_view())
}

#[tauri::command]
#[specta::specta]
pub async fn get_pending_failures() -> Vec<PendingFailure> {
    notification::pending_failures()
}

#[tauri::command]
#[specta::specta]
pub fn clash5_memory_mitigation_status() -> crate::core::memory_mitigation::MemoryMitigationStatus {
    crate::core::memory_mitigation::status()
}
