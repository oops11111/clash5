use clash5_data::{
    AppMatcher, AppRoute, BindingStore, ExitPolicy, Hop, NodeRef, ProxyChain, RouteTarget, Source, compile_app_routes,
};
use serde_yaml_ng::Mapping;

const CHAIN_ID: &str = "18dc0b8a-7485-49bc-8a91-f48bdc736846";
const ROUTE_ID: &str = "4a2b3c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d";

fn node(name: &str) -> NodeRef {
    NodeRef {
        source: Source::Manual,
        stable_id: format!("v2:{name}"),
    }
}

fn chain(enabled: bool) -> ProxyChain {
    ProxyChain {
        schema_version: 2,
        id: CHAIN_ID.into(),
        name: "Route chain".into(),
        profile_id: None,
        hops: vec![Hop::Node { node: node("a") }, Hop::Node { node: node("c") }],
        enabled,
        exit_policy: ExitPolicy::Warn,
        pinned_exit_ip: None,
    }
}

fn route(matchers: Vec<AppMatcher>, target: RouteTarget, enabled: bool) -> AppRoute {
    AppRoute {
        schema_version: 1,
        id: ROUTE_ID.into(),
        name: "Browser".into(),
        matchers,
        target,
        enabled,
    }
}

fn config() -> Mapping {
    serde_yaml_ng::from_str("rules: ['MATCH,DIRECT']\n").unwrap()
}

fn rules(config: &Mapping) -> Vec<String> {
    config["rules"]
        .as_sequence()
        .unwrap()
        .iter()
        .map(|rule| rule.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn process_rules_come_before_the_profile_rules_and_name_the_chain_group() {
    let mut config = config();
    let chains = [chain(true)];
    let routes = [route(
        vec![
            AppMatcher::ProcessName { name: "browser".into() },
            AppMatcher::ProcessPath {
                path: "/opt/tool/bin/tool".into(),
            },
        ],
        RouteTarget::Chain {
            chain_id: CHAIN_ID.into(),
        },
        true,
    )];
    assert_eq!(compile_app_routes(&mut config, &routes, &chains).unwrap(), 2);
    assert_eq!(
        rules(&config),
        vec![
            "PROCESS-NAME,browser,clash5-chain-18dc0b8a-7485-49bc-8a91-f48bdc736846".to_owned(),
            "PROCESS-PATH,/opt/tool/bin/tool,clash5-chain-18dc0b8a-7485-49bc-8a91-f48bdc736846".to_owned(),
            "MATCH,DIRECT".to_owned(),
        ]
    );
    assert_eq!(config["find-process-mode"], "always");
}

#[test]
fn direct_and_reject_targets_and_disabled_routes_are_handled() {
    let mut config = config();
    let routes = [
        route(
            vec![AppMatcher::ProcessName { name: "a".into() }],
            RouteTarget::Direct,
            true,
        ),
        route(
            vec![AppMatcher::ProcessName { name: "b".into() }],
            RouteTarget::Reject,
            false,
        ),
    ];
    assert_eq!(compile_app_routes(&mut config, &routes, &[]).unwrap(), 1);
    assert_eq!(rules(&config)[0], "PROCESS-NAME,a,DIRECT");
}

#[test]
fn no_active_routes_leaves_the_configuration_untouched() {
    let mut config = config();
    let before = config.clone();
    assert_eq!(compile_app_routes(&mut config, &[], &[]).unwrap(), 0);
    assert_eq!(config, before);
    assert!(config.get("find-process-mode").is_none());
}

#[test]
fn a_route_to_a_missing_or_disabled_chain_is_rejected() {
    let mut config = config();
    let target = RouteTarget::Chain {
        chain_id: CHAIN_ID.into(),
    };
    let routes = [route(
        vec![AppMatcher::ProcessName { name: "x".into() }],
        target.clone(),
        true,
    )];
    assert!(compile_app_routes(&mut config, &routes, &[]).is_err());
    assert!(compile_app_routes(&mut config, &routes, &[chain(false)]).is_err());
}

#[test]
fn matchers_that_could_change_rule_meaning_are_rejected() {
    let bad = [
        route(
            vec![AppMatcher::ProcessName {
                name: "a,DIRECT".into(),
            }],
            RouteTarget::Direct,
            true,
        ),
        route(
            vec![AppMatcher::ProcessName {
                name: "dir/tool".into(),
            }],
            RouteTarget::Direct,
            true,
        ),
        route(
            vec![AppMatcher::ProcessName {
                name: "line\nbreak".into(),
            }],
            RouteTarget::Direct,
            true,
        ),
        route(
            vec![AppMatcher::ProcessPath {
                path: "relative/tool".into(),
            }],
            RouteTarget::Direct,
            true,
        ),
        route(
            vec![AppMatcher::ProcessPath {
                path: "/opt/a,b".into(),
            }],
            RouteTarget::Direct,
            true,
        ),
        route(vec![], RouteTarget::Direct, true),
    ];
    for route in bad {
        assert!(route.validate().is_err(), "{route:?}");
    }
    let windows = route(
        vec![AppMatcher::ProcessPath {
            path: "C:\\Program Files\\app.exe".into(),
        }],
        RouteTarget::Direct,
        true,
    );
    assert!(windows.validate().is_ok());
}

#[test]
fn routes_round_trip_through_the_store() {
    let mut store = BindingStore::memory().unwrap();
    let saved = route(
        vec![AppMatcher::ProcessName { name: "browser".into() }],
        RouteTarget::Direct,
        true,
    );
    store.save_app_route(&saved).unwrap();
    assert_eq!(store.app_routes().unwrap(), vec![saved]);
    store.delete_app_route(ROUTE_ID).unwrap();
    assert!(store.app_routes().unwrap().is_empty());
}
