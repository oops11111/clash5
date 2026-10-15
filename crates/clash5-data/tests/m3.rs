use clash5_data::{
    BindingStore, ExitPolicy, Node, PluginBinding, PluginBindingTarget, ProxyChain, Source, parse_proxy_node,
};
use serde_json::json;
use std::collections::BTreeMap;

fn node(name: &str, password: &str) -> Node {
    Node {
        kind: "http".into(),
        server: "edge.example.invalid".into(),
        port: 8443,
        credentials: BTreeMap::from([("password".into(), password.into())]),
        display_name: name.into(),
    }
}

fn chain(profile_id: Option<&str>) -> ProxyChain {
    let front: serde_yaml_ng::Value =
        serde_yaml_ng::from_str("name: front\ntype: http\nserver: front.example.invalid\nport: 8001\n").unwrap();
    let landing: serde_yaml_ng::Value =
        serde_yaml_ng::from_str("name: landing\ntype: http\nserver: landing.example.invalid\nport: 8002\n").unwrap();
    ProxyChain {
        schema_version: 1,
        id: "18dc0b8a-7485-49bc-8a91-f48bdc736846".into(),
        name: "Chain".into(),
        profile_id: profile_id.map(str::to_owned),
        front: clash5_data::NodeRef {
            source: Source::Manual,
            stable_id: parse_proxy_node(&front).unwrap().stable_id().unwrap(),
        },
        landing: clash5_data::NodeRef {
            source: Source::Manual,
            stable_id: parse_proxy_node(&landing).unwrap().stable_id().unwrap(),
        },
        enabled: true,
        exit_policy: ExitPolicy::Warn,
    }
}

fn binding(target: PluginBindingTarget) -> PluginBinding {
    PluginBinding {
        id: "binding-a".into(),
        plugin_id: "plugin-a".into(),
        target,
        priority: 80,
        enabled: true,
        config: json!({"mode": "safe"}),
        orphan: false,
        disappeared_at: None,
    }
}

#[test]
fn plugin_node_bindings_keep_provider_scope_and_follow_orphan_restore() {
    let mut store = BindingStore::memory().unwrap();
    let node_a = node("A", "a");
    let node_b = node("B", "b");
    let id_a = node_a.stable_id().unwrap();
    let id_b = node_b.stable_id().unwrap();
    store
        .refresh_provider("provider-a", std::slice::from_ref(&node_a))
        .unwrap();
    store.refresh_provider("provider-b", &[node_b]).unwrap();
    let target_a = PluginBindingTarget::Node {
        source: Source::Subscription {
            provider_id: "provider-a".into(),
        },
        stable_id: id_a.clone(),
    };
    let target_b = PluginBindingTarget::Node {
        source: Source::Subscription {
            provider_id: "provider-b".into(),
        },
        stable_id: id_b,
    };
    store.upsert_plugin_binding(&binding(target_a.clone())).unwrap();
    store.upsert_plugin_binding(&binding(target_b)).unwrap();

    store.refresh_provider("provider-a", &[]).unwrap();
    let orphan = store.plugin_bindings(Some(&target_a)).unwrap().remove(0);
    assert!(orphan.orphan);
    assert!(orphan.disappeared_at.is_some());

    store.refresh_provider("provider-a", &[node_a]).unwrap();
    let restored = store.plugin_bindings(Some(&target_a)).unwrap().remove(0);
    assert!(!restored.orphan);
    assert_eq!(restored.target, target_a);
    store.delete_plugin_binding(&target_a, "binding-a").unwrap();
    assert!(store.plugin_bindings(Some(&target_a)).unwrap().is_empty());
}

#[test]
fn chain_binding_is_profile_scoped_and_recovers_if_chain_is_restored() {
    let mut store = BindingStore::memory().unwrap();
    let chain = chain(Some("profile-a"));
    let chain_id = chain.id.clone();
    let target = PluginBindingTarget::Chain {
        chain_id: chain_id.clone(),
    };
    store.save_proxy_chain(&chain).unwrap();
    store.upsert_plugin_binding(&binding(target.clone())).unwrap();
    assert_eq!(store.plugin_bindings(Some(&target)).unwrap().len(), 1);

    store.delete_proxy_chain(&chain_id).unwrap();
    assert!(store.plugin_bindings(Some(&target)).unwrap()[0].orphan);

    store.save_proxy_chain(&chain).unwrap();
    assert!(!store.plugin_bindings(Some(&target)).unwrap()[0].orphan);
    store.delete_proxy_chain(&chain_id).unwrap();
    assert_eq!(store.purge_orphans(None).unwrap(), 1);
    assert!(store.plugin_bindings(Some(&target)).unwrap().is_empty());
}

#[test]
fn plugin_binding_batch_is_atomic_when_one_target_is_invalid() {
    let mut store = BindingStore::memory().unwrap();
    let node = node("A", "a");
    let stable_id = node.stable_id().unwrap();
    store.refresh_provider("provider-a", &[node]).unwrap();
    let valid_target = PluginBindingTarget::Node {
        source: Source::Subscription {
            provider_id: "provider-a".into(),
        },
        stable_id,
    };
    let missing_target = PluginBindingTarget::Chain {
        chain_id: "18dc0b8a-7485-49bc-8a91-f48bdc736847".into(),
    };

    assert!(
        store
            .upsert_plugin_bindings(&[binding(valid_target), binding(missing_target)])
            .is_err()
    );
    assert!(store.plugin_bindings(None).unwrap().is_empty());
}

#[test]
fn plugin_audit_records_metadata_without_binding_settings() {
    let mut store = BindingStore::memory().unwrap();
    store
        .record_plugin_audit(
            "plugin.install",
            &json!({"plugin_id": "plugin-a", "permissions": ["config.patch"], "signer_fingerprint": "a1b2"}),
        )
        .unwrap();

    let events = store.plugin_audit_events(10).unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event, "plugin.install");
    assert_eq!(events[0].details["plugin_id"], "plugin-a");
    assert!(events[0].details.get("config").is_none());
    assert!(store.record_plugin_audit("plugin.unknown", &json!({})).is_err());
}

#[test]
fn schema_v2_migrates_transactionally_to_plugin_bindings_v3() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-data");
    std::fs::create_dir_all(&root).unwrap();
    let directory = tempfile::tempdir_in(&root).unwrap();
    let path = directory.path().join("m2.sqlite");
    {
        let db = rusqlite::Connection::open(&path).unwrap();
        db.pragma_update(None, "user_version", 2).unwrap();
    }
    let store = BindingStore::open(&path).unwrap();
    drop(store);
    let db = rusqlite::Connection::open(&path).unwrap();
    let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
    let table_exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='plugin_bindings')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, 3);
    assert!(table_exists);

    let rollback_directory = tempfile::tempdir_in(&root).unwrap();
    let rollback_path = rollback_directory.path().join("m2.sqlite");
    {
        let db = rusqlite::Connection::open(&rollback_path).unwrap();
        db.execute_batch("CREATE TABLE blocker(value TEXT); CREATE INDEX plugin_bindings_target ON blocker(value);")
            .unwrap();
        db.pragma_update(None, "user_version", 2).unwrap();
    }
    assert!(BindingStore::open(&rollback_path).is_err());
    let db = rusqlite::Connection::open(&rollback_path).unwrap();
    let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
    let table_exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='plugin_bindings')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, 2);
    assert!(!table_exists);
}
