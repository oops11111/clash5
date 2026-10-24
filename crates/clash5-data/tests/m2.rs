use clash5_data::{
    BindingStore, ChainError, ExitPolicy, Hop, NodeRef, ProxyChain, ScopedProxy, Source, compile_chains,
    parse_proxy_node,
};
use serde_yaml_ng::{Mapping, Value};

const CHAIN_ID: &str = "18dc0b8a-7485-49bc-8a91-f48bdc736846";

fn proxy(name: &str, port: u16) -> Mapping {
    serde_yaml_ng::from_str(&format!(
        "name: {name}\ntype: http\nserver: 127.0.0.1\nport: {port}\nusername: fixture\npassword: test-only-password\ndialer-proxy: user-override\n"
    ))
    .unwrap()
}

fn scoped(source: Source, name: &str, port: u16) -> ScopedProxy {
    let proxy = proxy(name, port);
    ScopedProxy {
        reference: NodeRef {
            source,
            stable_id: parse_proxy_node(&Value::Mapping(proxy.clone()))
                .unwrap()
                .stable_id()
                .unwrap(),
        },
        proxy,
    }
}

fn fixture() -> (ProxyChain, Vec<ScopedProxy>, Mapping) {
    let catalog = vec![
        scoped(Source::Manual, "Front", 18081),
        scoped(
            Source::Subscription {
                provider_id: "provider-a".into(),
            },
            "Landing",
            18082,
        ),
    ];
    let chain = ProxyChain {
        schema_version: 2,
        id: CHAIN_ID.into(),
        name: "Fixture chain".into(),
        profile_id: Some("profile-a".into()),
        hops: vec![
            Hop::Node {
                node: catalog[0].reference.clone(),
            },
            Hop::Node {
                node: catalog[1].reference.clone(),
            },
        ],
        enabled: true,
        exit_policy: ExitPolicy::Warn,
        pinned_exit_ip: None,
    };
    let config = serde_yaml_ng::from_str(
        "proxies:\n  - {name: original, type: http, server: 127.0.0.1, port: 18083, dialer-proxy: user-override}\nproxy-groups:\n  - {name: user-override, type: select, proxies: [DIRECT]}\nrules: ['MATCH,original']\n",
    )
    .unwrap();
    (chain, catalog, config)
}

#[test]
fn two_hop_snapshot_preserves_original_overrides_and_rules() {
    let (chain, catalog, mut config) = fixture();
    let original = config.clone();
    let compiled = compile_chains(&mut config, &[chain], &[], &catalog).unwrap();
    let runtime = &compiled[0];
    let proxies = config["proxies"].as_sequence().unwrap();
    assert_eq!(proxies[0], original["proxies"][0]);
    assert_eq!(config["rules"], original["rules"]);
    assert!(proxies[1].get("dialer-proxy").is_none());
    assert_eq!(proxies[1]["name"], runtime.first_hop_name);
    assert_eq!(proxies[2]["dialer-proxy"], runtime.first_hop_name);
    assert_eq!(proxies[2]["name"], runtime.landing_name);
    assert_eq!(config["proxy-groups"][1]["proxies"][0], runtime.landing_name);
    assert_eq!(
        serde_json::to_value(runtime).unwrap(),
        serde_json::json!({
            "chain_id": CHAIN_ID,
            "group_name": format!("clash5-chain-{CHAIN_ID}"),
            "first_hop_name": format!("clash5-chain-{CHAIN_ID}-h0"),
            "landing_name": format!("clash5-chain-{CHAIN_ID}-h1")
        })
    );
}

#[test]
fn rename_resolves_by_identity_and_identical_duplicates_are_safe() {
    let (chain, mut catalog, mut config) = fixture();
    catalog[0].proxy.insert("name".into(), "Renamed".into());
    let duplicate = scoped(Source::Manual, "Another name", 18081);
    catalog.push(duplicate);
    assert!(compile_chains(&mut config, &[chain], &[], &catalog).is_ok());
}

#[test]
fn ambiguous_identity_and_wrong_source_leave_config_untouched() {
    let (chain, mut catalog, mut config) = fixture();
    let original = config.clone();
    let mut conflicting = scoped(Source::Manual, "Conflicting", 18081);
    conflicting.proxy.insert("tls".into(), true.into());
    catalog.push(conflicting);
    assert_eq!(
        compile_chains(&mut config, std::slice::from_ref(&chain), &[], &catalog),
        Err(ChainError::AmbiguousNode)
    );
    assert_eq!(config, original);
    catalog.pop();
    catalog[1].reference.source = Source::Subscription {
        provider_id: "provider-b".into(),
    };
    assert_eq!(
        compile_chains(&mut config, &[chain], &[], &catalog),
        Err(ChainError::MissingNode)
    );
    assert_eq!(config, original);
}

#[test]
fn conflicting_generated_names_and_later_missing_nodes_roll_back_all_chains() {
    let (chain, catalog, mut config) = fixture();
    let mut missing = chain.clone();
    missing.id = "28dc0b8a-7485-49bc-8a91-f48bdc736846".into();
    let Hop::Node { node } = missing.hops.last_mut().unwrap() else {
        panic!("fixture hops are nodes")
    };
    node.stable_id = "not-present".into();
    let original = config.clone();
    assert_eq!(
        compile_chains(&mut config, &[chain.clone(), missing], &[], &catalog),
        Err(ChainError::MissingNode)
    );
    assert_eq!(config, original);
    config.get_mut("proxies").unwrap().as_sequence_mut().unwrap()[0]["name"] = chain.group_name().into();
    let original = config.clone();
    assert_eq!(
        compile_chains(&mut config, &[chain], &[], &catalog),
        Err(ChainError::NameConflict)
    );
    assert_eq!(config, original);
}

#[test]
fn chain_survives_restart_without_persisting_credentials_and_rejects_invalid_edits() {
    let (chain, _, _) = fixture();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-data");
    std::fs::create_dir_all(&root).unwrap();
    let directory = tempfile::tempdir_in(root).unwrap();
    let path = directory.path().join("m2.sqlite");
    {
        let mut store = BindingStore::open(&path).unwrap();
        store.save_proxy_chain(&chain).unwrap();
        let mut invalid = chain.clone();
        invalid.id = "not-a-uuid".into();
        assert!(store.save_proxy_chain(&invalid).is_err());
        invalid = chain.clone();
        invalid.hops = vec![invalid.hops[0].clone()];
        assert!(store.save_proxy_chain(&invalid).is_err());
        invalid.hops = vec![invalid.hops[0].clone(), invalid.hops[0].clone()];
        assert!(store.save_proxy_chain(&invalid).is_err());
    }
    let mut store = BindingStore::open(&path).unwrap();
    assert_eq!(store.proxy_chains().unwrap(), vec![chain.clone()]);
    let raw = store.chain(CHAIN_ID).unwrap().unwrap().to_string();
    assert!(!raw.contains("test-only-password"));
    assert!(!raw.contains("username"));
    store.delete_proxy_chain(CHAIN_ID).unwrap();
    assert!(store.proxy_chains().unwrap().is_empty());
}

#[test]
fn disabled_chain_needs_no_runtime_nodes_and_does_not_modify_config() {
    let (mut chain, _, mut config) = fixture();
    chain.enabled = false;
    let original = config.clone();
    assert!(compile_chains(&mut config, &[chain], &[], &[]).unwrap().is_empty());
    assert_eq!(config, original);
}

#[test]
fn fallback_groups_reference_only_enabled_chains_and_reject_cycles_atomically() {
    let (mut primary, catalog, mut config) = fixture();
    let mut fallback = primary.clone();
    fallback.id = "28dc0b8a-7485-49bc-8a91-f48bdc736846".into();
    primary.exit_policy = ExitPolicy::Switch {
        fallback_chain_id: fallback.id.clone(),
    };
    let original = config.clone();
    assert_eq!(
        compile_chains(&mut config, std::slice::from_ref(&primary), &[], &catalog),
        Err(ChainError::InvalidDefinition)
    );
    assert_eq!(config, original);
    compile_chains(&mut config, &[primary.clone(), fallback.clone()], &[], &catalog).unwrap();
    assert_eq!(config["proxy-groups"][1]["proxies"][1], fallback.group_name());
    assert_eq!(config["proxy-groups"][0]["proxies"][1], primary.group_name());
    fallback.exit_policy = ExitPolicy::Switch {
        fallback_chain_id: primary.id.clone(),
    };
    let mut config = original.clone();
    assert_eq!(
        compile_chains(&mut config, &[primary, fallback], &[], &catalog),
        Err(ChainError::InvalidDefinition)
    );
    assert_eq!(config, original);
}
