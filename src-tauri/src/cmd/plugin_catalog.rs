use super::{CmdResult, CommandFailure, network::fetch_via_core};
use crate::clash5::{
    catalog::{
        self, CatalogEntry, CatalogIndex, CatalogRecord, INDEX_MAX_BYTES, PACKAGE_MAX_BYTES, SIGNATURE_MAX_BYTES,
    },
    plugins,
};
use crate::utils::dirs;
use std::path::PathBuf;

#[derive(serde::Serialize, specta::Type)]
pub struct PluginCatalogView {
    id: String,
    name: String,
    url: String,
    signer_fingerprint: String,
    plugins: Vec<CatalogEntry>,
}

#[derive(serde::Serialize, specta::Type)]
pub struct PluginCatalogPreview {
    url: String,
    name: String,
    signer: String,
    signer_fingerprint: String,
    plugin_count: u32,
}

fn records_path() -> CmdResult<PathBuf> {
    Ok(dirs::app_home_dir()
        .map_err(CommandFailure::plain)?
        .join("plugin-catalogs.json"))
}

fn fingerprint(signer: &str) -> String {
    catalog::sha256_hex(signer.trim().as_bytes())
}

fn failure(detail: impl std::fmt::Display) -> CommandFailure {
    CommandFailure::coded("C5-CATALOG-001", detail)
}

/// Fetches a catalog's index, signature and pinned signer key from the catalog location.
async fn fetch_catalog(url: &str) -> CmdResult<(CatalogIndex, String, String, String)> {
    let index_url = catalog::validate_catalog_url(url).map_err(failure)?;
    let signer_url = catalog::package_url_named(&index_url, "signer.pub").map_err(failure)?;
    let signature_url = catalog::package_url_named(&index_url, "index.json.minisig").map_err(failure)?;
    let index_bytes = fetch_via_core(url, INDEX_MAX_BYTES).await?;
    let signature = String::from_utf8(fetch_via_core(signature_url.as_str(), SIGNATURE_MAX_BYTES).await?)
        .map_err(|_| failure("catalog signature is not text"))?;
    let signer = String::from_utf8(fetch_via_core(signer_url.as_str(), 1024).await?)
        .map_err(|_| failure("catalog signer is not text"))?;
    catalog::verify_index(&index_bytes, &signature, &signer).map_err(failure)?;
    let index = catalog::parse_index(&index_bytes).map_err(failure)?;
    let index_json = String::from_utf8(index_bytes).map_err(|_| failure("catalog index is not UTF-8"))?;
    Ok((index, index_json, signature, signer))
}

fn view(record: &CatalogRecord) -> CmdResult<PluginCatalogView> {
    let index: CatalogIndex =
        serde_json::from_str(&record.index_json).map_err(|_| failure("stored index is invalid"))?;
    Ok(PluginCatalogView {
        id: record.id.clone(),
        name: index.name,
        url: record.url.clone(),
        signer_fingerprint: fingerprint(&record.signer),
        plugins: index.plugins,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn clash5_list_plugin_catalogs() -> CmdResult<Vec<PluginCatalogView>> {
    let path = records_path()?;
    let records = catalog::load_records(&path).map_err(failure)?;
    records.iter().map(view).collect()
}

#[tauri::command]
#[specta::specta]
pub async fn clash5_preview_plugin_catalog(url: String) -> CmdResult<PluginCatalogPreview> {
    let (index, _, _, signer) = fetch_catalog(&url).await?;
    Ok(PluginCatalogPreview {
        url,
        name: index.name,
        signer_fingerprint: fingerprint(&signer),
        signer,
        plugin_count: index.plugins.len() as u32,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn clash5_save_plugin_catalog(
    url: String,
    confirmed_signer_fingerprint: String,
) -> CmdResult<PluginCatalogView> {
    let path = records_path()?;
    let (_, index_json, signature, signer) = fetch_catalog(&url).await?;
    if fingerprint(&signer) != confirmed_signer_fingerprint {
        return Err(failure("catalog signer changed since it was previewed"));
    }
    let mut records = catalog::load_records(&path).map_err(failure)?;
    if records.iter().any(|record| record.url == url) {
        return Err(failure("this catalog is already added"));
    }
    let record = CatalogRecord {
        id: nanoid::nanoid!(),
        url,
        signer,
        index_json,
        signature,
    };
    let view = view(&record)?;
    records.push(record);
    catalog::save_records(&path, &records).map_err(failure)?;
    Ok(view)
}

#[tauri::command]
#[specta::specta]
pub async fn clash5_refresh_plugin_catalog(catalog_id: String) -> CmdResult<PluginCatalogView> {
    let path = records_path()?;
    let mut records = catalog::load_records(&path).map_err(failure)?;
    let position = records
        .iter()
        .position(|record| record.id == catalog_id)
        .ok_or_else(|| failure("catalog is not known"))?;
    let url = records[position].url.clone();
    let pinned = records[position].signer.clone();
    let (_, index_json, signature, signer) = fetch_catalog(&url).await?;
    // The pinned signer is never replaced by a refresh. A new signer needs a new confirmation.
    if fingerprint(&signer) != fingerprint(&pinned) {
        return Err(failure("catalog signer changed; remove the catalog and add it again"));
    }
    records[position].index_json = index_json;
    records[position].signature = signature;
    let view = view(&records[position])?;
    catalog::save_records(&path, &records).map_err(failure)?;
    Ok(view)
}

#[tauri::command]
#[specta::specta]
pub async fn clash5_remove_plugin_catalog(catalog_id: String) -> CmdResult {
    let path = records_path()?;
    let mut records = catalog::load_records(&path).map_err(failure)?;
    let before = records.len();
    records.retain(|record| record.id != catalog_id);
    if records.len() == before {
        return Err(failure("catalog is not known"));
    }
    catalog::save_records(&path, &records).map_err(failure)?;
    Ok(())
}

/// Downloads one listed package, checks it against the signed digest and manifest, and stages it.
/// The result goes through the same preview and install flow as a file picked by the user.
#[tauri::command]
#[specta::specta]
pub async fn clash5_download_catalog_plugin(
    catalog_id: String,
    plugin_id: String,
) -> CmdResult<super::clash5_data::PluginPackagePreview> {
    let path = records_path()?;
    let records = catalog::load_records(&path).map_err(failure)?;
    let record = records
        .iter()
        .find(|record| record.id == catalog_id)
        .ok_or_else(|| failure("catalog is not known"))?;
    let index: CatalogIndex =
        serde_json::from_str(&record.index_json).map_err(|_| failure("stored index is invalid"))?;
    let entry = index
        .plugins
        .into_iter()
        .find(|entry| entry.id == plugin_id)
        .ok_or_else(|| failure("plugin is not listed by this catalog"))?;
    let index_url = catalog::validate_catalog_url(&record.url).map_err(failure)?;
    let package_url = catalog::package_url(&index_url, &entry.file).map_err(failure)?;
    let bytes = fetch_via_core(package_url.as_str(), PACKAGE_MAX_BYTES).await?;
    if catalog::sha256_hex(&bytes) != entry.sha256 {
        return Err(failure("downloaded package does not match the signed catalog digest"));
    }
    let inspected =
        plugins::inspect_package(&bytes).map_err(|_| failure("downloaded package is not a valid plugin"))?;
    if inspected.manifest.id != entry.id || inspected.manifest.version != entry.version {
        return Err(failure("downloaded package does not match the catalog entry"));
    }
    super::clash5_data::stage_package_bytes(bytes).await
}
