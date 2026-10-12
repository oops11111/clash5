use clash5_data::{BindingStore, Node, NodeQuality, QualityStatus, Source, replace_subscription_lines};
use std::collections::BTreeMap;

fn node(name: &str) -> Node {
    Node {
        kind: "vless".into(),
        server: "edge.example.invalid".into(),
        port: 443,
        credentials: BTreeMap::from([("uuid".into(), "a".repeat(32))]),
        display_name: name.into(),
    }
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
