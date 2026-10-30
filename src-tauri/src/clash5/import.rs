use anyhow::{Context as _, Result, bail};
use serde_yaml_ng::{Mapping, Value};
use sha2::{Digest as _, Sha256};
use std::{
    fs::{self, File},
    io::Read as _,
    path::{Component, Path},
};

const MAX_MANIFEST_ENTRIES: usize = 100_000;
const MAX_PROFILE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct ImportedProfile {
    pub(crate) name: String,
    pub(crate) contents: String,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn update_file_hash(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("Could not read {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 32 * 1024].into_boxed_slice();
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex(&hasher.finalize()))
}

fn collect_manifest(root: &Path, directory: &Path, entries: &mut Vec<String>) -> Result<()> {
    let mut children = fs::read_dir(directory)
        .with_context(|| format!("Could not inspect {}", directory.display()))?
        .collect::<std::io::Result<Vec<_>>>()?;
    children.sort_by_key(|entry| entry.file_name());

    for child in children {
        if entries.len() >= MAX_MANIFEST_ENTRIES {
            bail!("source directory contains too many entries");
        }
        let path = child.path();
        let relative = path
            .strip_prefix(root)
            .context("Source path escaped the selected directory")?
            .to_string_lossy()
            .replace('\\', "/");
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&path)?.to_string_lossy().replace('\\', "/");
            entries.push(format!("L\0{relative}\0{target}"));
        } else if metadata.is_dir() {
            entries.push(format!("D\0{relative}"));
            collect_manifest(root, &path, entries)?;
        } else if metadata.is_file() {
            entries.push(format!("F\0{relative}\0{}", update_file_hash(&path)?));
        } else {
            entries.push(format!("O\0{relative}"));
        }
    }
    Ok(())
}

/// Hashes paths and file contents without following symlinks or writing below `root`.
pub(crate) fn source_manifest(root: &Path) -> Result<String> {
    let metadata = fs::symlink_metadata(root).context("Could not inspect source directory")?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("source path must be a real directory");
    }
    let root = fs::canonicalize(root).context("Could not resolve source directory")?;
    let mut entries = Vec::new();
    collect_manifest(&root, &root, &mut entries)?;
    entries.sort();
    let mut hasher = Sha256::new();
    for entry in entries {
        hasher.update(entry.as_bytes());
        hasher.update([0]);
    }
    Ok(format!("sha256:{}", hex(&hasher.finalize())))
}

pub(crate) fn ensure_separate_data_dirs(source: &Path, target: &Path) -> Result<()> {
    let source = fs::canonicalize(source).context("Could not resolve source directory")?;
    let target = fs::canonicalize(target).context("Could not resolve clash5 data directory")?;
    if source.starts_with(&target) || target.starts_with(&source) {
        bail!("source and clash5 data directories must be separate");
    }
    Ok(())
}

fn safe_profile_filename(raw: &str) -> Result<&Path> {
    if raw.contains('\\') {
        bail!("profile path must be a filename");
    }
    let path = Path::new(raw);
    let mut components = path.components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        bail!("profile path must be a filename");
    }
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "yaml" | "yml"))
    {
        bail!("profile file must be YAML");
    }
    Ok(path)
}

fn has_external_provider_files(root: &Mapping) -> bool {
    ["proxy-providers", "rule-providers"].iter().any(|section| {
        root.get(Value::String((*section).into()))
            .and_then(Value::as_mapping)
            .is_some_and(|providers| {
                providers.values().any(|provider| {
                    provider.as_mapping().is_some_and(|provider| {
                        provider.contains_key(Value::String("path".into()))
                            || provider.contains_key(Value::String("url".into()))
                    })
                })
            })
    })
}

fn profile_label(item: &crate::config::PrfItem, fallback: &str) -> String {
    item.name
        .as_deref()
        .map(str::to_owned)
        .unwrap_or_else(|| fallback.to_owned())
}

/// Reads supported cached profile files. Remote URLs and external provider files are never imported.
pub(crate) fn read_supported_profiles(source: &Path) -> Result<(Vec<ImportedProfile>, Vec<String>)> {
    let root = fs::canonicalize(source).context("Could not resolve source directory")?;
    let metadata_path = root.join("profiles.yaml");
    let metadata = fs::symlink_metadata(&metadata_path).context("profiles.yaml was not found")?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("profiles.yaml must be a regular file");
    }
    let serialized = fs::read_to_string(&metadata_path).context("Could not read profiles.yaml")?;
    let profiles: crate::config::IProfiles =
        serde_yaml_ng::from_str(&serialized).context("profiles.yaml is invalid")?;
    let profiles_dir = root.join("profiles");
    let profiles_metadata = fs::symlink_metadata(&profiles_dir).context("profiles directory was not found")?;
    if profiles_metadata.file_type().is_symlink() || !profiles_metadata.is_dir() {
        bail!("profiles directory must be a real directory");
    }

    let mut imported = Vec::new();
    let mut skipped = Vec::new();
    for item in profiles.items.unwrap_or_default() {
        let Some(kind) = item.itype.as_deref() else {
            skipped.push("profile without a type".into());
            continue;
        };
        if !matches!(kind, "local" | "remote") {
            skipped.push(profile_label(&item, "unsupported profile"));
            continue;
        }
        let Some(file) = item.file.as_deref() else {
            skipped.push(profile_label(&item, "profile without a cache file"));
            continue;
        };
        let filename = match safe_profile_filename(file) {
            Ok(filename) => filename,
            Err(_) => {
                skipped.push(profile_label(&item, "profile with an unsafe file path"));
                continue;
            }
        };
        let path = profiles_dir.join(filename);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => metadata,
            _ => {
                skipped.push(profile_label(&item, "profile cache file is unavailable"));
                continue;
            }
        };
        if metadata.len() > MAX_PROFILE_BYTES {
            skipped.push(profile_label(&item, "profile cache file is too large"));
            continue;
        }
        let contents = fs::read_to_string(&path).context("Profile cache is not valid UTF-8")?;
        let parsed: Value = match serde_yaml_ng::from_str(&contents) {
            Ok(parsed) => parsed,
            Err(_) => {
                skipped.push(profile_label(&item, "profile cache is invalid YAML"));
                continue;
            }
        };
        let Some(mapping) = parsed.as_mapping() else {
            skipped.push(profile_label(&item, "profile cache is not a YAML mapping"));
            continue;
        };
        if has_external_provider_files(mapping) {
            skipped.push(profile_label(&item, "profile uses external provider files"));
            continue;
        }
        imported.push(ImportedProfile {
            name: item
                .name
                .as_deref()
                .map(str::to_owned)
                .unwrap_or_else(|| filename.file_stem().unwrap_or_default().to_string_lossy().into_owned()),
            contents,
        });
    }
    Ok((imported, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT_TEMP_DIR: AtomicUsize = AtomicUsize::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> std::io::Result<Self> {
            for _ in 0..100 {
                let nonce = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir().join(format!("clash5-import-{}-{nonce}", std::process::id()));
                match fs::create_dir(&path) {
                    Ok(()) => return Ok(Self(path)),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error),
                }
            }
            Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "could not allocate a unique import test directory",
            ))
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn fixture(root: &Path) -> Result<()> {
        fs::create_dir_all(root.join("profiles"))?;
        fs::write(
            root.join("profiles.yaml"),
            include_str!("../../tests/fixtures/clash5-import/profiles.yaml"),
        )?;
        fs::write(
            root.join("profiles/demo.yaml"),
            include_str!("../../tests/fixtures/clash5-import/demo.yaml"),
        )?;
        fs::write(
            root.join("profiles/external.yaml"),
            include_str!("../../tests/fixtures/clash5-import/external.yaml"),
        )?;
        Ok(())
    }

    #[test]
    fn manifest_is_deterministic_and_changes_when_source_content_changes() -> Result<()> {
        let temp = TestDir::new()?;
        fixture(temp.path())?;
        let before = source_manifest(temp.path())?;
        assert_eq!(before, source_manifest(temp.path())?);
        fs::write(temp.path().join("profiles/demo.yaml"), "proxies: []\n")?;
        assert_ne!(before, source_manifest(temp.path())?);
        Ok(())
    }

    #[test]
    fn reads_only_cached_inline_profiles_and_preserves_source_fixture() -> Result<()> {
        let temp = TestDir::new()?;
        fixture(temp.path())?;
        let before = source_manifest(temp.path())?;
        let (profiles, skipped) = read_supported_profiles(temp.path())?;
        let after = source_manifest(temp.path())?;
        assert_eq!(before, after);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, "Fixture subscription");
        assert!(profiles[0].contents.contains("fixture-node"));
        assert_eq!(skipped.len(), 2);
        assert!(skipped.iter().any(|item| item == "Unsupported merge profile"));
        assert!(skipped.iter().any(|item| item == "External provider profile"));
        assert!(!profiles[0].contents.contains("secret-token"));
        Ok(())
    }

    #[test]
    fn rejects_profile_paths_that_could_escape_the_source_tree() {
        for path in [
            "../profiles.yaml",
            "nested/demo.yaml",
            "C:\\secret.yaml",
            "/tmp/demo.yaml",
        ] {
            assert!(safe_profile_filename(path).is_err(), "{path}");
        }
    }

    #[test]
    fn refuses_source_and_target_directories_that_overlap() -> Result<()> {
        let temp = TestDir::new()?;
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        let nested = source.join("nested");
        fs::create_dir_all(&nested)?;
        fs::create_dir_all(&target)?;
        assert!(ensure_separate_data_dirs(&source, &target).is_ok());
        assert!(ensure_separate_data_dirs(&source, &nested).is_err());
        assert!(ensure_separate_data_dirs(&nested, &source).is_err());
        Ok(())
    }
}
