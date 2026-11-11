use anyhow::{Context as _, Result, bail, ensure};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

pub(crate) const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
pub(crate) const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 512 * 1024 * 1024;
pub(crate) const MAX_ENTRIES: usize = 10_000;
const REQUIRED_FILES: [&str; 3] = ["config.yaml", "verge.yaml", "profiles.yaml"];
const DATA_DB: &str = "clash5-nodes.sqlite";

pub(crate) struct StagedBackup {
    root: PathBuf,
    files: Vec<PathBuf>,
    preserve: bool,
}

pub(crate) fn validate_database(snapshot: &Path) -> Result<()> {
    let mut header = [0_u8; 16];
    File::open(snapshot)?.read_exact(&mut header)?;
    ensure!(&header == b"SQLite format 3\0", "Invalid backup database header");
    clash5_data::BindingStore::open(snapshot)?.integrity_check()?;
    Ok(())
}

pub(crate) fn restore_database(snapshot: &Path, previous: &Path, destination: &Path) -> Result<()> {
    validate_database(snapshot)?;
    let existed = destination.try_exists()?;
    let mut store = clash5_data::BindingStore::open(destination)?;
    let recovery = previous.join(DATA_DB);
    if existed {
        store.snapshot_to(&recovery)?;
    }
    if let Err(error) = store.restore_from(snapshot).and_then(|()| store.integrity_check()) {
        if existed {
            if let Err(rollback) = store.restore_from(&recovery) {
                bail!("Database restore failed: {error}; database rollback failed: {rollback}");
            }
        } else {
            drop(store);
            fs::remove_file(destination)?;
            for extension in ["sqlite-wal", "sqlite-shm"] {
                match fs::remove_file(destination.with_extension(extension)) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        return Err(error.into());
    }
    Ok(())
}

fn safe_component(component: &str) -> bool {
    !component.is_empty()
        && component != "."
        && component != ".."
        && !component.ends_with(['.', ' '])
        && !component.chars().any(|c| c.is_control() || "\\/:<>\"|?*".contains(c))
        && !matches!(
            component
                .split('.')
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase()
                .as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
}

pub(crate) fn safe_profile_file(name: &str) -> bool {
    safe_component(name)
        && matches!(
            Path::new(name).extension().and_then(|ext| ext.to_str()),
            Some("yaml" | "yml" | "js")
        )
}

fn archive_path(name: &str, directory: bool) -> Result<PathBuf> {
    if directory {
        ensure!(name == "profiles/", "Unexpected backup directory");
        return Ok(PathBuf::from("profiles"));
    }
    if let Some(filename) = name.strip_prefix("profiles/") {
        ensure!(safe_profile_file(filename), "Invalid backup profile path");
    } else {
        ensure!(
            matches!(
                name,
                "config.yaml"
                    | "verge.yaml"
                    | "profiles.yaml"
                    | "dns_config.yaml"
                    | "clash5-nodes.sqlite"
                    | "clash5-backup.json"
            ),
            "Unexpected backup file"
        );
    }
    Ok(PathBuf::from(name))
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn destination(home: &Path, relative: &Path) -> Result<PathBuf> {
    let mut target = home.to_path_buf();
    for component in relative.components() {
        target.push(component);
        match fs::symlink_metadata(&target) {
            Ok(metadata) => ensure!(!is_link(&metadata), "Backup destination contains a link"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(target)
}

impl StagedBackup {
    pub(crate) fn prepare<R: Read + Seek>(mut source: R, home: &Path) -> Result<Self> {
        ensure!(
            source.seek(SeekFrom::End(0))? <= MAX_ARCHIVE_BYTES,
            "Backup archive exceeds 64 MiB"
        );
        source.rewind()?;
        let mut zip = zip::ZipArchive::new(source)?;
        ensure!(zip.len() <= MAX_ENTRIES, "Backup archive has too many entries");
        let root = home.join(format!(".clash5-restore-{}", nanoid::nanoid!()));
        fs::create_dir(&root)?;
        let mut staged = Self {
            root,
            files: Vec::new(),
            preserve: false,
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&staged.root, fs::Permissions::from_mode(0o700))?;
        }
        let mut names = HashSet::new();
        let mut total = 0_u64;
        let mut entries = Vec::new();
        for index in 0..zip.len() {
            let entry = zip.by_index(index)?;
            let name = std::str::from_utf8(entry.name_raw())?;
            let relative = archive_path(name, entry.is_dir())?;
            ensure!(
                names.insert(relative.to_string_lossy().to_lowercase()),
                "Duplicate backup path"
            );
            if let Some(mode) = entry.unix_mode() {
                let kind = mode & 0o170000;
                ensure!(
                    kind == 0 || kind == if entry.is_dir() { 0o040000 } else { 0o100000 },
                    "Backup contains a special file"
                );
            }
            ensure!(entry.size() <= MAX_ENTRY_BYTES, "Backup entry exceeds 64 MiB");
            total = total.checked_add(entry.size()).context("Backup size overflow")?;
            ensure!(total <= MAX_EXPANDED_BYTES, "Expanded backup exceeds 512 MiB");
            entries.push(relative);
        }
        for name in REQUIRED_FILES {
            ensure!(names.contains(name), "Backup is missing a required config file");
        }
        for (index, relative) in entries.into_iter().enumerate() {
            let mut entry = zip.by_index(index)?;
            let target = staged.root.join(&relative);
            if entry.is_dir() {
                fs::create_dir_all(target)?;
                continue;
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                options.mode(0o600);
            }
            let mut output = options.open(&target)?;
            let expected = entry.size();
            let copied = std::io::copy(&mut entry.by_ref().take(expected + 1), &mut output)?;
            ensure!(copied == expected, "Backup entry size mismatch");
            output.sync_all()?;
            staged.files.push(relative);
        }
        Ok(staged)
    }

    pub(crate) fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    pub(crate) fn publish(
        mut self,
        home: &Path,
        apply_database: impl FnOnce(&Path, &Path) -> Result<()>,
    ) -> Result<()> {
        let old = self.root.join("previous");
        fs::create_dir(&old)?;
        // Prepare every rollback copy before the first live file is replaced.
        let mut originals = Vec::new();
        for relative in self.files.iter().filter(|path| path.as_path() != Path::new(DATA_DB)) {
            let target = destination(home, relative)?;
            let exists = target.try_exists()?;
            if exists {
                ensure!(target.is_file(), "Backup destination is not a regular file");
                let copy = old.join(relative);
                if let Some(parent) = copy.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&target, copy)?;
            }
            originals.push((relative.clone(), exists));
        }
        let mut published = 0;
        let result = (|| {
            for (relative, _) in &originals {
                let target = destination(home, relative)?;
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::rename(self.root.join(relative), target)?;
                published += 1;
            }
            if self.path(DATA_DB).is_file() {
                let _ = destination(home, Path::new(DATA_DB))?;
                if let Err(error) = apply_database(&self.path(DATA_DB), &old) {
                    self.preserve = true;
                    return Err(error.context(format!("Database recovery snapshot retained in {}", old.display())));
                }
            }
            Ok::<_, anyhow::Error>(())
        })();
        if let Err(error) = result {
            let mut failures = Vec::new();
            for (relative, existed) in originals[..published].iter().rev() {
                let target = home.join(relative);
                let rollback = if *existed {
                    fs::rename(old.join(relative), &target)
                } else {
                    fs::remove_file(&target)
                };
                if let Err(error) = rollback {
                    failures.push(error.to_string());
                }
            }
            if !failures.is_empty() {
                self.preserve = true;
                bail!(
                    "Restore failed: {error:#}; rollback failed: {}; recovery copies: {}",
                    failures.join("; "),
                    old.display()
                );
            }
            return Err(error);
        }
        Ok(())
    }
}

impl Drop for StagedBackup {
    fn drop(&mut self) {
        if !self.preserve {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
