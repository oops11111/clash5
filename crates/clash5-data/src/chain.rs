use crate::{DataError, Source, StableId, identity::text};
use serde::{Deserialize, Serialize};
use serde_yaml_ng::{Mapping, Value};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct NodeRef {
    pub source: Source,
    pub stable_id: StableId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExitPolicy {
    Warn,
    Switch { fallback_chain_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct ProxyChain {
    pub schema_version: u8,
    pub id: String,
    pub name: String,
    pub profile_id: Option<String>,
    pub front: NodeRef,
    pub landing: NodeRef,
    pub enabled: bool,
    pub exit_policy: ExitPolicy,
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
        if self.schema_version != 1
            || !uuid_v4(&self.id)
            || self.name.trim().is_empty()
            || self.name.len() > 256
            || self.front == self.landing
        {
            return Err(DataError::InvalidInput);
        }
        text(&self.name)?;
        if let Some(profile_id) = &self.profile_id {
            text(profile_id)?;
        }
        for node in [&self.front, &self.landing] {
            text(&node.stable_id)?;
            node.source.scope()?;
        }
        if let ExitPolicy::Switch { fallback_chain_id } = &self.exit_policy
            && (!uuid_v4(fallback_chain_id) || fallback_chain_id == &self.id)
        {
            return Err(DataError::InvalidInput);
        }
        Ok(())
    }

    pub fn group_name(&self) -> String {
        format!("clash5-chain-{}", self.id)
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
    pub front_name: String,
    pub landing_name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainError {
    InvalidDefinition,
    MissingNode,
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

/// Compile two-hop copies atomically, preserving the original proxies, overrides and rules.
pub fn compile_chains(
    config: &mut Mapping,
    chains: &[ProxyChain],
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
        let front_name = format!("{group_name}-front");
        let landing_name = format!("{group_name}-landing");
        for name in [&group_name, &front_name, &landing_name] {
            if !names.insert(name.clone()) {
                return Err(ChainError::NameConflict);
            }
        }
        let mut front = resolve(&chain.front, catalog)?.clone();
        let mut landing = resolve(&chain.landing, catalog)?.clone();
        front.insert("name".into(), front_name.clone().into());
        // The copied front is the first hop even when its original has a dialer override.
        front.remove("dialer-proxy");
        landing.insert("name".into(), landing_name.clone().into());
        landing.insert("dialer-proxy".into(), front_name.clone().into());
        proxies.extend([Value::Mapping(front), Value::Mapping(landing)]);
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
            front_name,
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
