use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use serde_yaml_ng::{Mapping, Value};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{Cursor, Read as _},
    path::Path,
    sync::OnceLock,
    thread,
    time::{Duration, Instant},
};

#[derive(serde::Serialize)]
pub(crate) struct PluginApplication {
    pub provenance: BTreeMap<String, clash5_data::ConfigProvenance>,
    pub conflicts: Vec<clash5_data::PluginConflict>,
}

async fn record_plugin_audit(event: &str, details: serde_json::Value) {
    let event_name = event.to_owned();
    let event_for_log = event_name.clone();
    let result = crate::config::profiles::with_clash5_binding_store(move |store| {
        store.record_plugin_audit(&event_name, &details)
    })
    .await;
    if result.is_err() {
        tracing::warn!(event = %event_for_log, "plugin audit event could not be persisted");
    }
}
use wasmtime::{Config, Engine, Linker, Module, OptLevel, Store, StoreLimits, StoreLimitsBuilder, Trap};
use zip::ZipArchive;

const MAX_MODULE_BYTES: usize = 8 * 1024 * 1024;
const MAX_INPUT_BYTES: usize = 1024 * 1024;
const MAX_PATCH_BYTES: usize = 1024 * 1024;
const MAX_PACKAGE_BYTES: usize = 10 * 1024 * 1024;
const MAX_MANIFEST_BYTES: usize = 64 * 1024;
const MAX_MEMORY_BYTES: usize = 32 * 1024 * 1024;
const MAX_ACTIVE_BINDINGS: usize = 32;
const EPOCH_INTERVAL: Duration = Duration::from_millis(100);
const EPOCH_DEADLINE_TICKS: u64 = 30;
const FUEL_LIMIT: u64 = 10_000_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PluginManifest {
    pub schema_version: u16,
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    pub abi_version: u16,
    pub permissions: Vec<String>,
    pub writable_paths: Vec<String>,
    pub module_sha256: String,
}

pub(crate) struct VerifiedPluginPackage {
    pub manifest: PluginManifest,
    pub module: Vec<u8>,
    pub signer: String,
    pub signature: String,
    pub manifest_bytes: Vec<u8>,
}

#[derive(Serialize)]
pub(crate) struct PluginSummary {
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    pub permissions: Vec<String>,
    pub writable_paths: Vec<String>,
    pub signer_fingerprint: String,
}

#[derive(Serialize)]
pub(crate) struct PluginInvocation<'a> {
    pub configuration: &'a JsonValue,
    pub binding: &'a JsonValue,
}

pub(crate) async fn apply_bound_plugins(
    config: &mut Mapping,
    profile_id: Option<&str>,
    active_chain_ids: &HashSet<String>,
) -> anyhow::Result<PluginApplication> {
    let (bindings, nodes) = crate::config::profiles::with_clash5_binding_store(|store| {
        Ok((store.plugin_bindings(None)?, store.node_records(false)?))
    })
    .await?;
    let applicable = bindings
        .into_iter()
        .filter(|binding| {
            binding.enabled
                && !binding.orphan
                && match &binding.target {
                    clash5_data::PluginBindingTarget::Chain { chain_id } => {
                        active_chain_ids.contains(chain_id.as_str())
                    }
                    clash5_data::PluginBindingTarget::Node { source, stable_id } => nodes.iter().any(|node| {
                        node.active
                            && &node.source == source
                            && &node.stable_id == stable_id
                            && match source {
                                clash5_data::Source::Manual => true,
                                clash5_data::Source::Subscription { provider_id } => {
                                    profile_id.is_some_and(|profile| {
                                        provider_id == profile
                                            || provider_id.starts_with(&format!("proxy-provider:{profile}:"))
                                    })
                                }
                            }
                    }),
                }
        })
        .collect::<Vec<_>>();
    if applicable.len() > MAX_ACTIVE_BINDINGS {
        anyhow::bail!("too many active plugin bindings");
    }

    let base = serde_json::to_value(&*config)?;
    let plugin_root = crate::utils::dirs::app_home_dir()?.join("plugins");
    let mut patches = Vec::with_capacity(applicable.len());
    for binding in applicable {
        let audit_identity = serde_json::json!({
            "plugin_id": binding.plugin_id.clone(),
            "binding_id": binding.id.clone(),
        });
        let (manifest, module) = match load_installed_plugin(&plugin_root, &binding.plugin_id) {
            Ok(plugin) => plugin,
            Err(error) => {
                let mut details = audit_identity;
                details["result"] = JsonValue::String("failure".into());
                details["code"] = JsonValue::String(error.code().into());
                record_plugin_audit("plugin.failure", details).await;
                return Err(anyhow::anyhow!(error.code()));
            }
        };
        if !manifest
            .permissions
            .iter()
            .any(|permission| permission == "config.patch")
        {
            anyhow::bail!("plugin binding does not grant config.patch");
        }
        let visible_config = if manifest
            .permissions
            .iter()
            .any(|permission| permission == "config.read")
        {
            base.clone()
        } else {
            JsonValue::Object(Default::default())
        };
        let binding_config = match binding.config.get("__clash5_encrypted").and_then(JsonValue::as_str) {
            Some(encrypted) => {
                let clear =
                    crate::config::decrypt_data(encrypted).map_err(|error| anyhow::anyhow!(error.to_string()))?;
                serde_json::from_str(&clear)?
            }
            None => binding.config,
        };
        let patch = execute(
            module,
            manifest.clone(),
            PluginInvocation {
                configuration: &visible_config,
                binding: &binding_config,
            },
        )
        .await;
        let patch = match patch {
            Ok(patch) => patch,
            Err(error) => {
                let mut details = audit_identity;
                details["result"] = JsonValue::String("failure".into());
                details["code"] = JsonValue::String(error.code().into());
                record_plugin_audit("plugin.failure", details).await;
                return Err(anyhow::anyhow!(error.code()));
            }
        };
        let mut details = audit_identity;
        details["result"] = JsonValue::String("success".into());
        details["permissions"] = serde_json::json!(manifest.permissions.clone());
        record_plugin_audit(
            "plugin.permission.use",
            serde_json::json!({
                "plugin_id": binding.plugin_id.clone(),
                "binding_id": binding.id.clone(),
                "permissions": manifest.permissions,
            }),
        )
        .await;
        record_plugin_audit("plugin.execute", details).await;
        patches.push(clash5_data::PluginPatch {
            binding_id: binding.id,
            plugin_id: binding.plugin_id,
            priority: binding.priority,
            writable_paths: manifest.writable_paths,
            patch,
        });
    }

    let effective = clash5_data::merge_plugin_patches(&base, &patches)?;
    let yaml = serde_json::to_string(&effective.config)?;
    let effective_yaml: Value = serde_yaml_ng::from_str(&yaml)?;
    *config = effective_yaml
        .as_mapping()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("effective plugin config must remain a mapping"))?;
    Ok(PluginApplication {
        provenance: effective.provenance,
        conflicts: effective.conflicts,
    })
}

fn load_installed_plugin(plugin_root: &Path, id: &str) -> Result<(PluginManifest, Vec<u8>), PluginFailure> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(PluginFailure::InvalidPackage);
    }
    let directory = plugin_root.join(id);
    let metadata = fs::symlink_metadata(&directory).map_err(|_| PluginFailure::InvalidPackage)?;
    if !metadata.file_type().is_dir() {
        return Err(PluginFailure::InvalidPackage);
    }
    let manifest_bytes = read_installed_file(&directory, "manifest.json", MAX_MANIFEST_BYTES)?;
    let module = read_installed_file(&directory, "module.wasm", MAX_MODULE_BYTES)?;
    let signature = read_installed_file(&directory, "manifest.minisig", 4 * 1024)?;
    let signer = read_installed_file(&directory, "signer.pub", 1024)?;
    let manifest: PluginManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|_| PluginFailure::InvalidPackage)?;
    validate_manifest(&manifest)?;
    if manifest.id != id || hex_digest(&module) != manifest.module_sha256.to_ascii_lowercase() {
        return Err(PluginFailure::InvalidPackage);
    }
    verify_manifest_signature(
        &manifest_bytes,
        std::str::from_utf8(&signature).map_err(|_| PluginFailure::InvalidPackage)?,
        std::str::from_utf8(&signer)
            .map_err(|_| PluginFailure::InvalidPackage)?
            .trim(),
    )?;
    Ok((manifest, module))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PluginFailure {
    InvalidPackage,
    PermissionDenied,
    TimedOut,
    ExecutionFailed,
    InvalidPatch,
}

impl PluginFailure {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::InvalidPackage => "C5-PLUGIN-001",
            Self::PermissionDenied => "C5-PLUGIN-002",
            Self::TimedOut => "C5-PLUGIN-003",
            Self::ExecutionFailed => "C5-PLUGIN-004",
            Self::InvalidPatch => "C5-PLUGIN-005",
        }
    }
}

struct HostState {
    limits: StoreLimits,
}

static ENGINE: OnceLock<Result<Engine, String>> = OnceLock::new();
static EPOCH_TICKER: OnceLock<()> = OnceLock::new();

fn engine() -> Result<&'static Engine, PluginFailure> {
    let engine = ENGINE
        .get_or_init(|| {
            let mut config = Config::new();
            config
                .consume_fuel(true)
                .epoch_interruption(true)
                .cranelift_opt_level(OptLevel::None)
                .max_wasm_stack(512 * 1024);
            Engine::new(&config).map_err(|_| "WASM engine initialization failed".to_owned())
        })
        .as_ref()
        .map_err(|_| PluginFailure::ExecutionFailed)?;

    EPOCH_TICKER.get_or_init(|| {
        let engine = engine.clone();
        let _ = thread::Builder::new().name("clash5-wasm-epoch".into()).spawn(move || {
            loop {
                thread::sleep(EPOCH_INTERVAL);
                engine.increment_epoch();
            }
        });
    });
    Ok(engine)
}

pub(crate) fn validate_manifest(manifest: &PluginManifest) -> Result<(), PluginFailure> {
    let id_valid = !manifest.id.is_empty()
        && manifest.id.len() <= 64
        && manifest
            .id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-'));
    let version_valid = !manifest.version.is_empty()
        && manifest.version.len() <= 64
        && manifest
            .version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-'));
    if manifest.schema_version != 1
        || manifest.abi_version != 1
        || !id_valid
        || !version_valid
        || manifest.name.trim().is_empty()
        || manifest.name.len() > 128
        || manifest.description.len() > 2_048
        || manifest.module_sha256.len() != 64
        || !manifest.module_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        || manifest
            .permissions
            .iter()
            .any(|permission| !matches!(permission.as_str(), "config.read" | "config.patch"))
        || !manifest
            .permissions
            .iter()
            .any(|permission| permission == "config.patch")
        || clash5_data::validate_writable_paths(&manifest.writable_paths).is_err()
        || manifest
            .writable_paths
            .iter()
            .any(|path| path == "/rules" || path.starts_with("/rules/"))
    {
        return Err(PluginFailure::InvalidPackage);
    }
    Ok(())
}

pub(crate) fn verify_package(package: &[u8], trusted_keys: &[String]) -> Result<VerifiedPluginPackage, PluginFailure> {
    let package = inspect_package(package)?;
    if !trusted_keys.iter().any(|trusted| trusted.trim() == package.signer) {
        return Err(PluginFailure::PermissionDenied);
    }
    Ok(package)
}

pub(crate) fn inspect_package(package: &[u8]) -> Result<VerifiedPluginPackage, PluginFailure> {
    if package.is_empty() || package.len() > MAX_PACKAGE_BYTES {
        return Err(PluginFailure::InvalidPackage);
    }
    let mut archive = ZipArchive::new(Cursor::new(package)).map_err(|_| PluginFailure::InvalidPackage)?;
    let required = ["manifest.json", "module.wasm", "manifest.minisig", "signer.pub"];
    let names = archive
        .file_names()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| PluginFailure::InvalidPackage)?;
    if names.len() != required.len()
        || names.iter().any(|name| !required.contains(&name.as_ref()))
        || required
            .iter()
            .any(|required| names.iter().filter(|name| name.as_ref() == *required).count() != 1)
    {
        return Err(PluginFailure::InvalidPackage);
    }
    let manifest_bytes = read_package_entry(&mut archive, "manifest.json", MAX_MANIFEST_BYTES)?;
    let module = read_package_entry(&mut archive, "module.wasm", MAX_MODULE_BYTES)?;
    let signature = read_package_entry(&mut archive, "manifest.minisig", 4 * 1024)?;
    let signer = read_package_entry(&mut archive, "signer.pub", 1024)?;
    let manifest: PluginManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|_| PluginFailure::InvalidPackage)?;
    validate_manifest(&manifest)?;

    let signer = std::str::from_utf8(&signer)
        .map_err(|_| PluginFailure::InvalidPackage)?
        .trim()
        .to_owned();
    let signature = std::str::from_utf8(&signature).map_err(|_| PluginFailure::InvalidPackage)?;
    verify_manifest_signature(&manifest_bytes, signature, &signer)?;
    let digest = hex_digest(&module);
    if digest != manifest.module_sha256.to_ascii_lowercase() {
        return Err(PluginFailure::InvalidPackage);
    }

    Ok(VerifiedPluginPackage {
        manifest,
        module,
        signer,
        signature: signature.to_owned(),
        manifest_bytes,
    })
}

pub(crate) fn install_package(
    package: VerifiedPluginPackage,
    plugin_root: &Path,
) -> Result<PluginSummary, PluginFailure> {
    fs::create_dir_all(plugin_root).map_err(|_| PluginFailure::ExecutionFailed)?;
    let id = &package.manifest.id;
    let destination = plugin_root.join(id);
    let suffix = nanoid::nanoid!();
    let temporary = plugin_root.join(format!(".{id}.{suffix}.tmp"));
    let backup = plugin_root.join(format!(".{id}.{suffix}.backup"));
    fs::create_dir(&temporary).map_err(|_| PluginFailure::ExecutionFailed)?;
    let write_result = (|| {
        fs::write(temporary.join("manifest.json"), &package.manifest_bytes)?;
        fs::write(temporary.join("module.wasm"), &package.module)?;
        fs::write(temporary.join("manifest.minisig"), &package.signature)?;
        fs::write(temporary.join("signer.pub"), &package.signer)?;
        Ok::<(), std::io::Error>(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
        return Err(PluginFailure::ExecutionFailed);
    }

    let existing = match fs::symlink_metadata(&destination) {
        Ok(metadata) if metadata.file_type().is_dir() => true,
        Ok(_) => {
            let _ = fs::remove_dir_all(&temporary);
            return Err(PluginFailure::ExecutionFailed);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => {
            let _ = fs::remove_dir_all(&temporary);
            return Err(PluginFailure::ExecutionFailed);
        }
    };
    if existing && fs::rename(&destination, &backup).is_err() {
        let _ = fs::remove_dir_all(&temporary);
        return Err(PluginFailure::ExecutionFailed);
    }
    if fs::rename(&temporary, &destination).is_err() {
        if existing {
            let _ = fs::rename(&backup, &destination);
        }
        let _ = fs::remove_dir_all(&temporary);
        return Err(PluginFailure::ExecutionFailed);
    }
    if existing {
        let _ = fs::remove_dir_all(&backup);
    }
    Ok(summary(&package.manifest, &package.signer))
}

pub(crate) fn installed_plugins(plugin_root: &Path) -> Result<Vec<PluginSummary>, PluginFailure> {
    let entries = match fs::read_dir(plugin_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(PluginFailure::ExecutionFailed),
    };
    let mut plugins = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| PluginFailure::ExecutionFailed)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|_| PluginFailure::ExecutionFailed)?;
        if !metadata.file_type().is_dir() || entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let manifest_bytes = read_installed_file(&entry.path(), "manifest.json", MAX_MANIFEST_BYTES)?;
        let module = read_installed_file(&entry.path(), "module.wasm", MAX_MODULE_BYTES)?;
        let signature = read_installed_file(&entry.path(), "manifest.minisig", 4 * 1024)?;
        let signer = read_installed_file(&entry.path(), "signer.pub", 1024)?;
        let manifest: PluginManifest =
            serde_json::from_slice(&manifest_bytes).map_err(|_| PluginFailure::InvalidPackage)?;
        validate_manifest(&manifest)?;
        let signer = std::str::from_utf8(&signer)
            .map_err(|_| PluginFailure::InvalidPackage)?
            .trim();
        verify_manifest_signature(
            &manifest_bytes,
            std::str::from_utf8(&signature).map_err(|_| PluginFailure::InvalidPackage)?,
            signer,
        )?;
        if hex_digest(&module) != manifest.module_sha256.to_ascii_lowercase()
            || entry.file_name().to_string_lossy() != manifest.id
        {
            return Err(PluginFailure::InvalidPackage);
        }
        plugins.push(summary(&manifest, signer));
    }
    plugins.sort_by(|left, right| left.name.cmp(&right.name).then_with(|| left.id.cmp(&right.id)));
    Ok(plugins)
}

fn summary(manifest: &PluginManifest, signer: &str) -> PluginSummary {
    PluginSummary {
        id: manifest.id.clone(),
        version: manifest.version.clone(),
        name: manifest.name.clone(),
        description: manifest.description.clone(),
        permissions: manifest.permissions.clone(),
        writable_paths: manifest.writable_paths.clone(),
        signer_fingerprint: hex_digest(signer.as_bytes()),
    }
}

fn hex_digest(input: &[u8]) -> String {
    Sha256::digest(input).iter().map(|byte| format!("{byte:02x}")).collect()
}

fn read_installed_file(directory: &Path, name: &str, limit: usize) -> Result<Vec<u8>, PluginFailure> {
    let path = directory.join(name);
    let metadata = fs::symlink_metadata(&path).map_err(|_| PluginFailure::InvalidPackage)?;
    if !metadata.file_type().is_file() || metadata.len() > limit as u64 {
        return Err(PluginFailure::InvalidPackage);
    }
    fs::read(path).map_err(|_| PluginFailure::ExecutionFailed)
}

fn read_package_entry(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    name: &str,
    limit: usize,
) -> Result<Vec<u8>, PluginFailure> {
    let entry = archive.by_name(name).map_err(|_| PluginFailure::InvalidPackage)?;
    if entry.is_dir() || entry.size() > limit as u64 {
        return Err(PluginFailure::InvalidPackage);
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| PluginFailure::InvalidPackage)?;
    if bytes.len() > limit {
        return Err(PluginFailure::InvalidPackage);
    }
    Ok(bytes)
}

fn verify_manifest_signature(manifest: &[u8], signature: &str, signer: &str) -> Result<(), PluginFailure> {
    let public_key = minisign_verify::PublicKey::decode(signer).map_err(|_| PluginFailure::InvalidPackage)?;
    let signature = minisign_verify::Signature::decode(signature).map_err(|_| PluginFailure::InvalidPackage)?;
    public_key
        .verify(manifest, &signature, false)
        .map_err(|_| PluginFailure::InvalidPackage)
}

pub(crate) async fn execute(
    module_bytes: Vec<u8>,
    manifest: PluginManifest,
    invocation: PluginInvocation<'_>,
) -> Result<JsonValue, PluginFailure> {
    validate_manifest(&manifest)?;
    if module_bytes.is_empty() || module_bytes.len() > MAX_MODULE_BYTES {
        return Err(PluginFailure::InvalidPackage);
    }
    let input = serde_json::to_vec(&invocation).map_err(|_| PluginFailure::InvalidPackage)?;
    if input.len() > MAX_INPUT_BYTES {
        return Err(PluginFailure::InvalidPackage);
    }

    let writable_paths = manifest.writable_paths;
    tokio::task::spawn_blocking(move || execute_blocking(module_bytes, input, writable_paths))
        .await
        .map_err(|_| PluginFailure::ExecutionFailed)?
}

fn execute_blocking(
    module_bytes: Vec<u8>,
    input: Vec<u8>,
    writable_paths: Vec<String>,
) -> Result<JsonValue, PluginFailure> {
    let engine = engine()?;
    let module = Module::new(engine, &module_bytes).map_err(|_| PluginFailure::InvalidPackage)?;
    if module.imports().next().is_some() {
        return Err(PluginFailure::PermissionDenied);
    }

    let limits = StoreLimitsBuilder::new()
        .memory_size(MAX_MEMORY_BYTES)
        .memories(1)
        .tables(1)
        .table_elements(1_024)
        .instances(1)
        .build();
    let mut store = Store::new(engine, HostState { limits });
    store.limiter(|state| &mut state.limits);
    store.set_fuel(FUEL_LIMIT).map_err(|_| PluginFailure::ExecutionFailed)?;
    store.set_epoch_deadline(EPOCH_DEADLINE_TICKS);
    let start = Instant::now();

    let linker = Linker::new(engine);
    let instance = linker
        .instantiate(&mut store, &module)
        .map_err(|error| classify_execution_error(&error))?;
    let memory = instance
        .get_memory(&mut store, "memory")
        .ok_or(PluginFailure::InvalidPackage)?;
    let required_pages = input.len().div_ceil(65_536);
    let current_pages = memory.data_size(&store).div_ceil(65_536);
    if required_pages > current_pages {
        memory
            .grow(&mut store, (required_pages - current_pages) as u64)
            .map_err(|_| PluginFailure::InvalidPackage)?;
    }
    memory
        .write(&mut store, 0, &input)
        .map_err(|_| PluginFailure::InvalidPackage)?;
    let apply = instance
        .get_typed_func::<(i32, i32), i64>(&mut store, "apply")
        .map_err(|_| PluginFailure::InvalidPackage)?;
    let packed = apply
        .call(&mut store, (0, input.len() as i32))
        .map_err(|error| classify_execution_error(&error))? as u64;
    if start.elapsed() > EPOCH_INTERVAL * EPOCH_DEADLINE_TICKS as u32 {
        return Err(PluginFailure::TimedOut);
    }

    let pointer = packed as u32 as usize;
    let length = (packed >> 32) as u32 as usize;
    if length > MAX_PATCH_BYTES
        || pointer
            .checked_add(length)
            .is_none_or(|end| end > memory.data_size(&store))
    {
        return Err(PluginFailure::InvalidPatch);
    }
    let mut output = vec![0; length];
    memory
        .read(&store, pointer, &mut output)
        .map_err(|_| PluginFailure::InvalidPatch)?;
    let patch: JsonValue = serde_json::from_slice(&output).map_err(|_| PluginFailure::InvalidPatch)?;
    if clash5_data::validate_plugin_patch(&patch, &writable_paths).is_err() {
        return Err(PluginFailure::InvalidPatch);
    }
    Ok(patch)
}

fn classify_execution_error(error: &wasmtime::Error) -> PluginFailure {
    if error
        .downcast_ref::<Trap>()
        .is_some_and(|trap| matches!(trap, Trap::Interrupt | Trap::OutOfFuel))
    {
        PluginFailure::TimedOut
    } else {
        PluginFailure::ExecutionFailed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SIGNER: &str = "untrusted comment: minisign public key E7620F1842B4E81F\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
    const TEST_SIGNATURE: &str = "untrusted comment: signature from minisign secret key\nRUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\ntrusted comment: timestamp:1556193335\tfile:test\ny/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR1FkZZSNCisQbuQY+bHwhEBg==";

    #[test]
    fn minisign_verifies_the_exact_manifest_bytes() {
        assert_eq!(verify_manifest_signature(b"test", TEST_SIGNATURE, TEST_SIGNER), Ok(()));
        assert_eq!(
            verify_manifest_signature(b"Test", TEST_SIGNATURE, TEST_SIGNER),
            Err(PluginFailure::InvalidPackage)
        );
    }

    #[test]
    fn signed_desktop_fixture_passes_package_verification_and_requires_signer_consent() {
        let package = include_bytes!("../../../scripts/fixtures/m3-acceptance-plugin.c5p");
        let inspected = inspect_package(package);
        assert!(inspected.is_ok());
        let Some(inspected) = inspected.ok() else {
            return;
        };
        assert_eq!(inspected.manifest.id, "m3.smoke.plugin");
        assert_eq!(
            inspected.manifest.permissions,
            vec!["config.read".to_owned(), "config.patch".to_owned()]
        );
        assert!(matches!(
            verify_package(package, &["untrusted signer".into()]),
            Err(PluginFailure::PermissionDenied)
        ));
        assert!(verify_package(package, &[inspected.signer]).is_ok());
    }

    #[test]
    fn manifest_rejects_rules_and_unbounded_permission_paths() {
        let mut manifest = PluginManifest {
            schema_version: 1,
            id: "sample.plugin".into(),
            version: "1.0.0".into(),
            name: "Sample".into(),
            description: String::new(),
            abi_version: 1,
            permissions: vec!["config.patch".into()],
            writable_paths: vec!["/dns/nameserver".into()],
            module_sha256: "a".repeat(64),
        };
        assert_eq!(validate_manifest(&manifest), Ok(()));
        manifest.permissions = vec!["network".into(), "config.patch".into()];
        assert_eq!(validate_manifest(&manifest), Err(PluginFailure::InvalidPackage));
        manifest.permissions = vec!["config.read".into()];
        assert_eq!(validate_manifest(&manifest), Err(PluginFailure::InvalidPackage));
        manifest.permissions = vec!["config.patch".into()];
        manifest.writable_paths = vec!["/rules".into()];
        assert_eq!(validate_manifest(&manifest), Err(PluginFailure::InvalidPackage));
        manifest.writable_paths = vec![String::new()];
        assert_eq!(validate_manifest(&manifest), Err(PluginFailure::InvalidPackage));
    }

    #[test]
    fn imported_host_capabilities_are_denied_before_instantiation() {
        let module = [
            0, 97, 115, 109, 1, 0, 0, 0, // wasm header
            1, 4, 1, 0x60, 0, 0, // function type
            2, 9, 1, 3, b'e', b'n', b'v', 1, b'x', 0, 0, // imported function
        ];
        assert_eq!(
            execute_blocking(module.to_vec(), b"{}".to_vec(), vec![]),
            Err(PluginFailure::PermissionDenied)
        );
    }

    #[test]
    fn linear_memory_above_the_host_limit_is_rejected() {
        let module = [
            0, 97, 115, 109, 1, 0, 0, 0, // wasm header
            5, 4, 1, 0, 0x81, 0x04, // memory with 513 pages (32 MiB + 1 page)
            7, 10, 1, 6, b'm', b'e', b'm', b'o', b'r', b'y', 2, 0, // exported memory
        ];
        assert!(execute_blocking(module.to_vec(), b"{}".to_vec(), vec![]).is_err());
    }

    #[test]
    fn instruction_budget_interrupts_an_infinite_plugin_loop() {
        let module = [
            0, 97, 115, 109, 1, 0, 0, 0, // wasm header
            1, 7, 1, 0x60, 2, 0x7f, 0x7f, 1, 0x7e, // (i32, i32) -> i64
            3, 2, 1, 0, // one function
            5, 3, 1, 0, 1, // one-page memory
            7, 18, 2, 6, b'm', b'e', b'm', b'o', b'r', b'y', 2, 0, 5, b'a', b'p', b'p', b'l', b'y', 0, 0, 10, 10, 1, 8,
            0, 3, 0x40, 0x0c, 0, 0x0b, 0x00, 0x0b, // infinite loop body
        ];
        assert_eq!(
            execute_blocking(module.to_vec(), b"{}".to_vec(), vec![]),
            Err(PluginFailure::TimedOut)
        );
    }
}
