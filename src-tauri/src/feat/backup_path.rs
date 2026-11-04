use std::{
    io,
    path::{Component, Path, PathBuf},
};

pub(crate) fn backup_source(directory: &Path, filename: &str) -> io::Result<PathBuf> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidInput, "Invalid backup filename");
    let mut components = Path::new(filename).components();
    if filename.contains(['/', '\\', ':'])
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        return Err(invalid());
    }
    let root = directory.canonicalize()?;
    let source = root.join(filename).canonicalize()?;
    if source.parent() != Some(root.as_path()) || !source.is_file() {
        return Err(invalid());
    }
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_rejects_traversal() {
        for name in ["../secret", "..\\secret", "a/../secret", "a\\secret", ".", "..", ""] {
            assert_eq!(
                backup_source(Path::new("missing"), name).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }

    #[test]
    fn export_rejects_absolute() {
        for name in [
            "/secret",
            "C:\\secret",
            "C:secret",
            "\\\\server\\share",
            "file.zip:stream",
        ] {
            assert_eq!(
                backup_source(Path::new("missing"), name).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }

    #[test]
    fn export_accepts_local_file() {
        let directory = std::env::temp_dir().join(format!("clash5-backup-path-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("backup.zip"), b"fixture").unwrap();
        assert_eq!(
            backup_source(&directory, "backup.zip").unwrap(),
            directory.join("backup.zip").canonicalize().unwrap()
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn export_rejects_symlink_escape() {
        let directory = std::env::temp_dir().join(format!("clash5-backup-link-{}", std::process::id()));
        let backups = directory.join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        std::fs::write(directory.join("outside.zip"), b"fixture").unwrap();
        std::os::unix::fs::symlink(directory.join("outside.zip"), backups.join("linked.zip")).unwrap();
        assert_eq!(
            backup_source(&backups, "linked.zip").unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
