use clash5_data::{
    Binding, BindingStore, DataError, ExitPolicy, IdentityKey, IdentitySourceSnapshot, Node, NodeQuality, NodeRef,
    ProxyChain, Source, parse_inline_nodes,
};
use serde_json::json;
use std::collections::BTreeMap;

fn node() -> Node {
    Node {
        kind: "vless".into(),
        server: "edge.example.invalid".into(),
        port: 443,
        credentials: BTreeMap::new(),
        display_name: "Synthetic node".into(),
        transport: String::new(),
        sni: String::new(),
    }
}

#[test]
fn backup_snapshot_restores_committed_wal_data() {
    let dir = tempfile::tempdir().unwrap();
    let live_path = dir.path().join("live.sqlite");
    let snapshot_path = dir.path().join("snapshot.sqlite");
    let restored_path = dir.path().join("restored.sqlite");
    let mut live = BindingStore::open(&live_path).unwrap();
    let stable_id = live.upsert_manual(&node()).unwrap();
    assert!(live_path.with_extension("sqlite-wal").exists());

    live.snapshot_to(&snapshot_path).unwrap();
    let mut restored = BindingStore::open(&restored_path).unwrap();
    restored.restore_from(&snapshot_path).unwrap();
    let records = restored.node_records(false).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].stable_id, stable_id);
}
#[test]
fn identity_snapshot_matches_typescript_v1() {
    assert_eq!(
        node().stable_id().unwrap(),
        "v1:0cbcc98763a4f3d14ee0367690689111a9e8f66a05481423541a670ff3373f3a"
    );
}

#[test]
fn v2_identity_is_keyed_and_covers_transport_and_sni() {
    let key = IdentityKey::from_bytes(&[7; 32]).unwrap();
    let mut original = node();
    original
        .credentials
        .insert("uuid".into(), "550e8400-e29b-41d4-a716-446655440000".into());
    let id = original.stable_id_v2(&key).unwrap();
    assert_eq!(
        id,
        "v2:89dc04c99edf6143fa3e03e263290a44c8d1aee7a3087cb3e1cfaaa6cd9a8301"
    );
    assert!(id.starts_with("v2:") && id.len() == 67);
    assert_ne!(
        id,
        original
            .stable_id_v2(&IdentityKey::from_bytes(&[8; 32]).unwrap())
            .unwrap()
    );

    let mut explicit_tcp = original.clone();
    explicit_tcp.transport = "TCP".into();
    assert_eq!(id, explicit_tcp.stable_id_v2(&key).unwrap());
    let mut websocket = original.clone();
    websocket.transport = "ws".into();
    assert_ne!(id, websocket.stable_id_v2(&key).unwrap());
    let mut other_sni = original;
    other_sni.sni = "cdn.example.invalid".into();
    assert_ne!(id, other_sni.stable_id_v2(&key).unwrap());
}

#[test]
fn identity_key_id_is_non_secret_and_stable() {
    let key = IdentityKey::from_bytes(&[42; 32]).unwrap();
    assert_eq!(key.key_id().len(), 32);
    assert!(
        key.key_id()
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
    assert_eq!(IdentityKey::from_bytes(&[42; 32]).unwrap().key_id(), key.key_id());
}

#[test]
fn planned_key_rotation_moves_exact_snapshots_and_preserves_unresolved_orphans() {
    let old_key = IdentityKey::from_bytes(&[11; 32]).unwrap();
    let new_key = IdentityKey::from_bytes(&[12; 32]).unwrap();
    let mut store = BindingStore::memory_with_identity(old_key.clone()).unwrap();
    let node = node();
    let source_a = Source::Subscription {
        provider_id: "provider-a".into(),
    };
    let source_b = Source::Subscription {
        provider_id: "provider-b".into(),
    };
    let old_a = store
        .refresh_provider("provider-a", std::slice::from_ref(&node))
        .unwrap()
        .added
        .remove(0);
    let old_b = store
        .refresh_provider("provider-b", std::slice::from_ref(&node))
        .unwrap()
        .added
        .remove(0);
    bind(&mut store, source_a.clone(), &old_a, true);
    bind(&mut store, source_b.clone(), &old_b, false);
    store
        .save_quality(&source_a, &old_a, &NodeQuality::from_probe(Some(12), Some(24), 120))
        .unwrap();

    let generation = store
        .rotate_identity_key(
            new_key.clone(),
            &[IdentitySourceSnapshot {
                source: source_a.clone(),
                nodes: vec![node.clone()],
            }],
        )
        .unwrap();
    assert_eq!(generation, 2);
    let new_a = node.stable_id_v2(&new_key).unwrap();
    assert_ne!(old_a, new_a);
    let migrated = store.bindings(Some(&source_a)).unwrap().remove(0);
    assert_eq!(migrated.target_id, new_a);
    assert!(migrated.enabled);
    assert!(!migrated.orphan);
    assert!(
        store
            .node_records(false)
            .unwrap()
            .iter()
            .any(|record| { record.source == source_a && record.stable_id == new_a && record.quality.is_some() })
    );

    let unresolved = store.bindings(Some(&source_b)).unwrap().remove(0);
    assert_eq!(unresolved.target_id, old_b);
    assert!(!unresolved.enabled);
    assert!(unresolved.orphan);
    assert!(
        store
            .node_records(false)
            .unwrap()
            .iter()
            .all(|record| record.source != source_b)
    );
    assert!(
        store
            .node_records(true)
            .unwrap()
            .iter()
            .any(|record| { record.source == source_b && record.stable_id == old_b && !record.active })
    );
    assert_eq!(store.active_identity_key().unwrap().unwrap().0, generation);
    assert_eq!(store.identity_key().unwrap().key_id(), new_key.key_id());
}

#[test]
fn planned_key_rotation_rekeys_saved_chain_references_atomically() {
    let old_key = IdentityKey::from_bytes(&[13; 32]).unwrap();
    let new_key = IdentityKey::from_bytes(&[14; 32]).unwrap();
    let mut store = BindingStore::memory_with_identity(old_key.clone()).unwrap();
    let manual_node = node();
    let mut subscription_node = node();
    subscription_node.port = 8443;
    let manual_old_id = manual_node.stable_id_v2(&old_key).unwrap();
    let subscription_old_id = subscription_node.stable_id_v2(&old_key).unwrap();
    store
        .upsert_manual_batch(std::slice::from_ref(&manual_node), &[])
        .unwrap();
    store
        .refresh_provider("provider-a", std::slice::from_ref(&subscription_node))
        .unwrap();
    let chain = ProxyChain {
        schema_version: 2,
        id: "18dc0b8a-7485-49bc-8a91-f48bdc736846".into(),
        name: "Rotation chain".into(),
        profile_id: Some("profile-a".into()),
        hops: vec![
            clash5_data::Hop::Node {
                node: NodeRef {
                    source: Source::Manual,
                    stable_id: manual_old_id.clone(),
                },
            },
            clash5_data::Hop::Node {
                node: NodeRef {
                    source: Source::Subscription {
                        provider_id: "provider-a".into(),
                    },
                    stable_id: subscription_old_id.clone(),
                },
            },
        ],
        enabled: true,
        exit_policy: ExitPolicy::Warn,
    };
    store.save_proxy_chain(&chain).unwrap();

    store
        .rotate_identity_key(
            new_key.clone(),
            &[
                IdentitySourceSnapshot {
                    source: Source::Manual,
                    nodes: vec![manual_node.clone()],
                },
                IdentitySourceSnapshot {
                    source: Source::Subscription {
                        provider_id: "provider-a".into(),
                    },
                    nodes: vec![subscription_node.clone()],
                },
            ],
        )
        .unwrap();

    let chains = store.proxy_chains().unwrap();
    assert_eq!(chains.len(), 1);
    let clash5_data::Hop::Node { node: front } = &chains[0].hops[0] else {
        panic!("first hop is a node")
    };
    let clash5_data::Hop::Node { node: landing } = &chains[0].hops[1] else {
        panic!("second hop is a node")
    };
    assert_eq!(front.source, Source::Manual);
    assert_eq!(front.stable_id, manual_node.stable_id_v2(&new_key).unwrap());
    assert_eq!(
        landing.source,
        Source::Subscription {
            provider_id: "provider-a".into(),
        }
    );
    assert_eq!(landing.stable_id, subscription_node.stable_id_v2(&new_key).unwrap());
}

#[test]
fn planned_key_rotation_rolls_back_on_identity_collision() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rotation-collision.sqlite");
    let old_key = IdentityKey::from_bytes(&[31; 32]).unwrap();
    let new_key = IdentityKey::from_bytes(&[32; 32]).unwrap();
    let node = node();
    let source = Source::Subscription {
        provider_id: "provider-a".into(),
    };
    let old_id;
    {
        let mut store = BindingStore::open_with_identity(&path, old_key.clone()).unwrap();
        old_id = store
            .refresh_provider("provider-a", std::slice::from_ref(&node))
            .unwrap()
            .added
            .remove(0);
        bind(&mut store, source.clone(), &old_id, true);
    }
    let new_id = node.stable_id_v2(&new_key).unwrap();
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute(
        "INSERT INTO nodes(source_kind,provider_id,stable_id,display_name,active,kind,server,port,duplicate_count,stable_id_version,key_generation,transport,sni,identity_context_known) VALUES('subscription','provider-a',?1,'collision',0,'vless','edge.example.invalid',443,1,2,2,'tcp','',1)",
        [&new_id],
    )
    .unwrap();
    drop(raw);

    let mut store = BindingStore::open_with_identity(&path, old_key.clone()).unwrap();
    assert_eq!(
        store.rotate_identity_key(
            new_key,
            &[IdentitySourceSnapshot {
                source: source.clone(),
                nodes: vec![node],
            }],
        ),
        Err(DataError::Database)
    );
    assert_eq!(store.active_identity_key().unwrap().unwrap().0, 1);
    assert_eq!(store.identity_key().unwrap().key_id(), old_key.key_id());
    let binding = store.bindings(Some(&source)).unwrap().remove(0);
    assert_eq!(binding.target_id, old_id);
    assert!(!binding.orphan);
}

#[test]
fn planned_rotation_migrates_exact_legacy_v1_rows_and_orphans_missing_definitions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("legacy-rotation.sqlite");
    let node = node();
    let source_a = Source::Subscription {
        provider_id: "provider-a".into(),
    };
    let source_b = Source::Subscription {
        provider_id: "provider-b".into(),
    };
    let old_a;
    let old_b;
    {
        let mut store = BindingStore::open(&path).unwrap();
        old_a = store
            .refresh_provider("provider-a", std::slice::from_ref(&node))
            .unwrap()
            .added
            .remove(0);
        old_b = store
            .refresh_provider("provider-b", std::slice::from_ref(&node))
            .unwrap()
            .added
            .remove(0);
        bind(&mut store, source_a.clone(), &old_a, true);
        bind(&mut store, source_b.clone(), &old_b, true);
    }
    let old_key = IdentityKey::from_bytes(&[41; 32]).unwrap();
    let new_key = IdentityKey::from_bytes(&[42; 32]).unwrap();
    let mut store = BindingStore::open_with_identity(&path, old_key).unwrap();
    store
        .rotate_identity_key(
            new_key.clone(),
            &[IdentitySourceSnapshot {
                source: source_a.clone(),
                nodes: vec![node.clone()],
            }],
        )
        .unwrap();

    let new_a = node.stable_id_v2(&new_key).unwrap();
    let migrated = store.bindings(Some(&source_a)).unwrap().remove(0);
    assert_eq!(migrated.target_id, new_a);
    assert!(!migrated.orphan);
    let unavailable = store.bindings(Some(&source_b)).unwrap().remove(0);
    assert_eq!(unavailable.target_id, old_b);
    assert!(unavailable.orphan);
    assert_ne!(old_a, new_a);
}

#[test]
fn keyed_store_persists_only_key_identifier_and_fails_closed_on_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.sqlite");
    let key_bytes = [0xA7; 32];
    let key = IdentityKey::from_bytes(&key_bytes).unwrap();
    let expected_key_id = key.key_id();
    let mut store = BindingStore::open_with_identity(&path, key).unwrap();
    let mut private_node = node();
    private_node
        .credentials
        .insert("password".into(), "synthetic-never-persist-this".into());
    let id = store
        .refresh_provider("provider-a", &[private_node])
        .unwrap()
        .added
        .remove(0);
    assert!(id.starts_with("v2:"));
    drop(store);

    let db = rusqlite::Connection::open(&path).unwrap();
    let (stored_id, version, generation): (String, i64, i64) = db.query_row(
        "SELECT k.key_id,n.stable_id_version,n.key_generation FROM identity_keys k JOIN nodes n ON n.key_generation=k.generation",
        [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).unwrap();
    assert_eq!(stored_id, expected_key_id);
    assert_eq!(version, 2);
    assert_eq!(generation, 1);
    drop(db);
    let bytes = std::fs::read(&path).unwrap();
    assert!(!bytes.windows(key_bytes.len()).any(|window| window == key_bytes));
    let credential = b"synthetic-never-persist-this";
    assert!(!bytes.windows(credential.len()).any(|window| window == credential));

    let mismatch = BindingStore::open_with_identity(&path, IdentityKey::from_bytes(&[4; 32]).unwrap());
    assert_eq!(mismatch.err(), Some(DataError::IdentityRecovery));
    let readable = BindingStore::open(&path).unwrap();
    assert_eq!(
        readable
            .nodes(&Source::Subscription {
                provider_id: "provider-a".into()
            })
            .unwrap(),
        vec![id]
    );
}

#[test]
fn subscription_parser_captures_transport_and_sni_and_rejects_conflicts() {
    let node = parse_inline_nodes(
        "proxies:\n  - {name: test, type: ss, server: EDGE.example.invalid, port: 8388, cipher: aes-128-gcm, password: secret, network: ws, tls: {servername: CDN.example.invalid}, sni: cdn.example.invalid}\n",
    )
    .unwrap()
    .unwrap()
    .remove(0);
    assert_eq!(node.transport, "ws");
    assert_eq!(node.sni, "cdn.example.invalid");
    assert!(parse_inline_nodes(
        "proxies:\n  - {name: test, type: ss, server: edge.example.invalid, port: 8388, cipher: aes-128-gcm, password: secret, sni: a.example.invalid, servername: b.example.invalid}\n",
    )
    .is_err());
}

#[test]
fn exact_v1_refresh_migrates_node_binding_and_quality_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v1-to-v2.sqlite");
    let current = node();
    let old_id;
    {
        let mut old = BindingStore::open(&path).unwrap();
        old_id = old
            .refresh_provider("provider-a", std::slice::from_ref(&current))
            .unwrap()
            .added
            .remove(0);
        old.upsert_binding(&Binding {
            id: "plugin-binding".into(),
            plugin_id: "plugin-a".into(),
            target_id: old_id.clone(),
            source: Source::Subscription {
                provider_id: "provider-a".into(),
            },
            priority: 0,
            enabled: true,
            config: json!({}),
            orphan: false,
            disappeared_at: None,
        })
        .unwrap();
        old.save_quality(
            &Source::Subscription {
                provider_id: "provider-a".into(),
            },
            &old_id,
            &NodeQuality::from_probe(Some(10), Some(20), 100),
        )
        .unwrap();
    }
    let mut current_store =
        BindingStore::open_with_identity(&path, IdentityKey::from_bytes(&[2; 32]).unwrap()).unwrap();
    let new_id = current_store
        .refresh_provider("provider-a", &[current])
        .unwrap()
        .added
        .remove(0);
    assert_ne!(old_id, new_id);
    assert_eq!(
        current_store
            .nodes(&Source::Subscription {
                provider_id: "provider-a".into()
            })
            .unwrap(),
        vec![new_id.clone()]
    );
    let binding = current_store.bindings(None).unwrap().remove(0);
    assert_eq!(binding.target_id, new_id);
    assert!(!binding.orphan);
    assert!(current_store.node_records(true).unwrap()[0].quality.is_some());
}

#[test]
fn credential_rotation_rebinds_only_unique_source_scoped_candidates() {
    let mut store = BindingStore::memory_with_identity(IdentityKey::from_bytes(&[6; 32]).unwrap()).unwrap();
    let source_a = Source::Subscription {
        provider_id: "provider-a".into(),
    };
    let mut old = node();
    old.credentials
        .insert("uuid".into(), "11111111-1111-1111-1111-111111111111".into());
    let old_id = store
        .refresh_provider("provider-a", &[old.clone()])
        .unwrap()
        .added
        .remove(0);
    store
        .upsert_binding(&Binding {
            id: "unique-binding".into(),
            plugin_id: "plugin".into(),
            target_id: old_id.clone(),
            source: source_a.clone(),
            priority: 0,
            enabled: true,
            config: json!({}),
            orphan: false,
            disappeared_at: None,
        })
        .unwrap();
    store.refresh_provider("provider-a", &[]).unwrap();
    let mut rotated = old.clone();
    rotated
        .credentials
        .insert("uuid".into(), "22222222-2222-2222-2222-222222222222".into());
    let new_id = store
        .refresh_provider("provider-a", &[rotated])
        .unwrap()
        .added
        .remove(0);
    let unique = store.bindings(Some(&source_a)).unwrap().remove(0);
    assert_eq!(unique.target_id, new_id);
    assert!(!unique.orphan);
    assert!(store.pending_identity_rebinds(Some(&source_a)).unwrap().is_empty());

    let source_b = Source::Subscription {
        provider_id: "provider-b".into(),
    };
    let mut first = node();
    first
        .credentials
        .insert("uuid".into(), "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".into());
    let mut second = node();
    second
        .credentials
        .insert("uuid".into(), "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb".into());
    let ids = store
        .refresh_provider("provider-b", &[first.clone(), second.clone()])
        .unwrap()
        .added;
    for (index, id) in ids.iter().enumerate() {
        store
            .upsert_binding(&Binding {
                id: format!("ambiguous-{index}"),
                plugin_id: "plugin".into(),
                target_id: id.clone(),
                source: source_b.clone(),
                priority: index as i64,
                enabled: true,
                config: json!({}),
                orphan: false,
                disappeared_at: None,
            })
            .unwrap();
    }
    store.refresh_provider("provider-b", &[]).unwrap();
    let mut third = first;
    third
        .credentials
        .insert("uuid".into(), "cccccccc-cccc-cccc-cccc-cccccccccccc".into());
    let third_id = store.refresh_provider("provider-b", &[third]).unwrap().added.remove(0);
    let pending = store.pending_identity_rebinds(Some(&source_b)).unwrap();
    assert_eq!(pending.len(), 2);
    assert!(pending.iter().all(|item| item.candidate_ids == vec![third_id.clone()]));
    assert!(
        store
            .bindings(Some(&source_b))
            .unwrap()
            .iter()
            .all(|binding| binding.orphan)
    );
    store
        .resolve_identity_rebind(&source_b, &pending[0].old_stable_id, &third_id)
        .unwrap();
    assert!(
        store
            .bindings(Some(&source_b))
            .unwrap()
            .iter()
            .any(|binding| binding.target_id == third_id && !binding.orphan)
    );
}

#[test]
fn missing_key_keeps_data_readable_and_explicit_reset_preserves_orphaned_bindings() {
    let mut store = BindingStore::memory_with_identity(IdentityKey::from_bytes(&[1; 32]).unwrap()).unwrap();
    let source = Source::Subscription {
        provider_id: "restore-provider".into(),
    };
    let current = node();
    let old_id = store
        .refresh_provider("restore-provider", std::slice::from_ref(&current))
        .unwrap()
        .added
        .remove(0);
    store
        .upsert_binding(&Binding {
            id: "retain-me".into(),
            plugin_id: "plugin".into(),
            target_id: old_id.clone(),
            source: source.clone(),
            priority: 0,
            enabled: true,
            config: json!({}),
            orphan: false,
            disappeared_at: None,
        })
        .unwrap();
    store.enter_identity_recovery();
    assert!(store.identity_recovery());
    assert_eq!(store.nodes(&source).unwrap(), vec![old_id.clone()]);
    assert_eq!(
        store.upsert_binding(&Binding {
            id: "blocked".into(),
            plugin_id: "plugin".into(),
            target_id: old_id.clone(),
            source: source.clone(),
            priority: 0,
            enabled: true,
            config: json!({}),
            orphan: false,
            disappeared_at: None
        }),
        Err(DataError::IdentityRecovery)
    );
    let generation = store
        .reset_identity_key(IdentityKey::from_bytes(&[5; 32]).unwrap())
        .unwrap();
    assert_eq!(generation, 2);
    let binding = store.bindings(Some(&source)).unwrap().remove(0);
    assert_eq!(binding.target_id, old_id);
    assert!(binding.orphan);
    let new_id = store
        .refresh_provider("restore-provider", &[current])
        .unwrap()
        .added
        .remove(0);
    assert_ne!(new_id, binding.target_id);
    let retained = store.bindings(Some(&source)).unwrap().remove(0);
    assert_eq!(retained.target_id, binding.target_id);
    assert!(retained.orphan);
    assert!(store.pending_identity_rebinds(Some(&source)).unwrap().is_empty());
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
fn malformed_snapshot_keeps_previous_state_and_duplicate_is_folded() {
    let mut db = BindingStore::memory().unwrap();
    let first = node();
    let id = first.stable_id().unwrap();
    db.refresh_provider("a", std::slice::from_ref(&first)).unwrap();
    let folded = db.refresh_provider("a", &[first.clone(), first.clone()]).unwrap();
    assert_eq!(folded.duplicates, 1);
    assert_eq!(db.node_records(false).unwrap()[0].duplicate_count, 2);
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

#[test]
fn sqlite_and_audit_do_not_contain_node_credentials() {
    let test_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-data");
    std::fs::create_dir_all(&test_root).unwrap();
    let directory = tempfile::tempdir_in(test_root).unwrap();
    let path = directory.path().join("private.sqlite");
    let mut first = node();
    let password = format!("synthetic-{}", "x".repeat(36));
    first.credentials.insert("password".into(), password.clone());
    let mut db = BindingStore::open(&path).unwrap();
    db.refresh_provider("profile-a", &[first]).unwrap();
    drop(db);
    let bytes = std::fs::read(path).unwrap();
    assert!(
        !bytes
            .windows(password.len())
            .any(|window| window == password.as_bytes())
    );
}
