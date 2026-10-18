use super::CmdResult;
use crate::{cmd::StringifyErr as _, feat};
use feat::LocalBackupFile;

#[tauri::command]
#[specta::specta]
pub async fn create_local_backup() -> CmdResult<()> {
    feat::create_local_backup().await.stringify_err()
}

#[tauri::command]
#[specta::specta]
pub async fn list_local_backup() -> CmdResult<Vec<LocalBackupFile>> {
    feat::list_local_backup().await.stringify_err()
}

#[tauri::command]
#[specta::specta]
pub async fn delete_local_backup(filename: std::string::String) -> CmdResult<()> {
    feat::delete_local_backup(filename.into()).await.stringify_err()
}

#[tauri::command]
#[specta::specta]
pub async fn restore_local_backup(filename: std::string::String) -> CmdResult<()> {
    feat::restore_local_backup(filename.into())
        .await
        .map_err(|error| super::proxy_aware_error(&error))
}

#[tauri::command]
#[specta::specta]
pub async fn import_local_backup(source: std::string::String) -> CmdResult<std::string::String> {
    feat::import_local_backup(source.into())
        .await
        .stringify_err()
        .map(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn export_local_backup(filename: std::string::String, destination: std::string::String) -> CmdResult<()> {
    feat::export_local_backup(filename.into(), destination.into())
        .await
        .stringify_err()
}
