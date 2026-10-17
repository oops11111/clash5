use clash5_data::{ExitPolicy, NodeRef, ProxyChain, ScopedProxy, Source, compile_chains, parse_proxy_node};
use serde_yaml_ng::{Mapping, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ports = std::env::args()
        .skip(1)
        .map(|port| port.parse::<u16>())
        .collect::<Result<Vec<_>, _>>()?;
    if ports.len() != 4 || ports.contains(&0) {
        return Err("expected front, landing, probe and controller ports".into());
    }
    let mut catalog = vec![];
    for (name, port) in ["Front", "Landing"].into_iter().zip(&ports) {
        let proxy: Mapping =
            serde_yaml_ng::from_str(&format!("name: {name}\ntype: http\nserver: 127.0.0.1\nport: {port}\n"))?;
        let stable_id = parse_proxy_node(&Value::Mapping(proxy.clone()))?.stable_id()?;
        catalog.push(ScopedProxy {
            reference: NodeRef {
                source: Source::Manual,
                stable_id,
            },
            proxy,
        });
    }
    let chain = ProxyChain {
        schema_version: 1,
        id: "18dc0b8a-7485-49bc-8a91-f48bdc736846".into(),
        name: "Local two-hop fixture".into(),
        profile_id: None,
        front: catalog[0].reference.clone(),
        landing: catalog[1].reference.clone(),
        enabled: true,
        exit_policy: ExitPolicy::Warn,
    };
    let mut config: Mapping = serde_yaml_ng::from_str(&format!(
        "mixed-port: 0\nallow-lan: false\nmode: rule\nlog-level: warning\nexternal-controller: 127.0.0.1:{}\nsecret: fixture-controller-secret\nrules: ['MATCH,REJECT']\n",
        ports[3]
    ))?;
    let compiled = compile_chains(&mut config, &[chain], &catalog)?;
    let listeners: Value = serde_yaml_ng::from_str(&format!(
        "- {{name: fixture-probe, type: http, listen: 127.0.0.1, port: {}, proxy: {}}}",
        ports[2], compiled[0].group_name
    ))?;
    config.insert("listeners".into(), listeners);
    print!("{}", serde_yaml_ng::to_string(&config)?);
    Ok(())
}
