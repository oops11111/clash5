use clash5_data::{Binding, BindingStore, DataError, Node, Source, parse_inline_nodes};
use serde_json::json;
use std::collections::BTreeMap;

fn node() -> Node {
    Node {
        kind: "vless".into(),
        server: "edge.example.invalid".into(),
        port: 443,
        credentials: BTreeMap::new(),
        display_name: "Synthetic node".into(),
    }
}
#[test]
fn identity_snapshot_matches_typescript_v1() {
    assert_eq!(
        node().stable_id().unwrap(),
        "v1:0cbcc98763a4f3d14ee0367690689111a9e8f66a05481423541a670ff3373f3a"
    );
}
fn source(provider: &str) -> Source {
    Source::Subscription {
        provider_id: provider.into(),
    }
}
fn bind(db: &mut BindingStore, source: Source, id: &str, enabled: bool) {
    db.upsert_binding(&Binding {
        id: "binding".into(),
        plugin_id: "fixture".into(),
        target_id: id.into(),
        source,
        priority: 7,
        enabled,
        config: json!({}),
        orphan: false,
        disappeared_at: None,
    })
    .unwrap();
}

#[test]
fn identity_ignores_name_and_normalizes_credentials() {
    let mut first = node();
    first.credentials.insert("uuid".into(), "a".repeat(32));
    let id = first.stable_id().unwrap();
    first.display_name = "Renamed".into();
    first.kind = " VLESS ".into();
    first.server = " EDGE.EXAMPLE.INVALID. ".into();
    first
        .credentials
        .insert("uuid".into(), format!(" {{{}}} ", "A".repeat(32)));
    assert_eq!(id, first.stable_id().unwrap());
    first.port = 8443;
    assert_ne!(id, first.stable_id().unwrap());
    first.port = 443;
    first.credentials.insert("uuid".into(), "b".repeat(32));
    assert_ne!(id, first.stable_id().unwrap());
    first.credentials.insert("password".into(), "x".repeat(24));
    let password_id = first.stable_id().unwrap();
    first
        .credentials
        .insert("password".into(), format!(" {} ", "x".repeat(24)));
    assert_ne!(password_id, first.stable_id().unwrap());
}

#[test]
fn normalization_and_invalid_input() {
    let mut first = node();
    first.server = "[2001:0db8:0:0:0:0:0:1]".into();
    let id = first.stable_id().unwrap();
    first.server = "2001:db8::1".into();
    assert_eq!(id, first.stable_id().unwrap());
    first.server = "bücher.example.invalid".into();
    let id = first.stable_id().unwrap();
    first.server = "xn--bcher-kva.example.invalid".into();
    assert_eq!(id, first.stable_id().unwrap());
    for host in ["", "fe80::1%eth0", "https://example.invalid/profile"] {
        first.server = host.into();
        assert_eq!(first.stable_id(), Err(DataError::InvalidInput));
    }
}

#[test]
fn sources_and_provider_bindings_are_isolated() {
    let mut db = BindingStore::memory().unwrap();
    let first = node();
    let id = db.upsert_manual(&first).unwrap();
    bind(&mut db, Source::Manual, &id, true);
    db.refresh_provider("a", std::slice::from_ref(&first)).unwrap();
    db.refresh_provider("b", std::slice::from_ref(&first)).unwrap();
    bind(&mut db, source("a"), &id, true);
    bind(&mut db, source("b"), &id, false);
    db.save_chain("user-chain", &json!({"landing":id})).unwrap();
    db.refresh_provider("a", &[]).unwrap();
    assert_eq!(db.nodes(&Source::Manual).unwrap(), vec![id.clone()]);
    assert!(!db.bindings(Some(&Source::Manual)).unwrap()[0].orphan);
    assert!(!db.bindings(Some(&source("b"))).unwrap()[0].orphan);
    assert_eq!(db.chain("user-chain").unwrap(), Some(json!({"landing":id})));
    assert_eq!(db.bindings(None).unwrap().len(), 3);
}

#[test]
fn orphan_return_preserves_enabled_and_disabled_states() {
    for enabled in [true, false] {
        let mut db = BindingStore::memory().unwrap();
        let first = node();
        let id = first.stable_id().unwrap();
        db.refresh_provider("a", std::slice::from_ref(&first)).unwrap();
        bind(&mut db, source("a"), &id, enabled);
        assert_eq!(db.refresh_provider("a", &[]).unwrap().removed, vec![id.clone()]);
        let orphan = db.bindings(None).unwrap().remove(0);
        assert!(orphan.orphan);
        assert!(orphan.disappeared_at.is_some());
        assert_eq!(orphan.enabled, enabled);
        assert!(db.effective_bindings(&source("a"), &id).unwrap().is_empty());
        db.refresh_provider("a", &[]).unwrap();
        assert_eq!(db.bindings(None).unwrap()[0].disappeared_at, orphan.disappeared_at);
        let diff = db.refresh_provider("a", &[first]).unwrap();
        assert_eq!(diff.restored, vec![id.clone()]);
        let restored = db.bindings(None).unwrap().remove(0);
        assert!(!restored.orphan);
        assert_eq!(restored.enabled, enabled);
        assert!(restored.disappeared_at.is_none());
        assert_eq!(
            db.effective_bindings(&source("a"), &id).unwrap().len(),
            usize::from(enabled)
        );
    }
}

#[test]
fn rename_inherits_but_same_name_new_identity_does_not() {
    let mut db = BindingStore::memory().unwrap();
    let mut first = node();
    let id = first.stable_id().unwrap();
    db.refresh_provider("a", std::slice::from_ref(&first)).unwrap();
    bind(&mut db, source("a"), &id, true);
    first.display_name = "Renamed".into();
    assert_eq!(
        db.refresh_provider("a", std::slice::from_ref(&first)).unwrap().matched,
        vec![id.clone()]
    );
    assert!(!db.bindings(None).unwrap()[0].orphan);
    first.port = 8443;
    let new_id = first.stable_id().unwrap();
    db.refresh_provider("a", &[first]).unwrap();
    assert!(db.bindings(None).unwrap()[0].orphan);
    assert!(db.effective_bindings(&source("a"), &new_id).unwrap().is_empty());
}

#[test]
fn purge_is_filtered_and_permanent() {
    let mut db = BindingStore::memory().unwrap();
    let first = node();
    let id = first.stable_id().unwrap();
    for provider in ["a", "b"] {
        db.refresh_provider(provider, std::slice::from_ref(&first)).unwrap();
        bind(&mut db, source(provider), &id, true);
        db.refresh_provider(provider, &[]).unwrap();
    }
    assert_eq!(db.purge_orphans(Some(&Source::Manual)).unwrap(), 0);
    assert_eq!(db.purge_orphans(Some(&source("a"))).unwrap(), 1);
    db.refresh_provider("a", &[first]).unwrap();
    assert!(db.bindings(Some(&source("a"))).unwrap().is_empty());
    assert_eq!(db.purge_orphans(None).unwrap(), 1);
    assert!(db.bindings(None).unwrap().is_empty());
}

#[test]
fn database_reopen_and_atomic_failure() {
    let test_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-data");
    std::fs::create_dir_all(&test_root).unwrap();
    let directory = tempfile::tempdir_in(test_root).unwrap();
    let path = directory.path().join("store.sqlite");
    let first = node();
    let id = first.stable_id().unwrap();
    {
        let mut db = BindingStore::open(&path).unwrap();
        db.refresh_provider("a", std::slice::from_ref(&first)).unwrap();
        bind(&mut db, source("a"), &id, false);
    }
    let mut db = BindingStore::open(&path).unwrap();
    assert!(!db.bindings(None).unwrap()[0].enabled);
    let inspector = rusqlite::Connection::open(&path).unwrap();
    inspector.execute_batch("CREATE TRIGGER reject_refresh BEFORE INSERT ON audit_log WHEN NEW.event='provider.refresh' BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;").unwrap();
    assert_eq!(db.refresh_provider("a", &[]), Err(DataError::Database));
    assert_eq!(db.nodes(&source("a")).unwrap(), vec![id]);
    assert!(!db.bindings(None).unwrap()[0].orphan);
    inspector.execute_batch("DROP TRIGGER reject_refresh").unwrap();
    db.refresh_provider("a", &[]).unwrap();
    drop(db);
    let db = BindingStore::open(&path).unwrap();
    assert!(db.bindings(None).unwrap()[0].orphan);
    assert_eq!(
        inspector
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

#[test]
fn malformed_or_duplicate_snapshot_keeps_previous_state() {
    let mut db = BindingStore::memory().unwrap();
    let first = node();
    let id = first.stable_id().unwrap();
    db.refresh_provider("a", std::slice::from_ref(&first)).unwrap();
    assert_eq!(
        db.refresh_provider("a", &[first.clone(), first.clone()]),
        Err(DataError::InvalidInput)
    );
    let mut bad = first;
    bad.port = 0;
    assert_eq!(db.refresh_provider("a", &[bad]), Err(DataError::InvalidInput));
    assert_eq!(db.nodes(&source("a")).unwrap(), vec![id]);
}

#[test]
fn downloaded_inline_profile_drives_provider_diff_without_exposing_credentials() {
    let uuid = format!(
        "{}-{}-{}-{}-{}",
        "a".repeat(8),
        "a".repeat(4),
        "a".repeat(4),
        "a".repeat(4),
        "a".repeat(12)
    );
    let yaml = format!(
        "proxies:\n  - name: First\n    type: vless\n    server: edge.example.invalid\n    port: 443\n    uuid: {uuid}\n"
    );
    let nodes = parse_inline_nodes(&yaml).unwrap().unwrap();
    let mut db = BindingStore::memory().unwrap();
    let id = nodes[0].stable_id().unwrap();
    db.refresh_provider("profile-a", &nodes).unwrap();
    bind(&mut db, source("profile-a"), &id, true);
    let renamed = yaml.replace("First", "Renamed");
    let matched = db
        .refresh_provider("profile-a", &parse_inline_nodes(&renamed).unwrap().unwrap())
        .unwrap();
    assert_eq!(matched.matched, vec![id.clone()]);
    assert_eq!(db.bindings(None).unwrap()[0].target_id, id);
    let changed = yaml.replace("port: 443", "port: 8443");
    db.refresh_provider("profile-a", &parse_inline_nodes(&changed).unwrap().unwrap())
        .unwrap();
    assert!(db.bindings(None).unwrap()[0].orphan);
    db.refresh_provider("profile-a", &nodes).unwrap();
    assert!(!db.bindings(None).unwrap()[0].orphan);
}

#[test]
fn provider_only_profile_does_not_clear_inline_binding_snapshot() {
    assert!(
        parse_inline_nodes("proxy-providers:\n  provider: {type: http}\n")
            .unwrap()
            .is_none()
    );
    assert_eq!(parse_inline_nodes("proxies: []\n").unwrap().unwrap().len(), 0);
    assert!(matches!(
        parse_inline_nodes("proxies:\n  - {name: x, type: vless, server: a.example, port: 443}\n"),
        Err(DataError::InvalidInput)
    ));
}

#[test]
fn binding_ipc_source_and_draft_fields_are_stable() {
    let wire = json!({
        "id": "binding", "plugin_id": "fixture", "target_id": node().stable_id().unwrap(),
        "source": { "kind": "subscription", "provider_id": "profile-a" },
        "priority": 7, "enabled": true, "config": {}
    });
    let binding: Binding = serde_json::from_value(wire).unwrap();
    assert_eq!(binding.source, source("profile-a"));
    assert!(!binding.orphan);
    assert!(binding.disappeared_at.is_none());
    assert_eq!(
        serde_json::to_value(Source::Manual).unwrap(),
        json!({ "kind": "manual" })
    );
}
