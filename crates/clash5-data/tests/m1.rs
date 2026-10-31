use clash5_data::{
    Binding, BindingStore, DataError, Node, NodeQuality, QualityStatus, Source, replace_subscription_lines,
};
use std::collections::BTreeMap;

fn node(name: &str) -> Node {
    Node {
        kind: "vless".into(),
        server: "edge.example.invalid".into(),
        port: 443,
        credentials: BTreeMap::from([("uuid".into(), "a".repeat(32))]),
        display_name: name.into(),
        transport: String::new(),
        sni: String::new(),
    }
}

#[test]
fn manual_batch_validates_every_identity_before_writing() {
    let mut db = BindingStore::memory().unwrap();
    let first = node("First");
    let mut invalid = node("Invalid");
    invalid.port = 0;
    assert_eq!(
        db.upsert_manual_batch(&[first.clone(), invalid], &[]),
        Err(DataError::InvalidInput)
    );
    assert!(db.node_records(false).unwrap().is_empty());

    let mut second = node("Second");
    second.credentials.insert("uuid".into(), "b".repeat(32));
    assert_eq!(db.upsert_manual_batch(&[first, second], &[]).unwrap().len(), 2);
    assert_eq!(db.node_records(false).unwrap().len(), 2);
}

#[test]
fn manual_rename_keeps_probe_cache_but_runtime_change_invalidates_it() {
    let mut db = BindingStore::memory().unwrap();
    let id = node("Original").stable_id().unwrap();
    db.upsert_manual_batch(&[node("Original")], std::slice::from_ref(&id))
        .unwrap();
    db.save_quality(
        &Source::Manual,
        &id,
        &NodeQuality::from_probe(Some(10), Some(20), 1_000),
    )
    .unwrap();

    db.upsert_manual_batch(&[node("Renamed")], &[]).unwrap();
    let renamed = db.node_records(false).unwrap().remove(0);
    assert_eq!(renamed.display_name, "Renamed");
    assert_eq!(renamed.quality.unwrap().handshake_delay_ms, Some(20));

    db.upsert_manual_batch(&[node("Renamed")], &[id]).unwrap();
    assert!(db.node_records(false).unwrap().remove(0).quality.is_none());
}

#[test]
fn remote_refresh_replaces_only_lines() {
    let old = "proxies:\n  - {name: Old, type: vless, server: edge.example.invalid, port: 443, uuid: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa}\nproxy-providers:\n  old: {type: file, path: old.yaml}\nrules:\n  - DOMAIN,example.invalid,DIRECT\nproxy-groups:\n  - {name: User, type: select, proxies: [Old]}\nmode: rule\n";
    let incoming = "proxies:\n  - {name: New, type: vless, server: edge.example.invalid, port: 443, uuid: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa}\nrules:\n  - MATCH,REJECT\nmode: global\n";
    let result = replace_subscription_lines(old, incoming).unwrap();
    let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&result).unwrap();
    assert_eq!(yaml["proxies"][0]["name"], "New");
    assert!(yaml.get("proxy-providers").is_none());
    assert_eq!(yaml["rules"][0], "DOMAIN,example.invalid,DIRECT");
    assert_eq!(yaml["proxy-groups"][0]["name"], "User");
    assert_eq!(yaml["mode"], "rule");
    assert!(replace_subscription_lines(old, "rules: []\n").is_err());
}

#[test]
fn duplicate_fingerprint_has_deterministic_name_and_preserves_scope() {
    let mut db = BindingStore::memory().unwrap();
    let id = node("Alpha").stable_id().unwrap();
    let diff = db.refresh_provider("a", &[node("Zulu"), node("Alpha")]).unwrap();
    assert_eq!(diff.duplicates, 1);
    db.refresh_provider("b", &[node("Elsewhere")]).unwrap();
    db.upsert_manual(&node("Manual")).unwrap();
    let records = db.node_records(false).unwrap();
    assert_eq!(records.len(), 3);
    assert!(records.iter().all(|record| record.stable_id == id));
    let a = records
        .iter()
        .find(|record| {
            record.source
                == Source::Subscription {
                    provider_id: "a".into(),
                }
        })
        .unwrap();
    assert_eq!(a.display_name, "Alpha");
    assert_eq!(a.duplicate_count, 2);
    assert_eq!(a.kind, "vless");
    assert_eq!(a.server, "edge.example.invalid");
    db.refresh_provider("a", &[node("New name")]).unwrap();
    let updated = db.node_records(false).unwrap();
    let a = updated
        .iter()
        .find(|record| {
            record.source
                == Source::Subscription {
                    provider_id: "a".into(),
                }
        })
        .unwrap();
    assert_eq!(a.display_name, "New name");
    assert_eq!(a.duplicate_count, 1);
    db.refresh_provider("a", &[]).unwrap();
    assert_eq!(db.node_records(false).unwrap().len(), 2);
    assert_eq!(db.node_records(true).unwrap().len(), 3);
}

#[test]
fn v1_database_migrates_without_discarding_nodes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-data");
    std::fs::create_dir_all(&root).unwrap();
    let dir = tempfile::tempdir_in(root).unwrap();
    let path = dir.path().join("v1.sqlite");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE nodes(source_kind TEXT, provider_id TEXT, stable_id TEXT, display_name TEXT, active INTEGER); CREATE TABLE quality_cache(source_kind TEXT, provider_id TEXT, stable_id TEXT, quality TEXT); INSERT INTO nodes VALUES('manual','','v1:old','Old',1); PRAGMA user_version=1;").unwrap();
    drop(db);
    let store = BindingStore::open(&path).unwrap();
    let record = store.node_records(false).unwrap().remove(0);
    assert_eq!(record.stable_id, "v1:old");
    assert_eq!(record.display_name, "Old");
    assert_eq!(record.duplicate_count, 1);
    let migrated = rusqlite::Connection::open(&path).unwrap();
    let version: i64 = migrated
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 10);
    let columns: Vec<String> = migrated
        .prepare("PRAGMA table_info(nodes)")
        .unwrap()
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(columns.iter().any(|column| column == "stable_id_version"));
    assert!(columns.iter().any(|column| column == "key_generation"));
    assert!(columns.iter().any(|column| column == "transport"));
    assert!(columns.iter().any(|column| column == "sni"));
    let retention: String = migrated
        .query_row(
            "SELECT setting_value FROM app_settings WHERE setting_key='orphan_retention_days'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retention, "30");
    let journal_mode: String = migrated
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .unwrap();
    assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
}

#[test]
fn orphan_retention_is_configurable_and_expired_cleanup_preserves_recent_bindings() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-data");
    std::fs::create_dir_all(&root).unwrap();
    let dir = tempfile::tempdir_in(root).unwrap();
    let path = dir.path().join("orphan-retention.sqlite");
    let source = Source::Subscription {
        provider_id: "retention-provider".into(),
    };
    let mut store = BindingStore::open(&path).unwrap();
    let id = node("retention-node").stable_id().unwrap();
    store
        .refresh_provider("retention-provider", &[node("retention-node")])
        .unwrap();
    for binding_id in ["expired", "recent"] {
        store
            .upsert_binding(&Binding {
                id: binding_id.into(),
                plugin_id: "fixture-plugin".into(),
                target_id: id.clone(),
                source: source.clone(),
                priority: 0,
                enabled: true,
                config: serde_json::json!({}),
                orphan: false,
                disappeared_at: None,
            })
            .unwrap();
    }
    store.refresh_provider("retention-provider", &[]).unwrap();
    assert_eq!(store.orphan_retention_days().unwrap(), 30);
    assert!(store.set_orphan_retention_days(0).is_err());
    assert!(store.set_orphan_retention_days(366).is_err());
    drop(store);

    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "UPDATE bindings SET disappeared_at=CASE id WHEN 'expired' THEN strftime('%Y-%m-%dT%H:%M:%fZ','now','-31 days') ELSE strftime('%Y-%m-%dT%H:%M:%fZ','now','-1 day') END",
        [],
    )
    .unwrap();
    drop(db);

    let mut store = BindingStore::open(&path).unwrap();
    let preview = store.orphan_cleanup_preview().unwrap();
    assert_eq!(preview.retention_days, 30);
    assert_eq!(preview.total_orphans, 2);
    assert_eq!(preview.eligible_orphans, 1);
    store.set_orphan_retention_days(7).unwrap();
    let preview = store.orphan_cleanup_preview().unwrap();
    assert_eq!(preview.retention_days, 7);
    assert_eq!(preview.eligible_orphans, 1);
    assert_eq!(store.purge_expired_orphans(Some(&source)).unwrap(), 1);
    let remaining = store.bindings(Some(&source)).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, "recent");
    assert!(remaining[0].orphan);
}

#[test]
fn v2_plugin_migration_commits_before_identity_migration_rolls_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rollback.sqlite");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TABLE nodes(source_kind TEXT, provider_id TEXT, stable_id TEXT, display_name TEXT, kind TEXT, server TEXT, port INTEGER, duplicate_count INTEGER, active INTEGER);
         CREATE TABLE identity_keys(generation INTEGER);
         INSERT INTO nodes VALUES('manual','','v1:old','Old','ss','edge.example',443,1,1);
         PRAGMA user_version=2;",
    ).unwrap();
    drop(db);
    assert!(BindingStore::open(&path).is_err());
    let db = rusqlite::Connection::open(&path).unwrap();
    let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
    assert_eq!(version, 3);
    let columns: Vec<String> = db
        .prepare("PRAGMA table_info(nodes)")
        .unwrap()
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(!columns.iter().any(|column| column == "stable_id_version"));
    let plugin_table_exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='plugin_bindings')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(plugin_table_exists);
    let id: String = db
        .query_row("SELECT stable_id FROM nodes", [], |row| row.get(0))
        .unwrap();
    assert_eq!(id, "v1:old");
}

#[test]
fn quality_is_source_scoped_and_obeys_ten_minute_throttle() {
    let mut db = BindingStore::memory().unwrap();
    let id = node("One").stable_id().unwrap();
    db.refresh_provider("a", &[node("One")]).unwrap();
    db.refresh_provider("b", &[node("Two")]).unwrap();
    let quality = NodeQuality {
        status: QualityStatus::Slow,
        short_delay_ms: None,
        handshake_delay_ms: Some(5100),
        checked_at_ms: 1000,
        failure_streak: 0,
    };
    db.save_quality(
        &Source::Subscription {
            provider_id: "a".into(),
        },
        &id,
        &quality,
    )
    .unwrap();
    let records = db.node_records(false).unwrap();
    assert_eq!(records.iter().filter(|record| record.quality.is_some()).count(), 1);
    assert!(quality.is_fresh(600_999));
    assert!(!quality.is_fresh(601_000));
    assert!(!quality.is_fresh(999));
    assert_eq!(
        NodeQuality::from_probe(Some(50), Some(180), 0).status,
        QualityStatus::Healthy
    );
    assert_eq!(NodeQuality::from_probe(None, Some(5100), 0).status, QualityStatus::Slow);
    assert_eq!(NodeQuality::from_probe(Some(50), None, 0).status, QualityStatus::Failed);
}

#[test]
fn consecutive_probe_failures_back_off_and_success_resets_interval() {
    let first = NodeQuality::from_probe(None, None, 1_000);
    assert_eq!(first.failure_streak, 1);
    assert!(first.is_fresh(1_200_999));
    assert!(!first.is_fresh(1_201_000));

    let second = NodeQuality::from_probe_after(None, None, 1_201_000, Some(&first));
    assert_eq!(second.failure_streak, 2);
    assert!(second.is_fresh(3_600_999));
    assert!(!second.is_fresh(3_601_000));

    let third = NodeQuality::from_probe_after(None, None, 3_601_000, Some(&second));
    assert_eq!(third.failure_streak, 3);
    assert!(third.is_fresh(7_200_999));
    assert!(!third.is_fresh(7_201_000));

    let recovered = NodeQuality::from_probe_after(Some(42), Some(100), 7_201_000, Some(&third));
    assert_eq!(recovered.failure_streak, 0);
    assert!(!recovered.is_fresh(7_801_000));
}

#[test]
fn quality_cache_respects_performance_interval() {
    let quality = NodeQuality::from_probe(Some(45), Some(120), 1_000);
    assert!(quality.is_fresh_with_interval(300_999, 300_000));
    assert!(!quality.is_fresh_with_interval(301_000, 300_000));
    assert!(quality.is_fresh_with_interval(1_200_999, 1_200_000));
    assert!(!quality.is_fresh_with_interval(1_201_000, 1_200_000));
}

#[test]
fn older_quality_cache_without_failure_streak_still_deserializes() {
    let old = r#"{"status":"failed","short_delay_ms":null,"handshake_delay_ms":null,"checked_at_ms":1000}"#;
    let quality: NodeQuality = serde_json::from_str(old).unwrap();
    assert_eq!(quality.failure_streak, 0);
    assert!(quality.is_fresh(600_999));
}
