use crate::{ChainError, DataError, ProxyChain, identity::text};
use serde::{Deserialize, Serialize};
use serde_yaml_ng::{Mapping, Value};

pub const MAX_APP_MATCHERS: usize = 32;

/// Which processes a route applies to. A name matches the executable's name; a path matches the
/// full executable path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AppMatcher {
    ProcessName { name: String },
    ProcessPath { path: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RouteTarget {
    Chain { chain_id: String },
    Direct,
    Reject,
}

/// An application or subsystem route. Routes are independent objects; each one lists the processes
/// it covers and where their traffic goes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct AppRoute {
    pub schema_version: u8,
    pub id: String,
    pub name: String,
    pub matchers: Vec<AppMatcher>,
    pub target: RouteTarget,
    pub enabled: bool,
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

/// Rule text is comma separated and line based, so these characters could change the rule's meaning.
fn rule_safe(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && !value
            .chars()
            .any(|character| matches!(character, ',' | '\n' | '\r' | '\0'))
}

impl AppMatcher {
    fn validate(&self) -> Result<(), DataError> {
        match self {
            Self::ProcessName { name } => {
                if !rule_safe(name) || name.len() > 256 || name.contains(['/', '\\']) {
                    return Err(DataError::InvalidInput);
                }
            }
            Self::ProcessPath { path } => {
                let absolute = path.starts_with('/') || (path.len() > 2 && path.as_bytes()[1] == b':');
                if !rule_safe(path) || !absolute {
                    return Err(DataError::InvalidInput);
                }
            }
        }
        Ok(())
    }
}

impl AppRoute {
    pub fn validate(&self) -> Result<(), DataError> {
        if self.schema_version != 1
            || !uuid_v4(&self.id)
            || self.name.trim().is_empty()
            || self.name.len() > 256
            || self.matchers.is_empty()
            || self.matchers.len() > MAX_APP_MATCHERS
        {
            return Err(DataError::InvalidInput);
        }
        text(&self.name)?;
        for matcher in &self.matchers {
            matcher.validate()?;
        }
        if let RouteTarget::Chain { chain_id } = &self.target
            && !uuid_v4(chain_id)
        {
            return Err(DataError::InvalidInput);
        }
        Ok(())
    }
}

/// Adds one rule per matcher ahead of the existing rules, so a route beats the profile's own rules.
/// `find-process-mode: always` is required for process matching and is set only when routes exist.
pub fn compile_app_routes(
    config: &mut Mapping,
    routes: &[AppRoute],
    chains: &[ProxyChain],
) -> Result<usize, ChainError> {
    let active: Vec<&AppRoute> = routes.iter().filter(|route| route.enabled).collect();
    if active.is_empty() {
        return Ok(0);
    }
    let mut rules = vec![];
    for route in &active {
        route.validate().map_err(|_| ChainError::InvalidDefinition)?;
        let target = match &route.target {
            RouteTarget::Direct => "DIRECT".to_owned(),
            RouteTarget::Reject => "REJECT".to_owned(),
            RouteTarget::Chain { chain_id } => {
                let chain = chains
                    .iter()
                    .find(|chain| chain.id == *chain_id && chain.enabled)
                    .ok_or(ChainError::InvalidDefinition)?;
                chain.group_name()
            }
        };
        for matcher in &route.matchers {
            rules.push(match matcher {
                AppMatcher::ProcessName { name } => format!("PROCESS-NAME,{name},{target}"),
                AppMatcher::ProcessPath { path } => format!("PROCESS-PATH,{path},{target}"),
            });
        }
    }
    let count = rules.len();
    let existing = match config.remove("rules") {
        Some(Value::Sequence(existing)) => existing,
        None => vec![],
        _ => return Err(ChainError::InvalidConfig),
    };
    let mut merged: Vec<Value> = rules.into_iter().map(Value::from).collect();
    merged.extend(existing);
    config.insert("rules".into(), Value::Sequence(merged));
    config.insert("find-process-mode".into(), "always".into());
    Ok(count)
}
