use crate::{ChainError, DataError, ProxyChain, identity::text};
use serde::{Deserialize, Serialize};
use serde_yaml_ng::{Mapping, Value};

pub const MAX_APP_MATCHERS: usize = 32;

/// What a route matches. Process matchers use the executable name or full path. The domain matcher
/// matches a domain and its subdomains for every application, so it cannot separate applications.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AppMatcher {
    ProcessName { name: String },
    ProcessPath { path: String },
    DomainSuffix { suffix: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RouteTarget {
    Chain { chain_id: String },
    Direct,
    Reject,
}

/// An application or subsystem route. Routes are independent objects; each one lists what it covers
/// and where that traffic goes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct AppRoute {
    pub schema_version: u8,
    pub id: String,
    pub name: String,
    pub matchers: Vec<AppMatcher>,
    pub target: RouteTarget,
    pub enabled: bool,
    /// Resolver for this route's domain matchers, as a DoH URL or an IP address. It is only valid with
    /// domain matchers, and it applies to every application that resolves those domains.
    #[serde(default)]
    pub nameserver: Option<String>,
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

fn domain_label_valid(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
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
            Self::DomainSuffix { suffix } => {
                if suffix.len() > 253 || !suffix.split('.').all(domain_label_valid) {
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
        if let Some(nameserver) = &self.nameserver {
            let has_domains = self
                .matchers
                .iter()
                .any(|matcher| matches!(matcher, AppMatcher::DomainSuffix { .. }));
            let doh = nameserver.starts_with("https://") && rule_safe(nameserver);
            let ip = nameserver.parse::<std::net::IpAddr>().is_ok();
            if !has_domains || !(doh || ip) {
                return Err(DataError::InvalidInput);
            }
        }
        Ok(())
    }
}

/// Adds one rule per matcher ahead of the profile's rules. Process matchers need `find-process-mode`,
/// which is set only when a route uses them. A route's nameserver becomes a `nameserver-policy` entry
/// for each of its domain suffixes. Nothing is written unless every route validates first.
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
    let mut policies = Mapping::new();
    let mut process_matchers = false;
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
            match matcher {
                AppMatcher::ProcessName { name } => {
                    process_matchers = true;
                    rules.push(format!("PROCESS-NAME,{name},{target}"));
                }
                AppMatcher::ProcessPath { path } => {
                    process_matchers = true;
                    rules.push(format!("PROCESS-PATH,{path},{target}"));
                }
                AppMatcher::DomainSuffix { suffix } => {
                    rules.push(format!("DOMAIN-SUFFIX,{suffix},{target}"));
                    if let Some(nameserver) = &route.nameserver {
                        let key = format!("+.{suffix}");
                        if policies.contains_key(key.as_str()) {
                            return Err(ChainError::NameConflict);
                        }
                        policies.insert(key.into(), Value::Sequence(vec![nameserver.clone().into()]));
                    }
                }
            }
        }
    }
    // Validate the whole batch before touching the configuration, so a failure changes nothing.
    let existing_rules = match config.get("rules") {
        None => vec![],
        Some(Value::Sequence(existing)) => existing.clone(),
        Some(_) => return Err(ChainError::InvalidConfig),
    };
    let mut merged_policy = match config.get("dns") {
        None if policies.is_empty() => None,
        Some(Value::Mapping(dns)) => match dns.get("nameserver-policy") {
            None => Some(Mapping::new()),
            Some(Value::Mapping(existing)) => Some(existing.clone()),
            Some(_) => return Err(ChainError::InvalidConfig),
        },
        _ => return Err(ChainError::InvalidConfig),
    };
    if let Some(merged) = merged_policy.as_mut() {
        for (key, value) in &policies {
            // A profile's own policy for the same domain wins over an app route. Replacing it would change DNS.
            if merged.contains_key(key) {
                return Err(ChainError::NameConflict);
            }
            merged.insert(key.clone(), value.clone());
        }
    }

    let count = rules.len();
    let mut merged_rules: Vec<Value> = rules.into_iter().map(Value::from).collect();
    merged_rules.extend(existing_rules);
    config.insert("rules".into(), Value::Sequence(merged_rules));
    if let Some(merged) = merged_policy
        && let Some(Value::Mapping(dns)) = config.get_mut("dns")
    {
        dns.insert("nameserver-policy".into(), Value::Mapping(merged));
    }
    if process_matchers {
        config.insert("find-process-mode".into(), "always".into());
    }
    Ok(count)
}
