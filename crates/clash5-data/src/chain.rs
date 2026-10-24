use crate::{DataError, Source, StableId, identity::text};
use serde::{Deserialize, Serialize};
use serde_yaml_ng::{Mapping, Value};
use std::collections::HashSet;

/// Upper bound on hops per chain. Each hop adds a dialer link, so long chains only add latency.
pub const MAX_HOPS: usize = 6;
pub const MAX_POOL_MEMBERS: usize = 32;
const PROBE_URL: &str = "http://cp.cloudflare.com/generate_204";
const PROBE_INTERVAL_SECONDS: u32 = 300;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct NodeRef {
    pub source: Source,
    pub stable_id: StableId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Hop {
    /// One node, used as this hop.
    Node { node: NodeRef },
    /// A pool, used as this hop. Traffic uses whichever member the latency probe currently prefers.
    Pool { pool_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExitPolicy {
    Warn,
    Switch { fallback_chain_id: String },
}

/// One detection or test outcome. `status` is one of healthy, drift, warning, failed, tested.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct ChainSample {
    pub chain_id: String,
    pub measured_at: String,
    pub status: String,
    pub exit_ip: Option<String>,
    pub delay_ms: Option<u32>,
}

/// A pool is an independent object. Chains refer to it by ID, so one pool can serve many chains.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct NodePool {
    pub schema_version: u8,
    pub id: String,
    pub name: String,
    pub members: Vec<NodeRef>,
}

/// Deserialized by hand so the schema 1 front and landing pair can be read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
pub struct ProxyChain {
    pub schema_version: u8,
    pub id: String,
    pub name: String,
    pub profile_id: Option<String>,
    pub hops: Vec<Hop>,
    pub enabled: bool,
    pub exit_policy: ExitPolicy,
    /// Exit address the chain is expected to keep. Detection reports drift when it differs.
    pub pinned_exit_ip: Option<String>,
}

/// Schema 1 stored a fixed front and landing pair. It is read as two node hops and written back as schema 2.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProxyChainWire {
    schema_version: u8,
    id: String,
    name: String,
    profile_id: Option<String>,
    #[serde(default)]
    hops: Option<Vec<Hop>>,
    #[serde(default)]
    front: Option<NodeRef>,
    #[serde(default)]
    landing: Option<NodeRef>,
    enabled: bool,
    exit_policy: ExitPolicy,
    #[serde(default)]
    pinned_exit_ip: Option<String>,
}

impl TryFrom<ProxyChainWire> for ProxyChain {
    type Error = String;

    fn try_from(wire: ProxyChainWire) -> Result<Self, Self::Error> {
        let hops = match (wire.hops, wire.front, wire.landing) {
            (Some(hops), None, None) => hops,
            (None, Some(front), Some(landing)) => vec![Hop::Node { node: front }, Hop::Node { node: landing }],
            _ => return Err("a chain defines either hops or a legacy front and landing pair".into()),
        };
        let schema_version = if wire.schema_version == 1 {
            2
        } else {
            wire.schema_version
        };
        Ok(Self {
            schema_version,
            id: wire.id,
            name: wire.name,
            profile_id: wire.profile_id,
            hops,
            enabled: wire.enabled,
            exit_policy: wire.exit_policy,
            pinned_exit_ip: wire.pinned_exit_ip,
        })
    }
}

impl<'de> Deserialize<'de> for ProxyChain {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = ProxyChainWire::deserialize(deserializer)?;
        ProxyChain::try_from(wire).map_err(serde::de::Error::custom)
    }
}

fn uuid_v4(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            14 => byte == b'4',
            19 => matches!(byte, b'8' | b'9' | b'a' | b'b'),
            _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte),
        })
}

impl ProxyChain {
    /// Validate the persisted, credential-free chain definition.
    pub fn validate(&self) -> Result<(), DataError> {
        if self.schema_version != 2
            || !uuid_v4(&self.id)
            || self.name.trim().is_empty()
            || self.name.len() > 256
            || !(2..=MAX_HOPS).contains(&self.hops.len())
        {
            return Err(DataError::InvalidInput);
        }
        text(&self.name)?;
        if let Some(profile_id) = &self.profile_id {
            text(profile_id)?;
        }
        for pair in self.hops.windows(2) {
            if pair[0] == pair[1] {
                return Err(DataError::InvalidInput);
            }
        }
        for hop in &self.hops {
            match hop {
                Hop::Node { node } => {
                    text(&node.stable_id)?;
                    node.source.scope()?;
                }
                Hop::Pool { pool_id } => {
                    if !uuid_v4(pool_id) {
                        return Err(DataError::InvalidInput);
                    }
                }
            }
        }
        if let ExitPolicy::Switch { fallback_chain_id } = &self.exit_policy
            && (!uuid_v4(fallback_chain_id) || fallback_chain_id == &self.id)
        {
            return Err(DataError::InvalidInput);
        }
        if let Some(pinned) = &self.pinned_exit_ip
            && pinned.parse::<std::net::IpAddr>().is_err()
        {
            return Err(DataError::InvalidInput);
        }
        Ok(())
    }

    pub fn group_name(&self) -> String {
        format!("clash5-chain-{}", self.id)
    }

    /// Name of the entry the chain selector points at: the last hop's copy.
    pub fn landing_name(&self) -> String {
        format!("{}-h{}", self.group_name(), self.hops.len().saturating_sub(1))
    }
}

impl NodePool {
    pub fn validate(&self) -> Result<(), DataError> {
        if self.schema_version != 1
            || !uuid_v4(&self.id)
            || self.name.trim().is_empty()
            || self.name.len() > 256
            || !(1..=MAX_POOL_MEMBERS).contains(&self.members.len())
        {
            return Err(DataError::InvalidInput);
        }
        text(&self.name)?;
        for (index, member) in self.members.iter().enumerate() {
            text(&member.stable_id)?;
            member.source.scope()?;
            if self.members[..index].contains(member) {
                return Err(DataError::InvalidInput);
            }
        }
        Ok(())
    }
}

// Raw proxy definitions can contain credentials; never derive Debug or serialize this catalog.
pub struct ScopedProxy {
    pub reference: NodeRef,
    pub proxy: Mapping,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CompiledChain {
    pub chain_id: String,
    pub group_name: String,
    pub first_hop_name: String,
    pub landing_name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainError {
    InvalidDefinition,
    MissingNode,
    MissingPool,
    AmbiguousNode,
    NameConflict,
    InvalidConfig,
}

impl std::fmt::Display for ChainError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidDefinition => "C5-CHAIN-001",
            Self::MissingNode => "C5-CHAIN-002",
            Self::AmbiguousNode => "C5-CHAIN-003",
            Self::NameConflict => "C5-CHAIN-004",
            Self::InvalidConfig => "C5-CHAIN-005",
            Self::MissingPool => "C5-CHAIN-015",
        })
    }
}
impl std::error::Error for ChainError {}

fn resolve<'a>(reference: &NodeRef, catalog: &'a [ScopedProxy]) -> Result<&'a Mapping, ChainError> {
    let mut candidates = catalog.iter().filter(|entry| &entry.reference == reference);
    let first = &candidates.next().ok_or(ChainError::MissingNode)?.proxy;
    let mut canonical = first.clone();
    canonical.remove("name");
    for candidate in candidates {
        let mut other = candidate.proxy.clone();
        other.remove("name");
        if canonical != other {
            return Err(ChainError::AmbiguousNode);
        }
    }
    Ok(first)
}

/// Copies one proxy under a new name. The first hop of a chain has no dialer; every other hop dials
/// through the entry of the hop before it.
fn hop_copy(source: &Mapping, name: &str, dialer: Option<&str>) -> Mapping {
    let mut copy = source.clone();
    copy.insert("name".into(), name.into());
    match dialer {
        Some(dialer) => copy.insert("dialer-proxy".into(), dialer.into()),
        None => copy.remove("dialer-proxy"),
    };
    copy
}

/// Compile multi-hop copies atomically, preserving the original proxies, overrides and rules.
pub fn compile_chains(
    config: &mut Mapping,
    chains: &[ProxyChain],
    pools: &[NodePool],
    catalog: &[ScopedProxy],
) -> Result<Vec<CompiledChain>, ChainError> {
    for chain in chains.iter().filter(|chain| chain.enabled) {
        let mut seen = HashSet::new();
        let mut current = chain;
        while let ExitPolicy::Switch { fallback_chain_id } = &current.exit_policy {
            if !seen.insert(current.id.as_str()) {
                return Err(ChainError::InvalidDefinition);
            }
            current = chains
                .iter()
                .find(|fallback| fallback.enabled && fallback.id == *fallback_chain_id)
                .ok_or(ChainError::InvalidDefinition)?;
        }
    }
    let mut candidate = config.clone();
    let mut proxies = match candidate.remove("proxies") {
        Some(Value::Sequence(proxies)) => proxies,
        None => vec![],
        _ => return Err(ChainError::InvalidConfig),
    };
    let mut groups = match candidate.remove("proxy-groups") {
        Some(Value::Sequence(groups)) => groups,
        None => vec![],
        _ => return Err(ChainError::InvalidConfig),
    };
    let mut names: HashSet<String> = proxies
        .iter()
        .chain(&groups)
        .filter_map(|value| value.get("name").and_then(Value::as_str).map(str::to_owned))
        .chain(["DIRECT".into(), "REJECT".into(), "GLOBAL".into()])
        .collect();
    let mut compiled = vec![];
    for chain in chains.iter().filter(|chain| chain.enabled) {
        chain.validate().map_err(|_| ChainError::InvalidDefinition)?;
        let group_name = chain.group_name();
        if !names.insert(group_name.clone()) {
            return Err(ChainError::NameConflict);
        }
        let mut previous: Option<String> = None;
        let mut first_hop_name = None;
        for (index, hop) in chain.hops.iter().enumerate() {
            let hop_name = format!("{group_name}-h{index}");
            let entry = match hop {
                Hop::Node { node } => {
                    if !names.insert(hop_name.clone()) {
                        return Err(ChainError::NameConflict);
                    }
                    let source = resolve(node, catalog)?;
                    proxies.push(Value::Mapping(hop_copy(source, &hop_name, previous.as_deref())));
                    hop_name
                }
                Hop::Pool { pool_id } => {
                    let pool = pools
                        .iter()
                        .find(|pool| pool.id == *pool_id)
                        .ok_or(ChainError::MissingPool)?;
                    pool.validate().map_err(|_| ChainError::InvalidDefinition)?;
                    let mut members = vec![];
                    for (member_index, member) in pool.members.iter().enumerate() {
                        let member_name = format!("{hop_name}-m{member_index}");
                        if !names.insert(member_name.clone()) {
                            return Err(ChainError::NameConflict);
                        }
                        let source = resolve(member, catalog)?;
                        proxies.push(Value::Mapping(hop_copy(source, &member_name, previous.as_deref())));
                        members.push(Value::from(member_name));
                    }
                    if !names.insert(hop_name.clone()) {
                        return Err(ChainError::NameConflict);
                    }
                    let mut probe = Mapping::new();
                    probe.insert("name".into(), hop_name.clone().into());
                    probe.insert("type".into(), "url-test".into());
                    probe.insert("proxies".into(), Value::Sequence(members));
                    probe.insert("url".into(), PROBE_URL.into());
                    probe.insert("interval".into(), PROBE_INTERVAL_SECONDS.into());
                    groups.push(Value::Mapping(probe));
                    hop_name
                }
            };
            first_hop_name.get_or_insert_with(|| entry.clone());
            previous = Some(entry);
        }
        let landing_name = previous.ok_or(ChainError::InvalidDefinition)?;
        let mut group = Mapping::new();
        group.insert("name".into(), group_name.clone().into());
        group.insert("type".into(), "select".into());
        let mut members = vec![Value::from(landing_name.clone())];
        if let ExitPolicy::Switch { fallback_chain_id } = &chain.exit_policy {
            members.push(format!("clash5-chain-{fallback_chain_id}").into());
        }
        group.insert("proxies".into(), Value::Sequence(members));
        groups.push(Value::Mapping(group));
        compiled.push(CompiledChain {
            chain_id: chain.id.clone(),
            group_name,
            first_hop_name: first_hop_name.ok_or(ChainError::InvalidDefinition)?,
            landing_name,
        });
    }
    if compiled.is_empty() {
        return Ok(compiled);
    }
    for group in &mut groups {
        if group.get("type").and_then(Value::as_str) == Some("select")
            && !compiled
                .iter()
                .any(|chain| group.get("name").and_then(Value::as_str) == Some(chain.group_name.as_str()))
            && let Some(members) = group.get_mut("proxies").and_then(Value::as_sequence_mut)
        {
            members.extend(compiled.iter().map(|chain| Value::from(chain.group_name.clone())));
        }
    }
    candidate.insert("proxies".into(), Value::Sequence(proxies));
    candidate.insert("proxy-groups".into(), Value::Sequence(groups));
    *config = candidate;
    Ok(compiled)
}
