use clash5_data::{BindingStore, ExitPolicy, Hop, NodePool, NodeRef, ProxyChain, ScopedProxy, Source, compile_chains};
use serde_yaml_ng::{Mapping, Value};

const CHAIN_ID: &str = "18dc0b8a-7485-49bc-8a91-f48bdc736846";
const POOL_ID: &str = "3f1c2d4e-5a6b-4c7d-8e9f-0a1b2c3d4e5f";

fn proxy(name: &str, port: u16) -> ScopedProxy {
    let mapping: Mapping =
        serde_yaml_ng::from_str(&format!("name: {name}\ntype: http\nserver: 127.0.0.1\nport: {port}\n")).unwrap();
    ScopedProxy {
        reference: NodeRef {
            source: Source::Manual,
            stable_id: format!("v2:{name}"),
        },
        proxy: mapping,
    }
}

fn node(name: &str) -> NodeRef {
    NodeRef {
        source: Source::Manual,
        stable_id: format!("v2:{name}"),
    }
}

fn catalog() -> Vec<ScopedProxy> {
    vec![
        proxy("a", 1),
        proxy("b", 2),
        proxy("c", 3),
        proxy("p1", 4),
        proxy("p2", 5),
    ]
}

fn chain(hops: Vec<Hop>) -> ProxyChain {
    ProxyChain {
        schema_version: 2,
        id: CHAIN_ID.into(),
        name: "Three hop".into(),
        profile_id: None,
        hops,
        enabled: true,
        exit_policy: ExitPolicy::Warn,
    }
}

fn pool() -> NodePool {
    NodePool {
        schema_version: 1,
        id: POOL_ID.into(),
        name: "Pool".into(),
        members: vec![node("p1"), node("p2")],
    }
}

fn empty_config() -> Mapping {
    serde_yaml_ng::from_str("proxies: []\nproxy-groups: []\nrules: ['MATCH,DIRECT']\n").unwrap()
}

fn proxy_named<'a>(config: &'a Mapping, name: &str) -> &'a Value {
    config["proxies"]
        .as_sequence()
        .unwrap()
        .iter()
        .find(|value| value["name"] == name)
        .unwrap()
}

#[test]
fn three_hops_with_a_pool_in_the_middle_dial_through_the_previous_hop() {
    let mut config = empty_config();
    let hops = vec![
        Hop::Node { node: node("a") },
        Hop::Pool {
            pool_id: POOL_ID.into(),
        },
        Hop::Node { node: node("c") },
    ];
    let compiled = compile_chains(&mut config, &[chain(hops)], &[pool()], &catalog()).unwrap();
    let runtime = &compiled[0];
    let group = format!("clash5-chain-{CHAIN_ID}");
    assert_eq!(runtime.first_hop_name, format!("{group}-h0"));
    assert_eq!(runtime.landing_name, format!("{group}-h2"));

    // The first hop has no dialer. Each pool member dials the first hop.
    assert!(
        proxy_named(&config, &format!("{group}-h0"))
            .get("dialer-proxy")
            .is_none()
    );
    for member in ["m0", "m1"] {
        let copy = proxy_named(&config, &format!("{group}-h1-{member}"));
        assert_eq!(copy["dialer-proxy"], format!("{group}-h0"));
    }
    // The last hop dials through the pool group, so it uses whichever member the probe prefers.
    assert_eq!(
        proxy_named(&config, &format!("{group}-h2"))["dialer-proxy"],
        format!("{group}-h1")
    );

    let pool_group = config["proxy-groups"]
        .as_sequence()
        .unwrap()
        .iter()
        .find(|value| value["name"] == format!("{group}-h1"))
        .unwrap();
    assert_eq!(pool_group["type"], "url-test");
    assert_eq!(pool_group["proxies"].as_sequence().unwrap().len(), 2);
}

#[test]
fn a_legacy_front_and_landing_record_reads_as_two_node_hops_and_saves_as_schema_two() {
    let legacy = serde_json::json!({
        "schema_version": 1,
        "id": CHAIN_ID,
        "name": "Legacy",
        "profile_id": null,
        "front": {"source": {"kind": "manual"}, "stable_id": "v2:a"},
        "landing": {"source": {"kind": "manual"}, "stable_id": "v2:b"},
        "enabled": true,
        "exit_policy": {"mode": "warn"}
    });
    let chain: ProxyChain = serde_json::from_value(legacy).unwrap();
    assert_eq!(chain.schema_version, 2);
    assert_eq!(
        chain.hops,
        vec![Hop::Node { node: node("a") }, Hop::Node { node: node("b") }]
    );
}

#[test]
fn a_pool_used_by_a_stored_chain_cannot_be_deleted_until_the_chain_is_gone() {
    let mut store = BindingStore::memory().unwrap();
    store.save_node_pool(&pool()).unwrap();
    store
        .save_proxy_chain(&chain(vec![
            Hop::Node { node: node("a") },
            Hop::Pool {
                pool_id: POOL_ID.into(),
            },
        ]))
        .unwrap();
    assert!(store.delete_node_pool(POOL_ID).is_err());
    store.delete_proxy_chain(CHAIN_ID).unwrap();
    store.delete_node_pool(POOL_ID).unwrap();
    assert!(store.node_pools().unwrap().is_empty());
}

#[test]
fn pools_reject_duplicate_members_and_chains_reject_adjacent_repeats() {
    let mut duplicate = pool();
    duplicate.members.push(node("p1"));
    assert!(duplicate.validate().is_err());
    let repeated = chain(vec![Hop::Node { node: node("a") }, Hop::Node { node: node("a") }]);
    assert!(repeated.validate().is_err());
    let too_long = chain(vec![Hop::Node { node: node("a") }; 7]);
    assert!(too_long.validate().is_err());
}
