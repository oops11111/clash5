use crate::{
    config::{Config, IClashTemp, IProfiles, IVerge},
    core::{CoreManager, backup, proxy_control, proxy_control::SystemProxyStateUnknown},
    process::AsyncHandler,
    utils::{
        dirs::{PathBufExec as _, app_home_dir, local_backup_dir, verge_path},
        help,
    },
};
use anyhow::{Result, anyhow};
use chrono::Utc;
use clash_verge_logging::{Type, logging, logging_error};
use reqwest_dav::list_cmd::ListFile;
use serde::Serialize;
use smartstring::alias::String;
use std::{io, path::PathBuf};
use tokio::fs;

#[derive(Debug, Serialize, specta::Type)]
pub struct LocalBackupFile {
    #[specta(type = std::string::String)]
    pub filename: String,
    #[specta(type = std::string::String)]
    pub path: String,
    #[specta(type = std::string::String)]
    pub last_modified: String,
    #[specta(type = specta_typescript::Number)]
    pub content_length: u64,
}

async fn restore_backup_file(
    source: PathBuf,
    webdav_url: Option<String>,
    webdav_username: Option<String>,
    webdav_password: Option<String>,
) -> Result<()> {
    let home = app_home_dir()?;
    let stage_home = home.clone();
    let staged = AsyncHandler::spawn_blocking(move || {
        super::backup_archive::StagedBackup::prepare(std::fs::File::open(source)?, &stage_home)
    })
    .await??;

    let mut verge: serde_yaml_ng::Mapping = help::read_yaml(&staged.path("verge.yaml")).await?;
    for key in ["webdav_url", "webdav_username", "webdav_password"] {
        verge.remove(serde_yaml_ng::Value::String(key.into()));
    }
    let mut restored = crate::config::with_encryption(|| async {
        serde_yaml_ng::from_value::<IVerge>(serde_yaml_ng::Value::Mapping(verge))
    })
    .await?;
    restored.webdav_url = webdav_url.clone();
    restored.webdav_username = webdav_username.clone();
    restored.webdav_password = webdav_password.clone();
    help::save_yaml(&staged.path("verge.yaml"), &restored, None).await?;
    let _: serde_yaml_ng::Mapping = help::read_yaml(&staged.path("config.yaml")).await?;
    let dns = staged.path(crate::constants::files::DNS_CONFIG);
    if dns.is_file() {
        let _: serde_yaml_ng::Mapping = help::read_yaml(&dns).await?;
    }
    let profiles: IProfiles = help::read_yaml(&staged.path("profiles.yaml")).await?;
    for item in profiles.items.as_deref().unwrap_or_default() {
        if let Some(file) = &item.file {
            anyhow::ensure!(
                super::backup_archive::safe_profile_file(file),
                "Invalid profile filename in backup"
            );
            anyhow::ensure!(
                staged.path("profiles").join(file.as_str()).is_file(),
                "Backup is missing a referenced profile"
            );
            if !file.ends_with(".js") {
                let _: serde_yaml_ng::Mapping = help::read_yaml(&staged.path("profiles").join(file.as_str())).await?;
            }
        }
    }
    let snapshot = staged.path(crate::utils::dirs::CLASH5_DATA_DB);
    if snapshot.is_file() {
        AsyncHandler::spawn_blocking(move || super::backup_archive::validate_database(&snapshot)).await??;
    }
    let _profile_write = crate::config::profiles::PROFILE_WRITE_LOCK.lock().await;
    AsyncHandler::spawn_blocking(move || {
        staged.publish(&home, |snapshot, previous| {
            let path = home.join(crate::utils::dirs::CLASH5_DATA_DB);
            super::backup_archive::restore_database(snapshot, previous, &path)
        })
    })
    .await??;
    finalize_restored_verge_config(webdav_url, webdav_username, webdav_password).await
}

/// Reloads restored configs into memory while preserving WebDAV credentials.
async fn finalize_restored_verge_config(
    webdav_url: Option<String>,
    webdav_username: Option<String>,
    webdav_password: Option<String>,
) -> Result<()> {
    // A broken restored `verge.yaml` is a restore failure, not a reason to load defaults.
    let mut restored = help::read_yaml::<IVerge>(&verge_path()?).await?;
    restored.webdav_url = webdav_url;
    restored.webdav_username = webdav_username;
    restored.webdav_password = webdav_password;
    restored.save_file().await?;

    let restored_clash = IClashTemp::new().await;
    let clash_draft = Config::clash().await;
    clash_draft.edit_draft(|d| {
        *d = restored_clash.clone();
    });
    clash_draft.apply();

    let restored_profiles = IProfiles::new().await;
    let profiles_draft = Config::profiles().await;
    profiles_draft.edit_draft(|d| {
        *d = restored_profiles.clone();
    });
    profiles_draft.apply();

    let verge_draft = Config::verge().await;
    {
        // Hold the write lock so a concurrent patch cannot stage a draft the proxy write reads.
        let _config_write = Config::lock_config_write().await;
        verge_draft.edit_draft(|d| {
            *d = restored.clone();
        });
        verge_draft.apply();

        // Turn it off here; a failing core side effect below would otherwise skip this step.
        if !restored.enable_system_proxy.unwrap_or_default() {
            let result = async {
                let _lifecycle = CoreManager::global().lifecycle_lock.lock().await;
                proxy_control::apply().await?;
                proxy_control::refresh_guard().await
            }
            .await;
            if let Err(err) = result {
                logging!(error, Type::Backup, "Failed to turn the system proxy off: {err:#}");
                if SystemProxyStateUnknown::is_in(&err) {
                    return Err(err);
                }
            }
        }
    }

    // Run configuration side effects without rewriting the already-restored file.
    if let Err(err) = super::patch_verge(&restored, true).await {
        logging!(error, Type::Backup, "Failed to apply restored verge config: {err:#}");
        // Propagate unknown proxy state; ordinary side-effect failures stay logged.
        if SystemProxyStateUnknown::is_in(&err) {
            return Err(err);
        }
    }
    logging_error!(Type::Config, Config::sync_dns_override().await);
    Ok(())
}

#[tracing::instrument(skip_all, level = "info")]
pub async fn create_backup_and_upload_webdav() -> Result<()> {
    let (file_name, temp_file_path) = backup::create_backup().await.map_err(|err| {
        logging!(error, Type::Backup, "Failed to create backup: {err:#}");
        err
    })?;

    if let Err(err) = backup::WebDavClient::global()
        .upload(temp_file_path.clone(), file_name)
        .await
    {
        logging!(error, Type::Backup, "Failed to upload to WebDAV: {err:#}");
        backup::WebDavClient::global().reset();
        return Err(err);
    }

    if let Err(err) = temp_file_path.remove_if_exists().await {
        logging!(warn, Type::Backup, "Failed to remove temp file: {err:#}");
    }

    Ok(())
}

pub async fn list_wevdav_backup() -> Result<Vec<ListFile>> {
    backup::WebDavClient::global().list().await.map_err(|err| {
        logging!(error, Type::Backup, "Failed to list WebDAV backup files: {err:#}");
        err
    })
}

pub async fn delete_webdav_backup(filename: String) -> Result<()> {
    super::backup_path::validate_backup_filename(filename.as_str())?;
    let name = filename.to_string();
    backup::WebDavClient::global().delete(filename).await.map_err(|err| {
        logging!(
            error,
            Type::Backup,
            "Failed to delete WebDAV backup file {name}: {err:#}"
        );
        err
    })
}

#[tracing::instrument(skip_all, level = "info", fields(filename = %filename))]
pub async fn restore_webdav_backup(filename: String) -> Result<()> {
    super::backup_path::validate_backup_filename(filename.as_str())?;
    let verge = Config::verge().await;
    let verge_data = verge.latest_arc();
    let webdav_url = verge_data.webdav_url.clone();
    let webdav_username = verge_data.webdav_username.clone();
    let webdav_password = verge_data.webdav_password.clone();

    let backup_storage_path = app_home_dir()?.join(format!(".clash5-download-{}.zip", nanoid::nanoid!()));
    let cleanup_path = backup_storage_path.clone();
    let _download_cleanup = scopeguard::guard(cleanup_path, |path| {
        let _ = std::fs::remove_file(path);
    });
    let name = filename.to_string();
    backup::WebDavClient::global()
        .download(filename, backup_storage_path.clone())
        .await
        .map_err(|err| {
            logging!(
                error,
                Type::Backup,
                "Failed to download WebDAV backup file {name}: {err:#}"
            );
            err
        })?;

    restore_backup_file(backup_storage_path, webdav_url, webdav_username, webdav_password).await
}

pub async fn create_local_backup() -> Result<()> {
    create_local_backup_with_namer(|name| name.to_string().into())
        .await
        .map(|_| ())
}

pub async fn create_local_backup_with_namer<F>(namer: F) -> Result<String>
where
    F: FnOnce(&str) -> String,
{
    let (file_name, temp_file_path) = backup::create_backup().await.map_err(|err| {
        logging!(error, Type::Backup, "Failed to create local backup: {err:#}");
        err
    })?;

    let backup_dir = local_backup_dir()?;
    let final_name = namer(file_name.as_str());
    let target_path = backup_dir.join(final_name.as_str());

    if let Err(err) = move_file(temp_file_path.clone(), target_path.clone()).await {
        logging!(
            error,
            Type::Backup,
            "Failed to move local backup file to {}: {err:#}",
            target_path.display()
        );
        if let Err(clean_err) = temp_file_path.remove_if_exists().await {
            logging!(
                warn,
                Type::Backup,
                "Failed to remove temp backup file after move error: {clean_err:#}"
            );
        }
        return Err(err);
    }

    Ok(final_name)
}

#[tracing::instrument(skip_all, level = "info", fields(source = %source))]
pub async fn import_local_backup(source: String) -> Result<String> {
    use tauri_plugin_fs::FsExt as _;

    let source_path = PathBuf::from(source.as_str());
    anyhow::ensure!(
        crate::core::handle::Handle::app_handle()
            .fs_scope()
            .is_allowed(&source_path),
        "Backup source must be selected through the open dialog"
    );
    anyhow::ensure!(
        fs::metadata(&source_path).await?.len() <= super::backup_archive::MAX_ARCHIVE_BYTES,
        "Backup archive exceeds 64 MiB"
    );
    if !source_path.exists() {
        return Err(anyhow!("Backup file not found: {source}"));
    }
    if !source_path.is_file() {
        return Err(anyhow!("Backup path is not a file: {source}"));
    }

    let ext = source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();
    if ext != "zip" {
        return Err(anyhow!("Only .zip backup files are supported"));
    }

    let file_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("Invalid backup file name"))?;

    let backup_dir = local_backup_dir()?;
    let target_path = backup_dir.join(file_name);

    if target_path == source_path {
        return Ok(file_name.to_string().into());
    }

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    if target_path.exists() {
        return Err(anyhow!("Backup file already exists: {file_name}"));
    }

    fs::copy(&source_path, &target_path)
        .await
        .map_err(|err| anyhow!("Failed to import backup file: {err:#}"))?;

    Ok(file_name.to_string().into())
}

async fn move_file(from: PathBuf, to: PathBuf) -> Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).await?;
    }

    match fs::rename(&from, &to).await {
        Ok(_) => Ok(()),
        Err(rename_err) => {
            // Rename can fail across filesystems; fall back to copy then remove.
            logging!(
                warn,
                Type::Backup,
                "Failed to rename backup file directly, fallback to copy/remove: {rename_err}"
            );
            fs::copy(&from, &to)
                .await
                .map_err(|err| anyhow!("Failed to copy backup file: {err:#}"))?;
            fs::remove_file(&from)
                .await
                .map_err(|err| anyhow!("Failed to remove temp backup file: {err:#}"))?;
            Ok(())
        }
    }
}

pub async fn list_local_backup() -> Result<Vec<LocalBackupFile>> {
    let backup_dir = local_backup_dir()?;
    if !backup_dir.exists() {
        return Ok(vec![]);
    }

    let mut backups = Vec::new();
    let mut dir = fs::read_dir(&backup_dir).await?;
    while let Some(entry) = dir.next_entry().await? {
        let path = entry.path();
        let metadata = entry.metadata().await?;
        if !metadata.is_file() {
            continue;
        }

        let file_name = match path.file_name().and_then(|name| name.to_str()) {
            Some(name) => name,
            None => continue,
        };
        let last_modified = metadata
            .modified()
            .map(|time| chrono::DateTime::<Utc>::from(time).to_rfc3339())
            .unwrap_or_default();
        backups.push(LocalBackupFile {
            filename: file_name.into(),
            path: path.to_string_lossy().into(),
            last_modified: last_modified.into(),
            content_length: metadata.len(),
        });
    }

    backups.sort_by(|a, b| b.filename.cmp(&a.filename));
    Ok(backups)
}

pub async fn delete_local_backup(filename: String) -> Result<()> {
    let backup_dir = local_backup_dir()?;
    let target_path = match super::backup_path::backup_source(&backup_dir, filename.as_str()) {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    target_path.remove_if_exists().await?;
    Ok(())
}

#[tracing::instrument(skip_all, level = "info", fields(filename = %filename))]
pub async fn restore_local_backup(filename: String) -> Result<()> {
    let backup_dir = local_backup_dir()?;
    let target_path = super::backup_path::backup_source(&backup_dir, filename.as_str())?;

    let (webdav_url, webdav_username, webdav_password) = {
        let verge = Config::verge().await;
        let verge = verge.latest_arc();
        (
            verge.webdav_url.clone(),
            verge.webdav_username.clone(),
            verge.webdav_password.clone(),
        )
    };

    restore_backup_file(target_path, webdav_url, webdav_username, webdav_password).await
}

#[tracing::instrument(skip_all, level = "info", fields(filename = %filename, destination = %destination))]
pub async fn export_local_backup(filename: String, destination: String) -> Result<()> {
    use tauri_plugin_fs::FsExt as _;

    let backup_dir = local_backup_dir()?;
    let source_path = super::backup_path::backup_source(&backup_dir, filename.as_str())?;

    let dest_path = PathBuf::from(destination.as_str());
    if !dest_path.is_absolute()
        || dest_path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
        || !crate::core::handle::Handle::app_handle()
            .fs_scope()
            .is_allowed(&dest_path)
    {
        return Err(anyhow!("Export destination must be selected through the save dialog"));
    }

    fs::copy(&source_path, &dest_path)
        .await
        .map(|_| ())
        .map_err(|err| anyhow!("Failed to export backup file: {err:#}"))?;
    Ok(())
}
