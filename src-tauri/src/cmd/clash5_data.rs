use super::{CmdResult, coded_error};
use clash5_data::{
    Binding, DataError, Node, NodeQuality, NodeRecord, PendingIdentityRebind, PluginAuditEvent, PluginBinding,
    PluginBindingTarget, QualityStatus, Source, parse_inline_nodes,
};
use futures::{StreamExt as _, stream};
use serde::Serialize;
use serde_yaml_ng::Value;
use sha2::Digest as _;
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    clash5::{
        import, manual,
        plugins::{self, PluginFailure},
    },
    config::{
        Config, PrfItem,
        profiles::{PROFILE_WRITE_LOCK, profiles_append_item_with_filedata_safe, profiles_save_file_safe},
    },
    core::{CoreManager, handle::Handle, proxy_view::ProxyNodeSource, runtime_bundle::remote_providers_of},
    utils::dirs,
};

const SHORT_PROBE_MS: u32 = 1_500;
const HANDSHAKE_PROBE_MS: u32 = 10_000;
const PROBE_CONCURRENCY: usize = 16;
const SHORT_PROBE_URL: &str = "http://cp.cloudflare.com/generate_204";
const HANDSHAKE_PROBE_URL: &str = "https://cp.cloudflare.com/generate_204";
static PROBE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn failure(error: DataError) -> super::CommandFailure {
    coded_error(&error.to_string(), "clash5 node data operation failed")
}

async fn apply_plugin_change_with_rollback<Update, UpdateFuture, Restore, RestoreFuture, Outcome>(
    mut update: Update,
    restore: Restore,
    is_valid: impl Fn(&Outcome) -> bool,
) -> CmdResult
where
    Update: FnMut() -> UpdateFuture,
    UpdateFuture: Future<Output = anyhow::Result<Outcome>>,
    Restore: FnOnce() -> RestoreFuture,
    RestoreFuture: Future<Output = Result<(), DataError>>,
{
    if matches!(update().await, Ok(ref outcome) if is_valid(outcome)) {
        return Ok(());
    }
    restore()
        .await
        .map_err(|_| coded_error("C5-PLUGIN-004", "plugin binding could not be restored"))?;
    if !matches!(update().await, Ok(ref outcome) if is_valid(outcome)) {
        return Err(coded_error("C5-PLUGIN-004", "previous runtime could not be restored"));
    }
    Err(coded_error(
        "C5-PLUGIN-005",
        "plugin configuration was rejected; the previous binding was restored",
    ))
}

#[derive(Serialize)]
pub struct ManualNodeIdentity {
    stable_id: String,
}

#[tauri::command]
pub async fn clash5_register_manual_node(node: Node) -> CmdResult<ManualNodeIdentity> {
    crate::config::profiles::with_clash5_binding_store(move |store| {
        let stable_id = store.upsert_manual(&node)?;
        Ok(ManualNodeIdentity { stable_id })
    })
    .await
    .map_err(failure)
}

#[derive(Serialize)]
pub struct ManualImportResult {
    imported: usize,
}

#[derive(Serialize)]
pub struct VergeImportReport {
    imported: usize,
    skipped: Vec<String>,
    binding_sync_failures: usize,
    source_manifest_before: String,
    source_manifest_after: String,
}

#[tauri::command]
pub async fn clash5_import_verge_data(source_dir: String) -> CmdResult<VergeImportReport> {
    let source = PathBuf::from(source_dir.as_str());
    let target =
        dirs::app_home_dir().map_err(|_| coded_error("C5-IMPORT-001", "clash5 data directory is unavailable"))?;
    import::ensure_separate_data_dirs(&source, &target)
        .map_err(|_| coded_error("C5-IMPORT-001", "source and clash5 data directories must be separate"))?;
    let manifest_before = import::source_manifest(&source)
        .map_err(|_| coded_error("C5-IMPORT-001", "could not inspect the selected source directory"))?;
    let (profiles, skipped) = import::read_supported_profiles(&source)
        .map_err(|_| coded_error("C5-IMPORT-002", "could not read supported cached profiles"))?;
    let manifest_after_read = import::source_manifest(&source)
        .map_err(|_| coded_error("C5-IMPORT-003", "could not verify the source directory"))?;
    if manifest_before != manifest_after_read {
        return Err(coded_error(
            "C5-IMPORT-004",
            "source directory changed while it was being read",
        ));
    }

    let _profile_guard = PROFILE_WRITE_LOCK.lock().await;
    let mut binding_sync_failures = 0;
    for profile in &profiles {
        let item = PrfItem {
            itype: Some("local".into()),
            name: Some(profile.name.clone().into()),
            ..PrfItem::default()
        };
        let uid = profiles_append_item_with_filedata_safe(&item, Some(profile.contents.clone().into()))
            .await
            .map_err(|_| coded_error("C5-IMPORT-005", "could not save an imported profile"))?;
        if let Some(uid) = uid
            && let crate::config::profiles::BindingSync::Failed(_) =
                crate::config::profiles::reconcile_clash5_snapshot(uid, profile.contents.clone()).await
        {
            binding_sync_failures += 1;
        }
    }
    if !profiles.is_empty() {
        profiles_save_file_safe()
            .await
            .map_err(|_| coded_error("C5-IMPORT-005", "could not save imported profile metadata"))?;
    }

    let manifest_after = import::source_manifest(&source)
        .map_err(|_| coded_error("C5-IMPORT-006", "could not verify source preservation"))?;
    if manifest_before != manifest_after {
        return Err(coded_error("C5-IMPORT-007", "source directory changed during import"));
    }

    Ok(VergeImportReport {
        imported: profiles.len(),
        skipped,
        binding_sync_failures,
        source_manifest_before: manifest_before,
        source_manifest_after: manifest_after,
    })
}

async fn restore_manual_file(previous: Option<&str>) -> CmdResult {
    manual::restore(previous)
        .await
        .map_err(|_| coded_error("MANUAL_IMPORT_ROLLBACK_FAILED", "could not restore manual proxy file"))?;
    let outcome = CoreManager::global().update_config_forced().await.map_err(|_| {
        coded_error(
            "MANUAL_IMPORT_ROLLBACK_FAILED",
            "could not restore running configuration",
        )
    })?;
    if !outcome.is_valid() {
        return Err(coded_error(
            "MANUAL_IMPORT_ROLLBACK_FAILED",
            "running configuration was not restored",
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn clash5_import_manual_proxies(proxies: Vec<Value>) -> CmdResult<ManualImportResult> {
    let _profile_guard = PROFILE_WRITE_LOCK.lock().await;
    let path = manual::path().map_err(|_| coded_error("MANUAL_IMPORT_FAILED", "manual proxy path is unavailable"))?;
    let previous = match tokio::fs::read_to_string(&path).await {
        Ok(yaml) => Some(yaml),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err(coded_error("MANUAL_IMPORT_FAILED", "manual proxy file is unavailable")),
    };
    let mut file = manual::load()
        .await
        .map_err(|_| coded_error("MANUAL_IMPORT_FAILED", "manual proxy file is invalid"))?;
    let identity_key = crate::config::profiles::clash5_identity_key()
        .await
        .map_err(|_| coded_error("MANUAL_IMPORT_FAILED", "node identity is in recovery mode"))?;
    let import = file
        .add_import(proxies, &identity_key)
        .map_err(|_| coded_error("MANUAL_IMPORT_FAILED", "manual proxy import is invalid"))?;
    let imported = import.nodes.len();
    manual::save(&file)
        .await
        .map_err(|_| coded_error("MANUAL_IMPORT_FAILED", "manual proxy file could not be saved"))?;
    if manual::load()
        .await
        .and_then(|saved| saved.runtime_proxies(&identity_key))
        .is_err()
    {
        manual::restore(previous.as_deref())
            .await
            .map_err(|_| coded_error("MANUAL_IMPORT_ROLLBACK_FAILED", "could not restore manual proxy file"))?;
        return Err(coded_error(
            "MANUAL_IMPORT_FAILED",
            "manual proxy file could not be read back",
        ));
    }

    let applied = CoreManager::global().update_config_forced().await;
    if !applied.as_ref().is_ok_and(|outcome| outcome.is_valid()) {
        restore_manual_file(previous.as_deref()).await?;
        return Err(coded_error(
            "MANUAL_IMPORT_FAILED",
            "manual proxy configuration was rejected",
        ));
    }

    let saved = crate::config::profiles::with_clash5_binding_store(move |store| {
        store.upsert_manual_batch(&import.nodes, &import.changed_ids)
    })
    .await;
    if saved.is_err() {
        restore_manual_file(previous.as_deref()).await?;
        return Err(coded_error(
            "MANUAL_IMPORT_FAILED",
            "manual proxy inventory could not be saved",
        ));
    }
    Ok(ManualImportResult { imported })
}

#[tauri::command]
pub async fn clash5_bind_node(binding: Binding) -> CmdResult {
    crate::config::profiles::with_clash5_binding_store(move |store| store.upsert_binding(&binding))
        .await
        .map_err(failure)
}

#[tauri::command]
pub async fn clash5_list_bindings(source: Option<Source>) -> CmdResult<Vec<Binding>> {
    crate::config::profiles::with_clash5_binding_store(move |store| store.bindings(source.as_ref()))
        .await
        .map_err(failure)
}

#[tauri::command]
pub async fn clash5_list_plugin_bindings() -> CmdResult<Vec<PluginBindingSummary>> {
    crate::config::profiles::with_clash5_binding_store(|store| {
        store
            .plugin_bindings(None)
            .map(|bindings| bindings.into_iter().map(PluginBindingSummary::from).collect())
    })
    .await
    .map_err(failure)
}

#[tauri::command]
pub async fn clash5_list_pending_identity_rebinds(source: Option<Source>) -> CmdResult<Vec<PendingIdentityRebind>> {
    crate::config::profiles::with_clash5_binding_store(move |store| store.pending_identity_rebinds(source.as_ref()))
        .await
        .map_err(failure)
}

#[tauri::command]
pub async fn clash5_resolve_identity_rebind(source: Source, old_id: String, new_id: String) -> CmdResult {
    crate::config::profiles::with_clash5_binding_store(move |store| {
        store.resolve_identity_rebind(&source, &old_id, &new_id)
    })
    .await
    .map_err(failure)
}

#[derive(Serialize)]
pub struct Clash5IdentityStatus {
    recovery_mode: bool,
    pending_rebinds: usize,
}

#[tauri::command]
pub async fn clash5_identity_status() -> CmdResult<Clash5IdentityStatus> {
    crate::config::profiles::with_clash5_binding_store(|store| {
        Ok(Clash5IdentityStatus {
            recovery_mode: store.identity_recovery(),
            pending_rebinds: store.pending_identity_rebinds(None)?.len(),
        })
    })
    .await
    .map_err(failure)
}

#[tauri::command]
pub async fn clash5_get_plugin_binding_config(
    target: PluginBindingTarget,
    binding_id: String,
) -> CmdResult<serde_json::Value> {
    let binding = crate::config::profiles::with_clash5_binding_store(move |store| {
        store
            .plugin_bindings(Some(&target))?
            .into_iter()
            .find(|binding| binding.id == binding_id)
            .ok_or(DataError::NotFound)
    })
    .await
    .map_err(failure)?;
    if let Some(encrypted) = binding
        .config
        .get("__clash5_encrypted")
        .and_then(serde_json::Value::as_str)
    {
        let clear = crate::config::decrypt_data(encrypted)
            .map_err(|_| coded_error("C5-PLUGIN-004", "plugin settings could not be decrypted"))?;
        serde_json::from_str(&clear).map_err(|_| coded_error("C5-PLUGIN-004", "plugin settings are invalid"))
    } else {
        Ok(binding.config)
    }
}

#[derive(Serialize)]
pub struct PluginEffectiveConfig {
    config: serde_json::Value,
    provenance: std::collections::BTreeMap<String, clash5_data::ConfigProvenance>,
    conflicts: Vec<clash5_data::PluginConflict>,
}

#[tauri::command]
pub async fn clash5_get_plugin_effective_config() -> CmdResult<PluginEffectiveConfig> {
    let runtime = Config::runtime().await.latest_arc();
    let mut config = serde_json::to_value(runtime.config.as_ref())
        .map_err(|_| coded_error("C5-PLUGIN-004", "effective configuration could not be read"))?;
    redact_plugin_config(&mut config, None);
    Ok(PluginEffectiveConfig {
        config,
        provenance: runtime.plugin_provenance.clone(),
        conflicts: runtime.plugin_conflicts.clone(),
    })
}

fn redact_plugin_config(value: &mut serde_json::Value, key: Option<&str>) {
    if key.is_some_and(|key| {
        let key = key.to_ascii_lowercase().replace('_', "-");
        [
            "password",
            "secret",
            "token",
            "credential",
            "authorization",
            "auth",
            "uuid",
            "username",
            "private",
            "url",
        ]
        .iter()
        .any(|sensitive| key.contains(sensitive))
    }) {
        *value = serde_json::Value::String("[redacted]".into());
        return;
    }
    match value {
        serde_json::Value::Object(object) => {
            for (key, child) in object {
                redact_plugin_config(child, Some(key));
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_plugin_config(item, key);
            }
        }
        _ => {}
    }
}

#[tauri::command]
pub async fn clash5_save_plugin_binding(mut binding: PluginBinding) -> CmdResult {
    let _profile_guard = PROFILE_WRITE_LOCK.lock().await;
    if !binding.config.is_object() {
        return Err(coded_error(
            "C5-PLUGIN-001",
            "plugin binding settings must be an object",
        ));
    }
    let plugin_root = dirs::app_home_dir()
        .map_err(super::CommandFailure::plain)?
        .join("plugins");
    let installed = tokio::task::spawn_blocking(move || plugins::installed_plugins(&plugin_root))
        .await
        .map_err(|_| coded_error("C5-PLUGIN-004", "installed plugins could not be checked"))?
        .map_err(plugin_failure)?;
    if !installed.iter().any(|plugin| plugin.id == binding.plugin_id) {
        return Err(coded_error("C5-PLUGIN-001", "plugin is not installed"));
    }
    let clear_config = serde_json::to_string(&binding.config)
        .map_err(|_| coded_error("C5-PLUGIN-001", "plugin binding settings are invalid"))?;
    let encrypted = crate::config::encrypt_data(&clear_config)
        .map_err(|_| coded_error("C5-PLUGIN-004", "plugin binding settings could not be protected"))?;
    binding.config = serde_json::json!({"__clash5_encrypted": encrypted});
    let target = binding.target.clone();
    let binding_id = binding.id.clone();
    let previous = crate::config::profiles::with_clash5_binding_store(move |store| {
        let previous = store
            .plugin_bindings(Some(&binding.target))?
            .into_iter()
            .find(|item| item.id == binding.id);
        store.upsert_plugin_binding(&binding)?;
        Ok(previous)
    })
    .await
    .map_err(failure)?;
    apply_plugin_change_with_rollback(
        || CoreManager::global().update_config_forced(),
        move || {
            crate::config::profiles::with_clash5_binding_store(move |store| match previous {
                Some(previous) => store.upsert_plugin_binding(&previous),
                None => store.delete_plugin_binding(&target, &binding_id),
            })
        },
        |result| result.is_valid(),
    )
    .await?;
    Handle::refresh_clash();
    Ok(())
}

#[tauri::command]
pub async fn clash5_reset_identity() -> CmdResult {
    let _profile_guard = PROFILE_WRITE_LOCK.lock().await;
    crate::config::profiles::reset_clash5_identity()
        .await
        .map_err(failure)?;
    let applied = CoreManager::global().update_config_forced().await;
    if !applied.as_ref().is_ok_and(|outcome| outcome.is_valid()) {
        return Err(coded_error(
            "IDENTITY_RESET_RUNTIME_PENDING",
            "the new identity was saved but the running configuration could not be refreshed",
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn clash5_save_plugin_bindings(bindings: Vec<PluginBinding>) -> CmdResult {
    save_plugin_bindings(bindings).await
}

#[tauri::command]
pub async fn clash5_delete_plugin_binding(target: PluginBindingTarget, binding_id: String) -> CmdResult {
    let _profile_guard = PROFILE_WRITE_LOCK.lock().await;
    let previous = crate::config::profiles::with_clash5_binding_store({
        let target = target.clone();
        let binding_id = binding_id.clone();
        move |store| {
            let previous = store
                .plugin_bindings(Some(&target))?
                .into_iter()
                .find(|binding| binding.id == binding_id)
                .ok_or(DataError::NotFound)?;
            store.delete_plugin_binding(&target, &binding_id)?;
            Ok(previous)
        }
    })
    .await
    .map_err(failure)?;
    if previous.orphan {
        Handle::refresh_clash();
        return Ok(());
    }
    apply_plugin_change_with_rollback(
        || CoreManager::global().update_config_forced(),
        move || crate::config::profiles::with_clash5_binding_store(move |store| store.upsert_plugin_binding(&previous)),
        |result| result.is_valid(),
    )
    .await?;
    Handle::refresh_clash();
    Ok(())
}

#[tauri::command]
pub async fn clash5_set_plugin_binding_enabled(
    target: PluginBindingTarget,
    binding_id: String,
    enabled: bool,
) -> CmdResult {
    let _profile_guard = PROFILE_WRITE_LOCK.lock().await;
    let target_for_store = target.clone();
    let id_for_store = binding_id.clone();
    let previous = crate::config::profiles::with_clash5_binding_store(move |store| {
        let previous = store
            .plugin_bindings(Some(&target_for_store))?
            .into_iter()
            .find(|binding| binding.id == id_for_store)
            .ok_or(DataError::NotFound)?;
        if previous.orphan {
            return Err(DataError::InvalidInput);
        }
        let mut updated = previous.clone();
        updated.enabled = enabled;
        store.upsert_plugin_binding(&updated)?;
        Ok(previous)
    })
    .await
    .map_err(failure)?;
    apply_plugin_change_with_rollback(
        || CoreManager::global().update_config_forced(),
        move || crate::config::profiles::with_clash5_binding_store(move |store| store.upsert_plugin_binding(&previous)),
        |result| result.is_valid(),
    )
    .await?;
    Handle::refresh_clash();
    Ok(())
}

async fn save_plugin_bindings(mut bindings: Vec<PluginBinding>) -> CmdResult {
    if bindings.is_empty() || bindings.len() > 32 {
        return Err(coded_error("C5-PLUGIN-001", "plugin binding batch size is invalid"));
    }
    let _profile_guard = PROFILE_WRITE_LOCK.lock().await;
    let plugin_root = dirs::app_home_dir()
        .map_err(super::CommandFailure::plain)?
        .join("plugins");
    let plugins = tokio::task::spawn_blocking(move || plugins::installed_plugins(&plugin_root))
        .await
        .map_err(|_| coded_error("C5-PLUGIN-004", "installed plugins could not be checked"))?
        .map_err(plugin_failure)?;
    for binding in &mut bindings {
        if !plugins.iter().any(|plugin| plugin.id == binding.plugin_id) {
            return Err(coded_error("C5-PLUGIN-001", "plugin is not installed"));
        }
        if !binding.config.is_object() {
            return Err(coded_error(
                "C5-PLUGIN-001",
                "plugin binding settings must be an object",
            ));
        }
        let clear_config = serde_json::to_string(&binding.config)
            .map_err(|_| coded_error("C5-PLUGIN-001", "plugin binding settings are invalid"))?;
        let encrypted = crate::config::encrypt_data(&clear_config)
            .map_err(|_| coded_error("C5-PLUGIN-004", "plugin binding settings could not be protected"))?;
        binding.config = serde_json::json!({"__clash5_encrypted": encrypted});
    }
    let removals = bindings
        .iter()
        .map(|binding| (binding.target.clone(), binding.id.clone()))
        .collect::<Vec<_>>();
    let rollback_keys = removals.clone();
    let saved = crate::config::profiles::with_clash5_binding_store(move |store| {
        for (target, binding_id) in &removals {
            if store
                .plugin_bindings(Some(target))?
                .iter()
                .any(|existing| existing.id == *binding_id)
            {
                return Err(DataError::InvalidInput);
            }
        }
        store.upsert_plugin_bindings(&bindings)
    })
    .await;
    saved.map_err(failure)?;
    apply_plugin_change_with_rollback(
        || CoreManager::global().update_config_forced(),
        move || {
            crate::config::profiles::with_clash5_binding_store(move |store| {
                store.delete_plugin_bindings(&rollback_keys).map(|_| ())
            })
        },
        |result| result.is_valid(),
    )
    .await?;
    Handle::refresh_clash();
    Ok(())
}

#[derive(Serialize)]
pub struct PluginPackagePreview {
    staging_id: String,
    id: String,
    version: String,
    name: String,
    description: String,
    permissions: Vec<String>,
    signer_fingerprint: String,
    signer: String,
}

fn plugin_failure(error: PluginFailure) -> super::CommandFailure {
    coded_error(error.code(), "plugin package could not be verified or installed")
}

#[tauri::command]
pub async fn clash5_stage_plugin_package() -> CmdResult<Option<PluginPackagePreview>> {
    let path = tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .add_filter("clash5 plugin package", &["c5p"])
            .pick_file()
    })
    .await
    .map_err(|_| coded_error("C5-PLUGIN-004", "plugin package selection failed"))?;
    let Some(path) = path else {
        return Ok(None);
    };
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| coded_error("C5-PLUGIN-001", "plugin package could not be read"))?;
    if metadata.len() > 10 * 1024 * 1024 {
        return Err(coded_error("C5-PLUGIN-001", "plugin package is too large"));
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|_| coded_error("C5-PLUGIN-001", "plugin package could not be read"))?;
    let package = plugins::inspect_package(&bytes).map_err(plugin_failure)?;
    let root = dirs::app_home_dir()
        .map_err(super::CommandFailure::plain)?
        .join("plugins")
        .join(".staging");
    tokio::fs::create_dir_all(&root)
        .await
        .map_err(|_| coded_error("C5-PLUGIN-004", "plugin staging is unavailable"))?;
    let staging_id = nanoid::nanoid!();
    tokio::fs::write(root.join(format!("{staging_id}.c5p")), bytes)
        .await
        .map_err(|_| coded_error("C5-PLUGIN-004", "plugin package could not be staged"))?;
    let signer_fingerprint = sha2::Sha256::digest(package.signer.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(Some(PluginPackagePreview {
        staging_id,
        id: package.manifest.id,
        version: package.manifest.version,
        name: package.manifest.name,
        description: package.manifest.description,
        permissions: package.manifest.permissions,
        signer_fingerprint,
        signer: package.signer,
    }))
}

#[tauri::command]
pub async fn clash5_install_staged_plugin(
    staging_id: String,
    confirmed_signer: String,
) -> CmdResult<plugins::PluginSummary> {
    if staging_id.len() != 21
        || !staging_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        || confirmed_signer.len() > 1_024
    {
        return Err(coded_error("C5-PLUGIN-001", "staged plugin reference is invalid"));
    }
    let root = dirs::app_home_dir()
        .map_err(super::CommandFailure::plain)?
        .join("plugins");
    let staged = root.join(".staging").join(format!("{staging_id}.c5p"));
    let bytes = tokio::fs::read(&staged)
        .await
        .map_err(|_| coded_error("C5-PLUGIN-001", "staged plugin package is unavailable"))?;
    let package = plugins::verify_package(&bytes, &[confirmed_signer]).map_err(plugin_failure)?;
    let audit_details = serde_json::json!({
        "plugin_id": package.manifest.id.clone(),
        "version": package.manifest.version.clone(),
        "permissions": package.manifest.permissions.clone(),
        "signer_fingerprint": sha2::Sha256::digest(package.signer.as_bytes())
            .iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
    });
    let plugin_root = root.clone();
    let installed = tokio::task::spawn_blocking(move || plugins::install_package(package, &plugin_root))
        .await
        .map_err(|_| coded_error("C5-PLUGIN-004", "plugin installation failed"))?
        .map_err(plugin_failure)?;
    let _ = tokio::fs::remove_file(staged).await;
    let audit_result = crate::config::profiles::with_clash5_binding_store(move |store| {
        store.record_plugin_audit("plugin.install", &audit_details)
    })
    .await;
    if audit_result.is_err() {
        tracing::warn!(plugin_id = %installed.id, "plugin installation audit event could not be persisted");
    }
    Ok(installed)
}

#[tauri::command]
pub async fn clash5_list_plugin_audit_events(limit: Option<usize>) -> CmdResult<Vec<PluginAuditEvent>> {
    crate::config::profiles::with_clash5_binding_store(move |store| store.plugin_audit_events(limit.unwrap_or(50)))
        .await
        .map_err(failure)
}

#[tauri::command]
pub async fn clash5_list_plugins() -> CmdResult<Vec<plugins::PluginSummary>> {
    let root = dirs::app_home_dir()
        .map_err(super::CommandFailure::plain)?
        .join("plugins");
    tokio::task::spawn_blocking(move || plugins::installed_plugins(&root))
        .await
        .map_err(|_| coded_error("C5-PLUGIN-004", "installed plugins could not be listed"))?
        .map_err(plugin_failure)
}

#[derive(Serialize)]
pub struct PluginBindingSummary {
    id: String,
    plugin_id: String,
    target: PluginBindingTarget,
    priority: i64,
    enabled: bool,
    orphan: bool,
    disappeared_at: Option<String>,
}

impl From<PluginBinding> for PluginBindingSummary {
    fn from(binding: PluginBinding) -> Self {
        Self {
            id: binding.id,
            plugin_id: binding.plugin_id,
            target: binding.target,
            priority: binding.priority,
            enabled: binding.enabled,
            orphan: binding.orphan,
            disappeared_at: binding.disappeared_at,
        }
    }
}

#[tauri::command]
pub async fn clash5_rotate_identity() -> CmdResult {
    let _profile_guard = PROFILE_WRITE_LOCK.lock().await;
    let old_key = crate::config::profiles::clash5_identity_key().await.map_err(failure)?;
    let mut snapshots = Vec::<clash5_data::IdentitySourceSnapshot>::new();

    let profiles = Config::profiles().await.latest_arc();
    if let Some(profile_id) = profiles.current.clone() {
        let mapping = profiles
            .current_mapping()
            .await
            .map_err(|_| coded_error("IDENTITY_ROTATION_FAILED", "the current profile could not be read"))?;
        let yaml = serde_yaml_ng::to_string(&mapping)
            .map_err(|_| coded_error("IDENTITY_ROTATION_FAILED", "the current profile could not be parsed"))?;
        if let Some(nodes) = parse_inline_nodes(&yaml).map_err(|_| {
            coded_error(
                "IDENTITY_ROTATION_FAILED",
                "the current profile contains invalid node data",
            )
        })? {
            snapshots.push(clash5_data::IdentitySourceSnapshot {
                source: Source::Subscription {
                    provider_id: profile_id.to_string(),
                },
                nodes,
            });
        }

        let runtime = Config::runtime().await;
        let applied = runtime.data_arc();
        if let Some(config) = applied.config.as_ref() {
            let app_dir = dirs::app_home_dir().map_err(super::CommandFailure::plain)?;
            let providers = remote_providers_of(config, &app_dir).map_err(super::CommandFailure::plain)?;
            for provider in providers
                .into_iter()
                .filter(|provider| provider.section == "proxy-providers")
            {
                let path = app_dir.join(&provider.provider.destination);
                let yaml = match tokio::fs::read_to_string(path).await {
                    Ok(yaml) => yaml,
                    Err(_) => continue,
                };
                let Some(nodes) = parse_inline_nodes(&yaml).map_err(|_| {
                    coded_error(
                        "IDENTITY_ROTATION_FAILED",
                        "a proxy provider contains invalid node data",
                    )
                })?
                else {
                    continue;
                };
                snapshots.push(clash5_data::IdentitySourceSnapshot {
                    source: Source::Subscription {
                        provider_id: proxy_provider_source_id(&profile_id, &provider.name),
                    },
                    nodes,
                });
            }
        }
    }

    let mut manual_file = manual::load()
        .await
        .map_err(|_| coded_error("IDENTITY_ROTATION_FAILED", "manual proxies could not be read"))?;
    let manual_nodes = manual_file
        .identity_nodes(&old_key)
        .map_err(|_| coded_error("IDENTITY_ROTATION_FAILED", "manual proxy identity data is invalid"))?;
    let has_manual_proxies = !manual_nodes.is_empty();
    snapshots.push(clash5_data::IdentitySourceSnapshot {
        source: Source::Manual,
        nodes: manual_nodes,
    });

    let (generation, new_key) = crate::config::profiles::stage_clash5_identity_rotation_key()
        .await
        .map_err(failure)?;
    let previous = match tokio::fs::read_to_string(manual::path().map_err(super::CommandFailure::plain)?).await {
        Ok(yaml) => Some(yaml),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => {
            return Err(coded_error(
                "IDENTITY_ROTATION_FAILED",
                "manual proxy data could not be backed up",
            ));
        }
    };
    if has_manual_proxies {
        manual_file
            .rekey_identity(&old_key, &new_key)
            .map_err(|_| coded_error("IDENTITY_ROTATION_FAILED", "manual proxies could not be rekeyed"))?;
        if manual::save(&manual_file).await.is_err() {
            manual::restore(previous.as_deref()).await.map_err(|_| {
                coded_error(
                    "IDENTITY_ROTATION_ROLLBACK_FAILED",
                    "manual proxies could not be restored",
                )
            })?;
            return Err(coded_error(
                "IDENTITY_ROTATION_FAILED",
                "manual proxies could not be saved",
            ));
        }
        let applied = CoreManager::global().update_config_forced().await;
        if !applied.as_ref().is_ok_and(|outcome| outcome.is_valid()) {
            restore_manual_file(previous.as_deref()).await?;
            return Err(coded_error(
                "IDENTITY_ROTATION_FAILED",
                "the rekeyed manual proxies were rejected",
            ));
        }
    }

    if let Err(error) =
        crate::config::profiles::commit_clash5_identity_rotation(old_key, generation, new_key, snapshots).await
    {
        if has_manual_proxies {
            restore_manual_file(previous.as_deref()).await?;
        }
        return Err(failure(error));
    }
    Ok(())
}
#[tauri::command]
pub async fn clash5_list_nodes(include_inactive: Option<bool>) -> CmdResult<Vec<NodeRecord>> {
    let recovery_mode = crate::config::profiles::with_clash5_binding_store(|store| Ok(store.identity_recovery()))
        .await
        .map_err(failure)?;
    if !recovery_mode {
        refresh_runtime_proxy_providers().await?;
    }
    crate::config::profiles::with_clash5_binding_store(move |store| {
        store.node_records(include_inactive.unwrap_or(false))
    })
    .await
    .map_err(failure)
}

fn proxy_provider_source_id(profile_id: &str, provider_name: &str) -> String {
    format!("proxy-provider:{profile_id}:{provider_name}")
}

fn stale_runtime_provider_sources(
    records: Vec<NodeRecord>,
    profile_id: &str,
    declared: &HashSet<String>,
) -> HashSet<String> {
    let current_prefix = format!("proxy-provider:{profile_id}:");
    records
        .into_iter()
        .filter_map(|record| match record.source {
            Source::Subscription { provider_id }
                if provider_id.starts_with(&current_prefix) && !declared.contains(&provider_id) =>
            {
                Some(provider_id)
            }
            _ => None,
        })
        .collect()
}

async fn refresh_runtime_proxy_providers() -> CmdResult {
    let Some(profile_id) = Config::profiles().await.latest_arc().current.clone() else {
        return Ok(());
    };
    let runtime = Config::runtime().await;
    let applied = runtime.data_arc();
    let Some(config) = applied.config.as_ref() else {
        return Ok(());
    };
    let app_dir = dirs::app_home_dir().map_err(super::CommandFailure::plain)?;
    let providers = remote_providers_of(config, &app_dir).map_err(super::CommandFailure::plain)?;
    let declared: HashSet<_> = providers
        .iter()
        .filter(|provider| provider.section == "proxy-providers")
        .map(|provider| proxy_provider_source_id(&profile_id, &provider.name))
        .collect();
    let mut snapshots = Vec::new();
    for provider in providers
        .into_iter()
        .filter(|provider| provider.section == "proxy-providers")
    {
        let path = app_dir.join(&provider.provider.destination);
        let Ok(yaml) = tokio::fs::read_to_string(path).await else {
            continue;
        };
        let Ok(Some(nodes)) = parse_inline_nodes(&yaml) else {
            continue;
        };
        snapshots.push((proxy_provider_source_id(&profile_id, &provider.name), nodes));
    }
    crate::config::profiles::with_clash5_binding_store(move |store| {
        let stale = stale_runtime_provider_sources(store.node_records(true)?, &profile_id, &declared);
        for source_id in stale {
            store.refresh_provider(&source_id, &[])?;
        }
        for (source_id, nodes) in &snapshots {
            store.refresh_provider(source_id, nodes)?;
        }
        Ok(())
    })
    .await
    .map_err(failure)
}

#[derive(Serialize)]
pub struct ProbeBatch {
    pub probed: usize,
    pub cached: usize,
    pub unavailable: usize,
    pub failed: usize,
}

enum RuntimeProbeTarget {
    Core(String),
    Provider { provider_name: String, proxy_name: String },
}

async fn measure(target: &RuntimeProbeTarget, url: &str, timeout_ms: u32) -> Option<u32> {
    let api = Handle::mihomo();
    let request = async {
        match target {
            RuntimeProbeTarget::Core(name) => api.delay_proxy_by_name(name, url, timeout_ms).await,
            RuntimeProbeTarget::Provider {
                provider_name,
                proxy_name,
            } => {
                api.healthcheck_node_in_provider(provider_name, proxy_name, url, timeout_ms)
                    .await
            }
        }
    };
    tokio::time::timeout(Duration::from_millis(u64::from(timeout_ms) + 1_500), request)
        .await
        .ok()?
        .ok()
        .and_then(|result| (result.delay > 0).then_some(result.delay))
}

async fn probe_targets<F, Fut>(
    targets: Vec<(NodeRecord, RuntimeProbeTarget)>,
    now_ms: u64,
    test: F,
) -> Vec<(Source, String, NodeQuality)>
where
    F: Fn(RuntimeProbeTarget) -> Fut + Clone + Send + Sync,
    Fut: Future<Output = (Option<u32>, Option<u32>)> + Send,
{
    stream::iter(targets.into_iter().map(|(record, target)| {
        let test = test.clone();
        async move {
            let (short, handshake) = test(target).await;
            let quality = NodeQuality::from_probe_after(short, handshake, now_ms, record.quality.as_ref());
            (record.source, record.stable_id, quality)
        }
    }))
    .buffer_unordered(PROBE_CONCURRENCY)
    .collect()
    .await
}

#[tauri::command]
pub async fn clash5_probe_nodes() -> CmdResult<ProbeBatch> {
    let _guard = PROBE_LOCK
        .try_lock()
        .map_err(|_| super::coded_error("PROBE_BUSY", "a node probe is already running"))?;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| super::coded_error("PROBE_CLOCK", "system clock is unavailable"))?
        .as_millis() as u64;
    refresh_runtime_proxy_providers().await?;
    let records = crate::config::profiles::with_clash5_binding_store(|store| store.node_records(false))
        .await
        .map_err(failure)?;
    let current = Config::profiles().await.latest_arc().current.clone();
    let view = super::proxy::get_proxy_view().await?;
    let mut name_count = HashMap::new();
    let mut core_names = std::collections::HashSet::new();
    for node in view.records.values() {
        *name_count.entry(node.name.clone()).or_insert(0usize) += 1;
        if matches!(node.source, ProxyNodeSource::Core { .. }) {
            core_names.insert(node.name.clone());
        }
    }
    let mut batch = ProbeBatch {
        probed: 0,
        cached: 0,
        unavailable: 0,
        failed: 0,
    };
    let mut targets = Vec::new();
    let provider_prefix = format!("proxy-provider:{}:", current.as_deref().unwrap_or_default());
    for record in records {
        let target = match &record.source {
            Source::Subscription { provider_id } if current.as_deref() == Some(provider_id.as_str()) => core_names
                .contains(&record.display_name)
                .then(|| RuntimeProbeTarget::Core(record.display_name.clone())),
            Source::Subscription { provider_id } => {
                provider_id.strip_prefix(&provider_prefix).and_then(|provider_name| {
                    let matches = view
                        .records
                        .values()
                        .filter(|node| {
                            matches!(&node.source, ProxyNodeSource::Provider { provider_name: owner, proxy_name }
                            if owner == provider_name && proxy_name == &record.display_name)
                        })
                        .count();
                    (matches == 1).then(|| RuntimeProbeTarget::Provider {
                        provider_name: provider_name.to_string(),
                        proxy_name: record.display_name.clone(),
                    })
                })
            }
            Source::Manual => {
                let name = manual::runtime_name(&record.stable_id);
                core_names.contains(&name).then_some(RuntimeProbeTarget::Core(name))
            }
        };
        let Some(target) = target else {
            batch.unavailable += 1;
            continue;
        };
        if matches!(&target, RuntimeProbeTarget::Core(name) if name_count.get(name) != Some(&1)) {
            batch.unavailable += 1;
            continue;
        }
        if record.quality.as_ref().is_some_and(|quality| quality.is_fresh(now_ms)) {
            batch.cached += 1;
            continue;
        }
        targets.push((record, target));
    }
    let results = probe_targets(targets, now_ms, |target| async move {
        let short = measure(&target, SHORT_PROBE_URL, SHORT_PROBE_MS).await;
        let handshake = measure(&target, HANDSHAKE_PROBE_URL, HANDSHAKE_PROBE_MS).await;
        (short, handshake)
    })
    .await;
    batch.probed = results.len();
    batch.failed = results
        .iter()
        .filter(|(_, _, quality)| quality.status == QualityStatus::Failed)
        .count();
    crate::config::profiles::with_clash5_binding_store(move |store| {
        for (source, stable_id, quality) in &results {
            store.save_quality(source, stable_id, quality)?;
        }
        Ok(())
    })
    .await
    .map_err(failure)?;
    Ok(batch)
}

#[tauri::command]
pub async fn clash5_purge_orphans(source: Option<Source>) -> CmdResult<usize> {
    crate::config::profiles::with_clash5_binding_store(move |store| store.purge_orphans(source.as_ref()))
        .await
        .map_err(failure)
}

#[tauri::command]
pub async fn clash5_orphan_cleanup_preview() -> CmdResult<clash5_data::OrphanCleanupPreview> {
    crate::config::profiles::with_clash5_binding_store(|store| store.orphan_cleanup_preview())
        .await
        .map_err(failure)
}

#[tauri::command]
pub async fn clash5_set_orphan_retention_days(days: u16) -> CmdResult<()> {
    crate::config::profiles::with_clash5_binding_store(move |store| store.set_orphan_retention_days(days))
        .await
        .map_err(failure)
}

#[tauri::command]
pub async fn clash5_purge_expired_orphans() -> CmdResult<usize> {
    crate::config::profiles::with_clash5_binding_store(|store| store.purge_expired_orphans(None))
        .await
        .map_err(failure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    fn node(stable_id: &str) -> NodeRecord {
        NodeRecord {
            stable_id: stable_id.into(),
            source: Source::Manual,
            display_name: stable_id.into(),
            kind: "ss".into(),
            server: "fixture.example.invalid".into(),
            port: 443,
            active: true,
            duplicate_count: 1,
            quality: None,
        }
    }

    #[test]
    fn switching_profiles_only_retires_provider_sources_owned_by_the_current_profile() {
        let source = |provider_id: &str| {
            let mut record = node(provider_id);
            record.source = Source::Subscription {
                provider_id: provider_id.into(),
            };
            record
        };
        let declared = HashSet::from(["proxy-provider:profile-b:still-present".into()]);
        let stale = stale_runtime_provider_sources(
            vec![
                source("proxy-provider:profile-a:old"),
                source("proxy-provider:profile-b:still-present"),
                source("proxy-provider:profile-b:removed"),
                node("manual"),
            ],
            "profile-b",
            &declared,
        );
        assert_eq!(stale, HashSet::from(["proxy-provider:profile-b:removed".into()]));
    }

    #[tokio::test]
    async fn one_failed_probe_does_not_abort_other_nodes() {
        let mut results = probe_targets(
            vec![
                (node("failed"), RuntimeProbeTarget::Core("failed".into())),
                (node("healthy"), RuntimeProbeTarget::Core("healthy".into())),
            ],
            1_000,
            |target| async move {
                if matches!(target, RuntimeProbeTarget::Core(name) if name == "failed") {
                    (None, None)
                } else {
                    (Some(45), Some(120))
                }
            },
        )
        .await;
        results.sort_by(|left, right| left.1.cmp(&right.1));
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].2.status, QualityStatus::Failed);
        assert_eq!(results[1].2.status, QualityStatus::Healthy);
    }

    #[tokio::test]
    async fn probe_batch_never_exceeds_the_parallel_limit() {
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let targets = (0..PROBE_CONCURRENCY * 3)
            .map(|index| {
                (
                    node(&format!("node-{index}")),
                    RuntimeProbeTarget::Core(format!("runtime-{index}")),
                )
            })
            .collect();
        let results = probe_targets(targets, 1_000, {
            let active = Arc::clone(&active);
            let peak = Arc::clone(&peak);
            move |_| {
                let active = Arc::clone(&active);
                let peak = Arc::clone(&peak);
                async move {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(current, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    active.fetch_sub(1, Ordering::SeqCst);
                    (Some(45), Some(120))
                }
            }
        })
        .await;
        assert_eq!(results.len(), PROBE_CONCURRENCY * 3);
        assert!(peak.load(Ordering::SeqCst) > 1);
        assert!(peak.load(Ordering::SeqCst) <= PROBE_CONCURRENCY);
    }

    #[test]
    fn effective_configuration_redacts_credentials_and_provider_urls() {
        let mut config = serde_json::json!({
            "proxies": [{"name": "edge", "server": "edge.example.invalid", "password": "secret", "uuid": "private-id"}],
            "proxy-providers": {"remote": {"url": "https://user:pass@example.invalid/sub?token=secret", "path": "providers/remote.yaml"}}
        });

        redact_plugin_config(&mut config, None);

        assert_eq!(config["proxies"][0]["server"], "edge.example.invalid");
        assert_eq!(config["proxies"][0]["password"], "[redacted]");
        assert_eq!(config["proxies"][0]["uuid"], "[redacted]");
        assert_eq!(config["proxy-providers"]["remote"]["url"], "[redacted]");
        assert_eq!(config["proxy-providers"]["remote"]["path"], "providers/remote.yaml");
    }

    #[tokio::test]
    async fn rejected_plugin_update_restores_binding_and_previous_runtime() {
        let update_count = Arc::new(AtomicUsize::new(0));
        let restore_count = Arc::new(AtomicUsize::new(0));
        let result = apply_plugin_change_with_rollback(
            {
                let update_count = Arc::clone(&update_count);
                move || {
                    let attempt = update_count.fetch_add(1, Ordering::SeqCst);
                    std::future::ready(Ok(attempt > 0))
                }
            },
            {
                let restore_count = Arc::clone(&restore_count);
                move || {
                    restore_count.fetch_add(1, Ordering::SeqCst);
                    std::future::ready(Ok(()))
                }
            },
            |valid| *valid,
        )
        .await;

        assert!(result.is_err());
        let Some(error) = result.err() else {
            return;
        };
        assert_eq!(error.code.as_deref(), Some("C5-PLUGIN-005"));
        assert!(error.detail.contains("previous binding was restored"));
        assert_eq!(restore_count.load(Ordering::SeqCst), 1);
        assert_eq!(update_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn failed_plugin_rollback_reports_recovery_error_without_claiming_success() {
        let update_count = Arc::new(AtomicUsize::new(0));
        let result = apply_plugin_change_with_rollback(
            {
                let update_count = Arc::clone(&update_count);
                move || {
                    update_count.fetch_add(1, Ordering::SeqCst);
                    std::future::ready(Ok(false))
                }
            },
            || std::future::ready(Err(DataError::Database)),
            |valid| *valid,
        )
        .await;

        assert!(result.is_err());
        let Some(error) = result.err() else {
            return;
        };
        assert_eq!(error.code.as_deref(), Some("C5-PLUGIN-004"));
        assert_eq!(update_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_previous_runtime_reload_reports_recovery_error() {
        let update_count = Arc::new(AtomicUsize::new(0));
        let restore_count = Arc::new(AtomicUsize::new(0));
        let result = apply_plugin_change_with_rollback(
            {
                let update_count = Arc::clone(&update_count);
                move || {
                    update_count.fetch_add(1, Ordering::SeqCst);
                    std::future::ready(Ok(false))
                }
            },
            {
                let restore_count = Arc::clone(&restore_count);
                move || {
                    restore_count.fetch_add(1, Ordering::SeqCst);
                    std::future::ready(Ok(()))
                }
            },
            |valid| *valid,
        )
        .await;

        assert!(result.is_err());
        let Some(error) = result.err() else {
            return;
        };
        assert_eq!(error.code.as_deref(), Some("C5-PLUGIN-004"));
        assert_eq!(restore_count.load(Ordering::SeqCst), 1);
        assert_eq!(update_count.load(Ordering::SeqCst), 2);
    }
}
