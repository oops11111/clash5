#![allow(clippy::unwrap_used, clippy::panic)]

#[path = "../src/feat/backup_archive.rs"]
mod backup_archive;

use backup_archive::{MAX_ARCHIVE_BYTES, StagedBackup};
use std::{
    fs,
    io::{Cursor, Write as _},
    path::PathBuf,
};
use zip::write::SimpleFileOptions;

struct Home(PathBuf);

impl Home {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("clash5-restore-test-{}", nanoid::nanoid!()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn archive(extra: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in [
        ("config.yaml", b"port: 7890".as_slice()),
        ("verge.yaml", b"{}"),
        ("profiles.yaml", b"{}"),
    ]
    .into_iter()
    .chain(extra.iter().copied())
    {
        writer
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(data).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn reject(bytes: Vec<u8>, message: &str) {
    let home = Home::new();
    fs::write(home.0.join("config.yaml"), b"old").unwrap();
    let error = match StagedBackup::prepare(Cursor::new(bytes), &home.0) {
        Ok(_) => panic!("Unsafe backup accepted"),
        Err(error) => error,
    };
    assert!(error.to_string().contains(message), "{error:#}");
    assert_eq!(fs::read(home.0.join("config.yaml")).unwrap(), b"old");
    assert_eq!(fs::read_dir(&home.0).unwrap().count(), 1);
}

#[test]
fn valid_backup_stays_staged_until_publication() {
    let home = Home::new();
    fs::write(home.0.join("config.yaml"), b"old").unwrap();
    let staged = StagedBackup::prepare(Cursor::new(archive(&[("profiles/a.yaml", b"proxies: []")])), &home.0).unwrap();
    assert_eq!(fs::read(home.0.join("config.yaml")).unwrap(), b"old");
    staged
        .publish(&home.0, |_, _| panic!("No database was included"))
        .unwrap();
    assert_eq!(fs::read(home.0.join("config.yaml")).unwrap(), b"port: 7890");
    assert_eq!(fs::read(home.0.join("profiles/a.yaml")).unwrap(), b"proxies: []");
}

#[test]
fn rejects_traversal_keys_special_names_and_unexpected_files_before_publication() {
    for name in [
        "../escape",
        "/escape",
        ".encryption_key",
        "plugins/untrusted.c5p",
        "profiles/../escape.yaml",
        "profiles/a\\b.yaml",
        "profiles/a.yaml:stream",
        "profiles/CON.yaml",
        "profiles/a.yaml ",
    ] {
        reject(
            archive(&[(name, b"unsafe")]),
            if name.starts_with("profiles/") {
                "Invalid backup profile path"
            } else {
                "Unexpected backup file"
            },
        );
    }
}

#[test]
fn rejects_case_collisions() {
    reject(
        archive(&[("profiles/A.yaml", b"a"), ("profiles/a.yaml", b"b")]),
        "Duplicate backup path",
    );
}

#[test]
fn corrupted_entry_does_not_publish_earlier_files() {
    let mut bytes = archive(&[("profiles/a.yaml", b"unique profile payload")]);
    let offset = bytes
        .windows(b"unique profile payload".len())
        .position(|part| part == b"unique profile payload")
        .unwrap();
    bytes[offset] ^= 0xff;
    reject(bytes, "checksum");
}

#[test]
fn rejects_symlinks_in_archive() {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_symlink("profiles/a.yaml", "../../outside", SimpleFileOptions::default())
        .unwrap();
    reject(writer.finish().unwrap().into_inner(), "special file");
}

#[test]
fn rejects_oversized_archive_before_reading_it() {
    let home = Home::new();
    let path = home.0.join("oversized.zip");
    let file = fs::File::create(&path).unwrap();
    file.set_len(MAX_ARCHIVE_BYTES + 1).unwrap();
    let error = StagedBackup::prepare(fs::File::open(path).unwrap(), &home.0)
        .err()
        .unwrap();
    assert!(error.to_string().contains("archive exceeds 64 MiB"));
}

fn with_declared_size(mut bytes: Vec<u8>, size: u32) -> Vec<u8> {
    for offset in 0..bytes.len().saturating_sub(46) {
        if bytes[offset..offset + 4] == [0x50, 0x4b, 0x01, 0x02] {
            bytes[offset + 24..offset + 28].copy_from_slice(&size.to_le_bytes());
        }
    }
    bytes
}

#[test]
fn rejects_entry_and_expanded_size_limits_from_directory_metadata() {
    reject(
        with_declared_size(archive(&[]), 65 * 1024 * 1024),
        "entry exceeds 64 MiB",
    );
    let names: Vec<_> = (0..6).map(|index| format!("profiles/{index}.yaml")).collect();
    let entries: Vec<_> = names.iter().map(|name| (name.as_str(), b"{}".as_slice())).collect();
    reject(
        with_declared_size(archive(&entries), 64 * 1024 * 1024),
        "Expanded backup exceeds 512 MiB",
    );
}

#[test]
fn rejects_more_than_ten_thousand_entries() {
    let names: Vec<_> = (0..9998).map(|index| format!("profiles/{index}.yaml")).collect();
    let entries: Vec<_> = names.iter().map(|name| (name.as_str(), b"".as_slice())).collect();
    reject(archive(&entries), "too many entries");
}

#[test]
fn database_failure_rolls_back_files_and_preserves_recovery_snapshot() {
    let home = Home::new();
    fs::write(home.0.join("config.yaml"), b"old").unwrap();
    let staged = StagedBackup::prepare(Cursor::new(archive(&[("clash5-nodes.sqlite", b"fixture")])), &home.0).unwrap();
    let error = staged
        .publish(&home.0, |_, previous| {
            fs::write(previous.join("clash5-nodes.sqlite"), b"previous database")?;
            anyhow::bail!("injected database failure")
        })
        .unwrap_err();
    assert!(error.to_string().contains("recovery snapshot retained"));
    assert_eq!(fs::read(home.0.join("config.yaml")).unwrap(), b"old");
    assert!(!home.0.join("verge.yaml").exists());
    assert!(!home.0.join("profiles.yaml").exists());
    let recovery = fs::read_dir(&home.0)
        .unwrap()
        .filter_map(Result::ok)
        .find(|entry| entry.file_type().unwrap().is_dir())
        .unwrap();
    assert_eq!(
        fs::read(recovery.path().join("previous/clash5-nodes.sqlite")).unwrap(),
        b"previous database"
    );
}

#[test]
fn sqlite_snapshot_restores_records_and_keeps_previous_database() {
    let home = Home::new();
    let previous = home.0.join("previous");
    fs::create_dir(&previous).unwrap();
    let snapshot = home.0.join("snapshot.sqlite");
    let destination = home.0.join("live.sqlite");
    let mut source = clash5_data::BindingStore::memory().unwrap();
    let node = clash5_data::Node {
        kind: "vless".into(),
        server: "synthetic.example.invalid".into(),
        port: 443,
        credentials: Default::default(),
        display_name: "Synthetic node".into(),
        transport: String::new(),
        sni: String::new(),
    };
    let id = source.upsert_manual(&node).unwrap();
    source.snapshot_to(&snapshot).unwrap();
    drop(clash5_data::BindingStore::open(&destination).unwrap());
    backup_archive::restore_database(&snapshot, &previous, &destination).unwrap();
    let restored = clash5_data::BindingStore::open(&destination).unwrap();
    restored.integrity_check().unwrap();
    assert_eq!(restored.node_records(false).unwrap()[0].stable_id, id);
    assert!(
        clash5_data::BindingStore::open(previous.join("clash5-nodes.sqlite"))
            .unwrap()
            .node_records(false)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn invalid_or_empty_database_does_not_create_destination() {
    let home = Home::new();
    let snapshot = home.0.join("snapshot.sqlite");
    let destination = home.0.join("live.sqlite");
    for bytes in [b"not a database".as_slice(), b""] {
        fs::write(&snapshot, bytes).unwrap();
        assert!(backup_archive::restore_database(&snapshot, &home.0, &destination).is_err());
        assert!(!destination.exists());
    }
}

#[cfg(unix)]
#[test]
fn rejects_existing_destination_symlink() {
    let home = Home::new();
    let outside = Home::new();
    fs::write(outside.0.join("config.yaml"), b"outside").unwrap();
    std::os::unix::fs::symlink(outside.0.join("config.yaml"), home.0.join("config.yaml")).unwrap();
    let staged = StagedBackup::prepare(Cursor::new(archive(&[])), &home.0).unwrap();
    assert!(staged.publish(&home.0, |_, _| Ok(())).is_err());
    assert_eq!(fs::read(outside.0.join("config.yaml")).unwrap(), b"outside");
}
