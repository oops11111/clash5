use anyhow::{Result, anyhow, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{collections::HashSet, path::Path};

pub(crate) const INDEX_MAX_BYTES: usize = 256 * 1024;
pub(crate) const SIGNATURE_MAX_BYTES: usize = 4 * 1024;
pub(crate) const PACKAGE_MAX_BYTES: usize = 10 * 1024 * 1024;
const MAX_PLUGINS: usize = 256;
const MAX_CATALOGS: usize = 32;

/// One plugin listed by a catalog. `sha256` covers the exact `.c5p` bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub(crate) struct CatalogEntry {
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    pub file: String,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub(crate) struct CatalogIndex {
    pub schema_version: u16,
    pub name: String,
    pub plugins: Vec<CatalogEntry>,
}

/// A catalog the user has confirmed. The verified index and its signature are kept so the catalog
/// can be read while offline. The signer is pinned at confirmation time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CatalogRecord {
    pub id: String,
    pub url: String,
    pub signer: String,
    pub index_json: String,
    pub signature: String,
}

pub(crate) fn validate_catalog_url(raw: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(raw).map_err(|_| anyhow!("catalog URL is invalid"))?;
    ensure!(url.scheme() == "https", "catalog URL must use https");
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "catalog URL must not carry credentials"
    );
    ensure!(url.fragment().is_none(), "catalog URL must not carry a fragment");
    ensure!(url.host_str().is_some(), "catalog URL needs a host");
    Ok(url)
}

/// Resolves a catalog entry's package beside the index. Only plain file names are accepted.
pub(crate) fn package_url(index_url: &reqwest::Url, file: &str) -> Result<reqwest::Url> {
    ensure!(
        file.len() <= 128
            && file.ends_with(".c5p")
            && file
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')),
        "catalog package file name is not allowed"
    );
    let url = index_url
        .join(file)
        .map_err(|_| anyhow!("catalog package URL is invalid"))?;
    ensure!(
        url.origin() == index_url.origin(),
        "catalog package must be served from the catalog origin"
    );
    Ok(url)
}

/// Joins a plain file name beside a catalog index, for the fixed companion files.
pub(crate) fn package_url_named(index_url: &reqwest::Url, file: &str) -> Result<reqwest::Url> {
    ensure!(
        matches!(file, "signer.pub" | "index.json.minisig"),
        "catalog companion file is not allowed"
    );
    index_url
        .join(file)
        .map_err(|_| anyhow!("catalog companion URL is invalid"))
}

pub(crate) fn parse_index(bytes: &[u8]) -> Result<CatalogIndex> {
    ensure!(bytes.len() <= INDEX_MAX_BYTES, "catalog index is too large");
    let index: CatalogIndex = serde_json::from_slice(bytes).map_err(|_| anyhow!("catalog index is malformed"))?;
    ensure!(index.schema_version == 1, "unsupported catalog schema");
    ensure!(
        !index.name.trim().is_empty() && index.name.len() <= 128,
        "catalog name is invalid"
    );
    ensure!(index.plugins.len() <= MAX_PLUGINS, "catalog lists too many plugins");
    let mut ids = HashSet::new();
    for entry in &index.plugins {
        ensure!(valid_plugin_id(&entry.id), "catalog plugin ID is invalid");
        ensure!(
            !entry.version.is_empty() && entry.version.len() <= 32,
            "catalog plugin version is invalid"
        );
        ensure!(
            !entry.name.is_empty() && entry.name.len() <= 128,
            "catalog plugin name is invalid"
        );
        ensure!(entry.description.len() <= 512, "catalog plugin description is too long");
        ensure!(
            entry.sha256.len() == 64 && entry.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "catalog plugin digest is invalid"
        );
        package_url(&reqwest::Url::parse("https://catalog.invalid/index.json")?, &entry.file)?;
        ensure!(ids.insert(entry.id.as_str()), "catalog lists a plugin ID twice");
    }
    Ok(index)
}

fn valid_plugin_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-'))
}

/// minisign verification of the exact index bytes, against the signer pinned for this catalog.
pub(crate) fn verify_index(index: &[u8], signature: &str, signer: &str) -> Result<()> {
    ensure!(signature.len() <= SIGNATURE_MAX_BYTES, "catalog signature is too large");
    let public_key = minisign_verify::PublicKey::decode(signer).map_err(|_| anyhow!("catalog signer is invalid"))?;
    let signature =
        minisign_verify::Signature::decode(signature).map_err(|_| anyhow!("catalog signature is invalid"))?;
    public_key
        .verify(index, &signature, false)
        .map_err(|_| anyhow!("catalog index signature does not verify"))
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Writes the catalog list through a temporary file so an interrupted write keeps the old list.
pub(crate) fn save_records(path: &Path, records: &[CatalogRecord]) -> Result<()> {
    ensure!(records.len() <= MAX_CATALOGS, "too many catalogs");
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, serde_json::to_vec_pretty(records)?)?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

pub(crate) fn load_records(path: &Path) -> Result<Vec<CatalogRecord>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &[u8] = include_bytes!("../../../scripts/fixtures/catalog/index.json");
    const SIGNATURE: &str = include_str!("../../../scripts/fixtures/catalog/index.json.minisig");
    const SIGNER: &str = include_str!("../../../scripts/fixtures/catalog/signer.pub");

    #[test]
    fn a_signed_index_verifies_and_parses() -> Result<()> {
        verify_index(INDEX, SIGNATURE, SIGNER)?;
        let index = parse_index(INDEX)?;
        assert_eq!(index.plugins[0].id, "m3.smoke.plugin");
        Ok(())
    }

    #[test]
    fn a_modified_index_or_another_signer_does_not_verify() {
        let mut tampered = INDEX.to_vec();
        tampered[10] ^= 1;
        assert!(verify_index(&tampered, SIGNATURE, SIGNER).is_err());
        let other = SIGNER.replace("RWQ", "RWR");
        assert!(verify_index(INDEX, SIGNATURE, &other).is_err());
    }

    #[test]
    fn index_entries_with_paths_or_bad_digests_are_rejected() {
        let base = |file: &str, digest: &str| {
            format!(
                r#"{{"schema_version":1,"name":"c","plugins":[{{"id":"a.b","version":"1.0.0","name":"n","description":"","file":"{file}","sha256":"{digest}"}}]}}"#
            )
        };
        let good = "a".repeat(64);
        assert!(parse_index(base("p.c5p", &good).as_bytes()).is_ok());
        assert!(parse_index(base("../p.c5p", &good).as_bytes()).is_err());
        assert!(parse_index(base("dir/p.c5p", &good).as_bytes()).is_err());
        assert!(parse_index(base("p.zip", &good).as_bytes()).is_err());
        assert!(parse_index(base("p.c5p", "xyz").as_bytes()).is_err());
    }

    #[test]
    fn catalog_urls_must_be_https_and_packages_stay_on_the_origin() -> Result<()> {
        assert!(validate_catalog_url("http://example.invalid/index.json").is_err());
        assert!(validate_catalog_url("https://user:pw@example.invalid/index.json").is_err());
        let index = validate_catalog_url("https://example.invalid/catalog/index.json")?;
        assert_eq!(
            package_url(&index, "p.c5p")?.as_str(),
            "https://example.invalid/catalog/p.c5p"
        );
        Ok(())
    }
}
