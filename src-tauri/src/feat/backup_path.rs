use std::{
    io,
    path::{Component, Path, PathBuf},
};

fn invalid_backup_path() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "Invalid backup path")
}

pub(crate) fn validate_backup_filename(filename: &str) -> io::Result<()> {
    let mut components = Path::new(filename).components();
    if filename.contains(['/', '\\', ':', '%', '?', '#'])
        || filename.chars().any(char::is_control)
        || filename.ends_with(['.', ' '])
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        return Err(invalid_backup_path());
    }
    Ok(())
}

pub(crate) fn backup_source(directory: &Path, filename: &str) -> io::Result<PathBuf> {
    validate_backup_filename(filename)?;
    let root = directory.canonicalize()?;
    let source = root.join(filename).canonicalize()?;
    if source.parent() != Some(root.as_path()) || !source.is_file() {
        return Err(invalid_backup_path());
    }
    Ok(source)
}

pub(crate) fn normalize_export_destination(destination: &Path) -> io::Result<PathBuf> {
    if !destination.is_absolute()
        || destination
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(invalid_backup_path());
    }

    let file_name = destination.file_name().ok_or_else(invalid_backup_path)?;
    let parent = destination.parent().ok_or_else(invalid_backup_path)?.canonicalize()?;
    let target = parent.join(file_name);
    if target.exists() {
        target.canonicalize()
    } else {
        Ok(target)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_and_absolute_backup_filenames() {
        for name in [
            "../secret.zip",
            "..\\secret.zip",
            "folder/../secret.zip",
            "C:\\secret.zip",
            "/secret.zip",
            "file.zip:stream",
        ] {
            assert_eq!(
                validate_backup_filename(name).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }

    #[test]
    fn rejects_traversal_and_relative_export_destinations() {
        for path in [Path::new("relative.zip"), Path::new("C:/allowed/../outside.zip")] {
            assert_eq!(
                normalize_export_destination(path).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }

    #[test]
    fn accepts_canonical_local_backup_source() {
        let directory = std::env::temp_dir().join(format!("clash5-backup-source-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("backup.zip"), b"fixture").unwrap();
        assert_eq!(
            backup_source(&directory, "backup.zip").unwrap(),
            directory.join("backup.zip").canonicalize().unwrap()
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn resolves_destination_parent_before_export() {
        let directory = std::env::temp_dir().join(format!("clash5-backup-dest-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let destination = directory.join("backup.zip");
        assert_eq!(
            normalize_export_destination(&destination).unwrap(),
            directory.canonicalize().unwrap().join("backup.zip")
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_backup_symlink_escape() {
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
