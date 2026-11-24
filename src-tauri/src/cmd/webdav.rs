use super::CmdResult;
use crate::{
    cmd::StringifyErr as _,
    config::{Config, IVerge},
    core, feat,
};
use reqwest_dav::list_cmd::ListFile;

#[derive(specta::Type, serde::Serialize)]
pub struct WebDavBackupEntry {
    href: std::string::String,
    last_modified: chrono::DateTime<chrono::Utc>,
    #[specta(type = specta_typescript::Number)]
    content_length: i64,
    content_type: std::string::String,
    tag: Option<std::string::String>,
}

#[tauri::command]
#[specta::specta]
pub async fn save_webdav_config(
    url: std::string::String,
    username: std::string::String,
    password: std::string::String,
) -> CmdResult<()> {
    let patch = IVerge {
        webdav_url: Some(url.into()),
        webdav_username: Some(username.into()),
        webdav_password: Some(password.into()),
        ..IVerge::default()
    };
    Config::verge().await.edit_draft(|e| e.patch_config(&patch));
    Config::verge().await.apply();

    let verge_data = Config::verge().await.data_arc();
    verge_data.save_file().await.stringify_err()?;
    core::backup::WebDavClient::global().reset();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn create_webdav_backup() -> CmdResult<()> {
    feat::create_backup_and_upload_webdav().await.stringify_err()
}

#[tauri::command]
#[specta::specta]
pub async fn list_webdav_backup() -> CmdResult<Vec<WebDavBackupEntry>> {
    feat::list_wevdav_backup().await.stringify_err().map(|files| {
        files
            .into_iter()
            .map(|file: ListFile| WebDavBackupEntry {
                href: file.href,
                last_modified: file.last_modified,
                content_length: file.content_length,
                content_type: file.content_type,
                tag: file.tag,
            })
            .collect()
    })
}

#[tauri::command]
#[specta::specta]
pub async fn delete_webdav_backup(filename: std::string::String) -> CmdResult<()> {
    feat::delete_webdav_backup(filename.into()).await.stringify_err()
}

#[tauri::command]
#[specta::specta]
pub async fn restore_webdav_backup(filename: std::string::String) -> CmdResult<()> {
    feat::restore_webdav_backup(filename.into())
        .await
        .map_err(|error| super::proxy_aware_error(&error))
}
