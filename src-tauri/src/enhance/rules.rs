//! Rule pipeline applied to the subscription's own rules.
//!
//! Proxy policies in subscription rules are replaced by the default proxy: the
//! target of the subscription's final `MATCH` rule. `DIRECT` and `REJECT` stay.
//! Text rule sets are placed before the subscription rules, and a rule that is
//! already present after decoupling is merged instead of added twice.

use crate::config::{RuleTarget, TextRuleSet};
use clash_verge_logging::{Type, logging};
use parking_lot::Mutex;
use serde::Serialize;
use serde_yaml_ng::{Mapping, Value};
use std::collections::{BTreeMap, HashMap, HashSet, btree_map::Entry};

/// How a rule in the generated config was produced, for the rules page.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct RuleNote {
    pub kind: std::string::String,
    pub payload: std::string::String,
    pub policy: std::string::String,
    /// Name of the text rule set that supplied the rule first; `None` for subscription rules.
    pub source: Option<std::string::String>,
    /// Number of identical rules merged into this one.
    pub merged: u32,
}

/// What the pipeline produced for the current profile, for the rules page.
#[derive(Debug, Clone, Default, Serialize, specta::Type)]
pub struct RuleOverview {
    pub notes: Vec<RuleNote>,
    pub subscription_rules: u32,
    /// Whether the subscription's proxy policies were replaced by the default proxy.
    pub decoupled: bool,
    /// The group that serves as the default proxy, when the config has one.
    pub default_group: Option<std::string::String>,
}

static OVERVIEW: Mutex<Option<RuleOverview>> = Mutex::new(None);

pub(crate) fn store_overview(overview: RuleOverview) {
    *OVERVIEW.lock() = Some(overview);
}

pub(crate) fn rule_overview() -> RuleOverview {
    OVERVIEW.lock().clone().unwrap_or_default()
}

fn overview(
    notes: Vec<RuleNote>,
    subscription: &[String],
    decoupled: bool,
    default_group: Option<&str>,
) -> RuleOverview {
    RuleOverview {
        notes,
        subscription_rules: u32::try_from(subscription.len()).unwrap_or(u32::MAX),
        decoupled,
        default_group: default_group.map(str::to_owned),
    }
}

const KEPT_POLICIES: [&str; 4] = ["DIRECT", "REJECT", "REJECT-DROP", "PASS"];

const RULE_TYPES: [&str; 14] = [
    "DOMAIN",
    "DOMAIN-SUFFIX",
    "DOMAIN-KEYWORD",
    "DOMAIN-REGEX",
    "GEOSITE",
    "GEOIP",
    "IP-CIDR",
    "IP-CIDR6",
    "SRC-IP-CIDR",
    "SRC-PORT",
    "DST-PORT",
    "PROCESS-NAME",
    "PROCESS-PATH",
    "RULE-SET",
];

fn fields(rule: &str) -> Vec<&str> {
    rule.split(',').map(str::trim).collect()
}

const fn policy_index(kind: &str) -> usize {
    if kind.eq_ignore_ascii_case("MATCH") { 1 } else { 2 }
}

fn is_kept(policy: &str) -> bool {
    KEPT_POLICIES.iter().any(|kept| policy.eq_ignore_ascii_case(kept))
}

fn decouple_rule(rule: &str, target: &str) -> String {
    let mut parts = fields(rule);
    let Some(first) = parts.first() else {
        return rule.to_owned();
    };
    let index = policy_index(first);
    let replace = parts.get(index).is_some_and(|policy| !is_kept(policy));
    if replace {
        parts[index] = target;
        parts.join(",")
    } else {
        rule.to_owned()
    }
}

/// Rules from one text rule set, plus the lines that could not be read.
struct TextRules {
    rules: Vec<String>,
    invalid: usize,
}

fn parse_text_rules(content: &str, target: &str) -> TextRules {
    let mut parsed = TextRules {
        rules: Vec::new(),
        invalid: 0,
    };
    for line in content.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts = fields(line);
        let kind = parts[0].to_ascii_uppercase();
        let flag_ok = parts.len() == 2 || (parts.len() == 3 && parts[2].eq_ignore_ascii_case("no-resolve"));
        match parts.get(1) {
            Some(payload) if flag_ok && !payload.is_empty() && RULE_TYPES.contains(&kind.as_str()) => {
                let flag = if parts.len() == 3 { ",no-resolve" } else { "" };
                parsed.rules.push(format!("{kind},{payload},{target}{flag}"));
            }
            _ => parsed.invalid += 1,
        }
    }
    parsed
}

/// Counts reported by [`merge_rules`].
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct MergeReport {
    pub(crate) added: usize,
    pub(crate) merged: usize,
    /// Same type and content as an existing rule but a different policy; both are kept.
    pub(crate) conflicts: usize,
}

fn content_key(rule: &str) -> Option<(String, String, String)> {
    let parts = fields(rule);
    let kind = parts.first()?.to_ascii_uppercase();
    let payload = parts.get(1)?.to_string();
    let policy = parts.get(policy_index(&kind))?.to_string();
    Some((kind, payload, policy))
}

fn merge_rules(custom: Vec<String>, subscription: Vec<String>) -> (Vec<String>, MergeReport) {
    let mut report = MergeReport::default();
    let mut seen: HashSet<(String, String, String)> = HashSet::new();
    let mut policies: HashMap<(String, String), String> = HashMap::new();
    for rule in &subscription {
        if let Some((kind, payload, policy)) = content_key(rule) {
            policies
                .entry((kind.clone(), payload.clone()))
                .or_insert_with(|| policy.clone());
            seen.insert((kind, payload, policy));
        }
    }

    let mut merged = Vec::with_capacity(custom.len() + subscription.len());
    for rule in custom {
        let Some(key) = content_key(&rule) else {
            continue;
        };
        if seen.contains(&key) {
            report.merged += 1;
            continue;
        }
        let (kind, payload, policy) = key.clone();
        match policies.get(&(kind.clone(), payload.clone())) {
            Some(existing) if *existing != policy => report.conflicts += 1,
            Some(_) => {}
            None => {
                policies.insert((kind, payload), policy);
            }
        }
        seen.insert(key);
        report.added += 1;
        merged.push(rule);
    }
    merged.extend(subscription);
    (merged, report)
}

/// clash5-owned group that holds every subscription node and provider; unmatched traffic exits here.
pub(crate) const DEFAULT_GROUP: &str = "clash5-default";

/// Adds `DEFAULT_GROUP` to the config when it has any node or provider to hold.
fn ensure_default_group(config: &mut Mapping) -> bool {
    let names: Vec<Value> = config
        .get("proxies")
        .and_then(Value::as_sequence)
        .into_iter()
        .flatten()
        .filter_map(|proxy| proxy.as_mapping()?.get("name")?.as_str().map(Value::from))
        .collect();
    let providers: Vec<Value> = config
        .get("proxy-providers")
        .and_then(Value::as_mapping)
        .into_iter()
        .flatten()
        .filter_map(|(name, _)| name.as_str().map(Value::from))
        .collect();
    if names.is_empty() && providers.is_empty() {
        return false;
    }

    let mut group = Mapping::new();
    group.insert(Value::from("name"), Value::from(DEFAULT_GROUP));
    group.insert(Value::from("type"), Value::from("select"));
    group.insert(Value::from("proxies"), Value::Sequence(names));
    if !providers.is_empty() {
        group.insert(Value::from("use"), Value::Sequence(providers));
    }

    let mut groups: Vec<Value> = config
        .get("proxy-groups")
        .and_then(Value::as_sequence)
        .cloned()
        .unwrap_or_default();
    groups.retain(|group| {
        group
            .as_mapping()
            .and_then(|group| group.get("name"))
            .and_then(Value::as_str)
            != Some(DEFAULT_GROUP)
    });
    groups.insert(0, Value::Mapping(group));
    config.insert(Value::from("proxy-groups"), Value::Sequence(groups));
    true
}

/// Subscription rules with their groups kept. The final `MATCH` always moves to the default
/// group; with `decouple` on, every proxy policy moves there too.
fn route_subscription(rules: &[String], decouple: bool, default: &str) -> Vec<String> {
    let last_match = rules.iter().rposition(|rule| {
        fields(rule)
            .first()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("MATCH"))
    });
    rules
        .iter()
        .enumerate()
        .map(|(index, rule)| {
            if decouple || Some(index) == last_match {
                decouple_rule(rule, default)
            } else {
                rule.clone()
            }
        })
        .collect()
}

const fn target_policy(target: RuleTarget, default: &str) -> &str {
    match target {
        RuleTarget::Default => default,
        RuleTarget::Direct => "DIRECT",
        RuleTarget::Reject => "REJECT",
    }
}

/// Applies the rule pipeline to a subscription config before its rules-editor sequences.
/// With `decouple` off, the subscription's own policies are kept and only text rules are merged.
pub(crate) fn apply(mut config: Mapping, text_rule_sets: &[TextRuleSet], decouple: bool) -> (Mapping, RuleOverview) {
    let rules_value = config.get("rules").and_then(Value::as_sequence);
    let subscription: Vec<String> = rules_value
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();

    if !ensure_default_group(&mut config) {
        return (config, overview(Vec::new(), &subscription, false, None));
    }
    let default = DEFAULT_GROUP.to_owned();

    let custom: Vec<(String, String)> = text_rule_sets
        .iter()
        .flat_map(|set| {
            let target = target_policy(set.target, &default);
            let rules = match set.url {
                Some(_) => vec![format!("RULE-SET,{},{target}", provider_name(&set.id))],
                None => parse_text_rules(&set.content, target).rules,
            };
            rules.into_iter().map(move |rule| (set.name.clone(), rule))
        })
        .collect();
    let providers: Vec<(Value, Value)> = text_rule_sets
        .iter()
        .filter_map(|set| {
            let url = set.url.as_deref()?;
            Some((
                Value::from(provider_name(&set.id)),
                provider_entry(&provider_name(&set.id), url),
            ))
        })
        .collect();
    if !providers.is_empty() {
        let mut rule_providers = config
            .get("rule-providers")
            .and_then(Value::as_mapping)
            .cloned()
            .unwrap_or_default();
        rule_providers.extend(providers);
        config.insert(Value::from("rule-providers"), Value::Mapping(rule_providers));
    }
    let decoupled = route_subscription(&subscription, decouple, &default);

    if decoupled.is_empty() && custom.is_empty() {
        return (config, overview(Vec::new(), &subscription, false, Some(&default)));
    }

    let notes = notes_for(&custom, &decoupled);
    let custom_rules: Vec<String> = custom.into_iter().map(|(_, rule)| rule).collect();
    let (merged, report) = merge_rules(custom_rules, decoupled);
    if report.conflicts > 0 {
        logging!(
            warn,
            Type::Config,
            "text rules conflict with existing rules on {} entries; both are kept",
            report.conflicts
        );
    }
    config.insert(
        Value::from("rules"),
        Value::Sequence(merged.into_iter().map(Value::from).collect()),
    );
    (config, overview(notes, &subscription, decouple, Some(&default)))
}

fn provider_name(id: &str) -> String {
    format!("clash5-set-{id}")
}

/// A classical-text rule provider that mihomo downloads and refreshes daily.
fn provider_entry(name: &str, url: &str) -> Value {
    let mut entry = Mapping::new();
    entry.insert(Value::from("type"), Value::from("http"));
    entry.insert(Value::from("behavior"), Value::from("classical"));
    entry.insert(Value::from("format"), Value::from("text"));
    entry.insert(Value::from("url"), Value::from(url));
    entry.insert(Value::from("path"), Value::from(format!("./ruleset/{name}.txt")));
    entry.insert(Value::from("interval"), Value::from(86_400u64));
    Value::Mapping(entry)
}

/// Notes for rules that came from a text rule set or were merged; `custom` precedes `subscription`.
fn notes_for(custom: &[(String, String)], subscription: &[String]) -> Vec<RuleNote> {
    let mut by_key: BTreeMap<(String, String, String), RuleNote> = BTreeMap::new();
    let rules = custom
        .iter()
        .map(|(name, rule)| (Some(name.as_str()), rule.as_str()))
        .chain(subscription.iter().map(|rule| (None, rule.as_str())));
    for (source, rule) in rules {
        let Some((kind, payload, policy)) = content_key(rule) else {
            continue;
        };
        match by_key.entry((kind.clone(), payload.clone(), policy.clone())) {
            Entry::Occupied(mut entry) => entry.get_mut().merged += 1,
            Entry::Vacant(entry) => {
                entry.insert(RuleNote {
                    kind,
                    payload,
                    policy,
                    source: source.map(str::to_owned),
                    merged: 0,
                });
            }
        }
    }
    by_key
        .into_values()
        .filter(|note| note.source.is_some() || note.merged > 0)
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn rules(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| (*line).to_owned()).collect()
    }

    #[test]
    fn decoupling_replaces_proxy_policies_and_keeps_direct_and_reject() {
        assert_eq!(
            decouple_rule("DOMAIN-SUFFIX,a.com,Proxy", "Default"),
            "DOMAIN-SUFFIX,a.com,Default"
        );
        assert_eq!(decouple_rule("GEOIP,CN,DIRECT", "Default"), "GEOIP,CN,DIRECT");
        assert_eq!(decouple_rule("MATCH,Proxy", "Default"), "MATCH,Default");
        assert_eq!(
            decouple_rule("IP-CIDR,10.0.0.0/8,Proxy,no-resolve", "Default"),
            "IP-CIDR,10.0.0.0/8,Default,no-resolve"
        );
    }

    #[test]
    fn routing_keeps_groups_and_moves_only_the_final_match_by_default() {
        let subscription = rules(&["DOMAIN-SUFFIX,a.com,Auto", "GEOIP,CN,DIRECT", "MATCH,Proxy"]);
        assert_eq!(
            route_subscription(&subscription, false, DEFAULT_GROUP),
            rules(&["DOMAIN-SUFFIX,a.com,Auto", "GEOIP,CN,DIRECT", "MATCH,clash5-default"])
        );
        assert_eq!(
            route_subscription(&subscription, true, DEFAULT_GROUP),
            rules(&[
                "DOMAIN-SUFFIX,a.com,clash5-default",
                "GEOIP,CN,DIRECT",
                "MATCH,clash5-default"
            ])
        );
    }

    #[test]
    fn duplicates_merge_and_new_text_rules_come_first() {
        let custom = rules(&[
            "DOMAIN-SUFFIX,ads.com,REJECT",
            "DOMAIN-SUFFIX,ads.com,REJECT",
            "DOMAIN-KEYWORD,tracker,REJECT",
        ]);
        let subscription = rules(&["DOMAIN-SUFFIX,ads.com,REJECT", "MATCH,Default"]);
        let (merged, report) = merge_rules(custom, subscription);
        assert_eq!(
            merged,
            rules(&[
                "DOMAIN-KEYWORD,tracker,REJECT",
                "DOMAIN-SUFFIX,ads.com,REJECT",
                "MATCH,Default"
            ])
        );
        assert_eq!(
            report,
            MergeReport {
                added: 1,
                merged: 2,
                conflicts: 0
            }
        );
    }

    #[test]
    fn same_content_with_another_policy_is_kept_and_counted_as_conflict() {
        let custom = rules(&["DOMAIN-SUFFIX,github.com,DIRECT"]);
        let subscription = rules(&["DOMAIN-SUFFIX,github.com,Default"]);
        let (merged, report) = merge_rules(custom, subscription);
        assert_eq!(merged.len(), 2);
        assert_eq!(
            report,
            MergeReport {
                added: 1,
                merged: 0,
                conflicts: 1
            }
        );
    }

    #[test]
    fn decoupling_switch_moves_every_policy_to_the_default_group() {
        let config: Mapping = serde_yaml_ng::from_str(
            "proxies: [{name: n1}]\nproxy-groups: [{name: Auto}, {name: Proxy}]\nrules: ['DOMAIN-SUFFIX,a.com,Auto', 'MATCH,Proxy']",
        )
        .unwrap();
        let rules_of = |config: &Mapping| -> Vec<String> {
            config["rules"]
                .as_sequence()
                .unwrap()
                .iter()
                .map(|rule| rule.as_str().unwrap().to_owned())
                .collect()
        };
        assert_eq!(
            rules_of(&apply(config.clone(), &[], true).0),
            rules(&["DOMAIN-SUFFIX,a.com,clash5-default", "MATCH,clash5-default"])
        );
        assert_eq!(
            rules_of(&apply(config, &[], false).0),
            rules(&["DOMAIN-SUFFIX,a.com,Auto", "MATCH,clash5-default"])
        );
    }

    #[test]
    fn notes_name_the_first_text_source_and_count_merges() {
        let custom = vec![
            ("广告".to_owned(), "DOMAIN-SUFFIX,ads.com,REJECT".to_owned()),
            ("广告".to_owned(), "DOMAIN-SUFFIX,ads.com,REJECT".to_owned()),
        ];
        let decoupled = rules(&["DOMAIN-SUFFIX,ads.com,REJECT", "MATCH,Proxy"]);
        let notes = notes_for(&custom, &decoupled);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].source.as_deref(), Some("广告"));
        assert_eq!(notes[0].merged, 2);
    }

    #[test]
    fn linked_sets_become_rule_providers_placed_before_subscription_rules() {
        let config: Mapping = serde_yaml_ng::from_str("proxies: [{name: n1}]\nrules: ['MATCH,Proxy']").unwrap();
        let sets = vec![TextRuleSet {
            id: "abc".into(),
            name: "广告".into(),
            target: RuleTarget::Reject,
            content: String::new(),
            url: Some("https://example.com/ads.txt".into()),
        }];
        let (out, pipeline) = apply(config, &sets, true);
        assert_eq!(
            out["rule-providers"]["clash5-set-abc"]["url"].as_str(),
            Some("https://example.com/ads.txt")
        );
        let rules: Vec<&str> = out["rules"]
            .as_sequence()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(rules, vec!["RULE-SET,clash5-set-abc,REJECT", "MATCH,clash5-default"]);
        assert_eq!(pipeline.notes[0].source.as_deref(), Some("广告"));
    }
}
