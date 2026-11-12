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
use std::{
    collections::HashSet,
    io::{self, Read, Seek},
    path::{Component, Path, PathBuf},
};
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

const MAX_BACKUP_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_BACKUP_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_BACKUP_ENTRIES: usize = 10_000;

struct TempRestoreFile(PathBuf);

impl Drop for TempRestoreFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn validate_backup_filename(filename: &str) -> Result<()> {
    let path = Path::new(filename);
    if filename.is_empty()
        || filename.contains('/')
        || filename.contains('\\')
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
        || !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
    {
        return Err(anyhow!("Invalid backup filename"));
    }
    Ok(())
}

fn extract_backup<R: Read + Seek>(zip: &mut zip::ZipArchive<R>, home: &Path) -> Result<()> {
    if zip.len() > MAX_BACKUP_ENTRIES {
        return Err(anyhow!("Backup archive contains too many entries"));
    }

    let stage = std::env::temp_dir().join(format!("clash5-restore-{}", nanoid::nanoid!()));
    std::fs::create_dir(&stage)?;
    let result = (|| -> Result<()> {
        let mut seen = HashSet::with_capacity(zip.len());
        let mut total_size = 0u64;
        let mut manifest: Option<serde_json::Value> = None;
        let mut files = Vec::new();

        for index in 0..zip.len() {
            let mut entry = zip.by_index(index)?;
            let raw_name = entry.name()?.into_owned();
            if raw_name.contains('\\') || !seen.insert(raw_name.to_lowercase()) {
                return Err(anyhow!("Backup archive contains an invalid or duplicate path"));
            }
            let relative = entry
                .enclosed_name()
                .ok_or_else(|| anyhow!("Invalid backup entry path"))?;
            if entry.is_symlink() {
                return Err(anyhow!("Backup archive contains a symbolic link"));
            }
            if entry.is_dir() {
                if relative != Path::new("profiles") {
                    return Err(anyhow!("Backup archive contains an unsupported directory"));
                }
                std::fs::create_dir_all(stage.join(relative))?;
                continue;
            }
            if !is_allowed_backup_path(&relative) {
                return Err(anyhow!("Backup archive contains an unsupported file"));
            }
            if entry.size() > MAX_BACKUP_ENTRY_BYTES {
                return Err(anyhow!("Backup entry exceeds the 64 MiB limit"));
            }
            let target = stage.join(&relative);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut output = std::fs::File::create(&target)?;
            let copied = io::copy(&mut entry.by_ref().take(MAX_BACKUP_ENTRY_BYTES + 1), &mut output)?;
            if copied > MAX_BACKUP_ENTRY_BYTES || copied != entry.size() {
                return Err(anyhow!("Backup entry size does not match its directory record"));
            }
            total_size = total_size.saturating_add(copied);
            if total_size > MAX_BACKUP_ARCHIVE_BYTES {
                return Err(anyhow!("Backup contents exceed the 512 MiB limit"));
            }
            if relative == Path::new(crate::utils::dirs::BACKUP_MANIFEST) {
                let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&target)?)?;
                let version = value["format_version"].as_u64();
                if value["product"] != "clash5"
                    || !version.is_some_and(|version| {
                        (1..=crate::core::backup::BACKUP_FORMAT_VERSION as u64).contains(&version)
                    })
                {
                    return Err(anyhow!("Unsupported Clash5 backup manifest"));
                }
                manifest = Some(value);
            } else {
                files.push(relative);
            }
        }

        if let Some(manifest) = manifest {
            let includes = manifest["includes"]
                .as_array()
                .ok_or_else(|| anyhow!("Invalid backup manifest includes list"))?;
            if includes.iter().any(|entry| {
                entry
                    .as_str()
                    .is_none_or(|name| !is_allowed_backup_path(Path::new(name)) && name != "profiles/")
            }) {
                return Err(anyhow!("Backup manifest lists unsupported files"));
            }
        }

        let mut database_snapshot = None;
        let mut replacements = Vec::new();
        for relative in &files {
            let staged = stage.join(relative);
            if relative == Path::new(crate::utils::dirs::CLASH5_DATA_DB) {
                database_snapshot = Some(staged);
                continue;
            }
            let target = home.join(relative);
            match replace_file_staged(&staged, &target, home) {
                Ok(previous) => replacements.push((target, previous)),
                Err(error) => {
                    rollback_replacements(replacements);
                    return Err(error);
                }
            }
        }
        let snapshot_result = database_snapshot
            .map(|snapshot| {
                clash5_data::BindingStore::open(home.join(crate::utils::dirs::CLASH5_DATA_DB))
                    .and_then(|mut store| store.restore_from(&snapshot))
            })
            .transpose();
        if let Err(error) = snapshot_result {
            rollback_replacements(replacements);
            return Err(error.into());
        }
        for (_, previous) in replacements {
            if let Some(previous) = previous {
                let _ = std::fs::remove_file(previous);
            }
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&stage);
    result
}

fn is_allowed_backup_path(path: &Path) -> bool {
    let components: Vec<_> = path.components().collect();
    match components.as_slice() {
        [Component::Normal(name)] => name.to_str().is_some_and(|name| {
            name == crate::utils::dirs::BACKUP_MANIFEST
                || name == crate::utils::dirs::CLASH_CONFIG
                || name == crate::utils::dirs::VERGE_CONFIG
                || name == crate::utils::dirs::PROFILE_YAML
                || name == crate::constants::files::DNS_CONFIG
                || name == crate::utils::dirs::CLASH5_DATA_DB
        }),
        [Component::Normal(dir), Component::Normal(name)] => dir.to_str() == Some("profiles") && !name.is_empty(),
        _ => false,
    }
}

fn replace_file_staged(staged: &Path, target: &Path, home: &Path) -> Result<Option<PathBuf>> {
    let parent = target.parent().ok_or_else(|| anyhow!("Invalid restore target"))?;
    std::fs::create_dir_all(parent)?;
    let canonical_home = home.canonicalize()?;
    if !parent.canonicalize()?.starts_with(canonical_home) {
        return Err(anyhow!("Restore target is outside the application directory"));
    }
    let name = target.file_name().ok_or_else(|| anyhow!("Invalid restore target"))?;
    let replacement = parent.join(format!(".{}.restore-{}", name.to_string_lossy(), nanoid::nanoid!()));
    let previous = parent.join(format!(".{}.previous-{}", name.to_string_lossy(), nanoid::nanoid!()));
    std::fs::copy(staged, &replacement)?;
    let had_previous = match std::fs::symlink_metadata(target) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            let _ = std::fs::remove_file(&replacement);
            return Err(anyhow!("Restore target is not a regular file"));
        }
        Ok(_) => {
            std::fs::rename(target, &previous)?;
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    if let Err(error) = std::fs::rename(&replacement, target) {
        if had_previous {
            let _ = std::fs::rename(&previous, target);
        }
        let _ = std::fs::remove_file(&replacement);
        return Err(error.into());
    }
    Ok(had_previous.then_some(previous))
}

fn rollback_replacements(replacements: Vec<(PathBuf, Option<PathBuf>)>) {
    for (target, previous) in replacements.into_iter().rev() {
        let _ = std::fs::remove_file(&target);
        if let Some(previous) = previous {
            let _ = std::fs::rename(previous, target);
        }
    }
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
    validate_backup_filename(filename.as_str())?;
    let verge = Config::verge().await;
    let verge_data = verge.latest_arc();
    let webdav_url = verge_data.webdav_url.clone();
    let webdav_username = verge_data.webdav_username.clone();
    let webdav_password = verge_data.webdav_password.clone();

    let backup_storage_path = std::env::temp_dir().join(format!("clash5-webdav-restore-{}.zip", nanoid::nanoid!()));
    let _cleanup = TempRestoreFile(backup_storage_path.clone());
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

    let value = backup_storage_path.clone();
    let file = AsyncHandler::spawn_blocking(move || std::fs::File::open(&value)).await??;
    let mut zip = zip::ZipArchive::new(file)?;
    let _profile_write = crate::config::profiles::PROFILE_WRITE_LOCK.lock().await;
    extract_backup(&mut zip, &app_home_dir()?)?;
    let res = finalize_restored_verge_config(webdav_url, webdav_username, webdav_password).await;
    let _ = backup_storage_path.remove_if_exists().await;
    res
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
    let source_path = PathBuf::from(source.as_str());
    if !source_path.exists() {
        return Err(anyhow!("Backup file not found: {source}"));
    }
    if !source_path.is_file() {
        return Err(anyhow!("Backup path is not a file: {source}"));
    }
    if fs::metadata(&source_path).await?.len() > MAX_BACKUP_ARCHIVE_BYTES {
        return Err(anyhow!("Backup archive exceeds the 512 MiB limit"));
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
    let target_path = backup_dir.join(filename.as_str());
    if !target_path.exists() {
        logging!(debug, Type::Backup, "Local backup file not found: {}", filename);
        return Ok(());
    }
    target_path.remove_if_exists().await?;
    Ok(())
}

#[tracing::instrument(skip_all, level = "info", fields(filename = %filename))]
pub async fn restore_local_backup(filename: String) -> Result<()> {
    validate_backup_filename(filename.as_str())?;
    let backup_dir = local_backup_dir()?;
    let target_path = backup_dir.join(filename.as_str());
    if !target_path.exists() {
        return Err(anyhow!("Backup file not found: {}", filename));
    }
    if fs::metadata(&target_path).await?.len() > MAX_BACKUP_ARCHIVE_BYTES {
        return Err(anyhow!("Backup archive exceeds the 512 MiB limit"));
    }

    let (webdav_url, webdav_username, webdav_password) = {
        let verge = Config::verge().await;
        let verge = verge.latest_arc();
        (
            verge.webdav_url.clone(),
            verge.webdav_username.clone(),
            verge.webdav_password.clone(),
        )
    };

    let file = AsyncHandler::spawn_blocking(move || std::fs::File::open(&target_path)).await??;
    let mut zip = zip::ZipArchive::new(file)?;
    let _profile_write = crate::config::profiles::PROFILE_WRITE_LOCK.lock().await;
    extract_backup(&mut zip, &app_home_dir()?)?;
    finalize_restored_verge_config(webdav_url, webdav_username, webdav_password).await?;
    Ok(())
}

#[tracing::instrument(skip_all, level = "info", fields(filename = %filename, destination = %destination))]
pub async fn export_local_backup(filename: String, destination: String) -> Result<()> {
    let backup_dir = local_backup_dir()?;
    let source_path = export_source_path(&backup_dir, filename.as_str())?;
    if !source_path.exists() {
        return Err(anyhow!("Backup file not found: {}", filename));
    }

    let dest_path = export_destination_path(destination.as_str())?;
    if source_path == dest_path {
        return Err(anyhow!("Backup export destination must differ from its source"));
    }

    fs::copy(&source_path, &dest_path)
        .await
        .map(|_| ())
        .map_err(|err| anyhow!("Failed to export backup file: {err:#}"))?;
    Ok(())
}

fn export_source_path(backup_dir: &Path, filename: &str) -> Result<PathBuf> {
    let name = Path::new(filename);
    if filename.is_empty()
        || name.is_absolute()
        || name.components().count() != 1
        || !matches!(name.components().next(), Some(Component::Normal(_)))
    {
        return Err(anyhow!("Invalid backup filename"));
    }

    let canonical_dir = backup_dir.canonicalize()?;
    let canonical_source = canonical_dir.join(name).canonicalize()?;
    if !canonical_source.starts_with(&canonical_dir) || !canonical_source.is_file() {
        return Err(anyhow!("Backup file is outside the local backup directory"));
    }
    Ok(canonical_source)
}

fn export_destination_path(destination: &str) -> Result<PathBuf> {
    let path = Path::new(destination);
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(anyhow!("Invalid backup export destination"));
    }

    let file_name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow!("Invalid backup export destination"))?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("Invalid backup export destination"))?
        .canonicalize()?;
    let normalized = parent.join(file_name);

    if let Ok(metadata) = std::fs::symlink_metadata(&normalized) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(anyhow!("Backup export destination must be a regular file"));
        }
        let canonical_destination = normalized.canonicalize()?;
        if !canonical_destination.starts_with(&parent) {
            return Err(anyhow!("Backup export destination is outside its selected directory"));
        }
        return Ok(canonical_destination);
    }

    Ok(normalized)
}

#[cfg(test)]
mod export_path_tests {
    use super::{export_destination_path, export_source_path};
    use std::{fs, path::PathBuf};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("clash5-backup-export-{}", nanoid::nanoid!()));
            fs::create_dir_all(path.join("backups")).unwrap();
            fs::write(path.join("backups").join("backup.zip"), b"backup").unwrap();
            fs::write(path.join("outside.zip"), b"outside").unwrap();
            Self(path)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn export_rejects_traversal() {
        let fixture = Fixture::new();
        let result = export_source_path(&fixture.0.join("backups"), "../outside.zip");
        assert!(result.is_err());
    }

    #[test]
    fn export_rejects_absolute() {
        let fixture = Fixture::new();
        let absolute = fixture.0.join("outside.zip");
        let result = export_source_path(&fixture.0.join("backups"), absolute.to_str().unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn export_requires_an_absolute_dialog_destination_without_parent_traversal() {
        assert!(export_destination_path("relative.zip").is_err());
        let path = std::env::temp_dir().join("..").join("clash5-export.zip");
        assert!(export_destination_path(path.to_str().unwrap()).is_err());
    }
}

#[cfg(test)]
mod restore_archive_tests {
    #![allow(clippy::unwrap_used)]

    use super::{extract_backup, is_allowed_backup_path, validate_backup_filename};
    use std::{
        io::Write as _,
        path::{Path, PathBuf},
    };

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("clash5-restore-test-{}", nanoid::nanoid!()));
            std::fs::create_dir_all(path.join("home")).unwrap();
            Self(path)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn archive(entries: &[(&str, &[u8])]) -> zip::ZipArchive<std::io::Cursor<Vec<u8>>> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        for (name, contents) in entries {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(contents).unwrap();
        }
        zip::ZipArchive::new(writer.finish().unwrap()).unwrap()
    }

    #[test]
    fn restore_allowlist_accepts_only_known_backup_files() {
        assert!(is_allowed_backup_path(Path::new("config.yaml")));
        assert!(is_allowed_backup_path(Path::new("profiles/subscription.yaml")));
        assert!(!is_allowed_backup_path(Path::new("plugins/malware.dll")));
        assert!(!is_allowed_backup_path(Path::new("profiles/../outside.yaml")));
    }

    #[test]
    fn restore_filename_rejects_path_components() {
        assert!(validate_backup_filename("backup.zip").is_ok());
        assert!(validate_backup_filename("../backup.zip").is_err());
        assert!(validate_backup_filename("profiles\\backup.zip").is_err());
    }

    #[test]
    fn restore_rejects_traversal_before_writing_outside_home() {
        let fixture = Fixture::new();
        let mut zip = archive(&[("../escaped.yaml", b"outside")]);
        assert!(extract_backup(&mut zip, &fixture.0.join("home")).is_err());
        assert!(!fixture.0.join("escaped.yaml").exists());
    }

    #[test]
    fn restore_writes_supported_files_from_staging() {
        let fixture = Fixture::new();
        let home = fixture.0.join("home");
        let mut zip = archive(&[("config.yaml", b"restored")]);
        extract_backup(&mut zip, &home).unwrap();
        assert_eq!(std::fs::read(home.join("config.yaml")).unwrap(), b"restored");
    }

    #[test]
    fn restore_rejects_case_colliding_paths() {
        let fixture = Fixture::new();
        let mut zip = archive(&[("profiles/a.yaml", b"first"), ("profiles/A.yaml", b"second")]);
        assert!(extract_backup(&mut zip, &fixture.0.join("home")).is_err());
        assert!(!fixture.0.join("home/profiles/a.yaml").exists());
    }
}
