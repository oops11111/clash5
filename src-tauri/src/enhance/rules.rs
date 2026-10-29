//! Rule pipeline applied to the subscription's own rules.
//!
//! Proxy policies in subscription rules are replaced by the default proxy: the
//! target of the subscription's final `MATCH` rule. `DIRECT` and `REJECT` stay.
//! Text rule sets are placed before the subscription rules, and a rule that is
//! already present after decoupling is merged instead of added twice.

use crate::config::{RuleTarget, TextRuleSet};
use clash_verge_logging::{Type, logging};
use serde_yaml_ng::{Mapping, Value};
use std::collections::{HashMap, HashSet};

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

fn policy_index(kind: &str) -> usize {
    if kind.eq_ignore_ascii_case("MATCH") { 1 } else { 2 }
}

fn is_kept(policy: &str) -> bool {
    KEPT_POLICIES.iter().any(|kept| policy.eq_ignore_ascii_case(kept))
}

/// The proxy policy a subscription rule names; `None` for `DIRECT`/`REJECT`.
fn proxy_policy(rule: &str) -> Option<&str> {
    let parts = fields(rule);
    let policy = *parts.get(policy_index(parts.first()?))?;
    (!is_kept(policy)).then_some(policy)
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

/// The subscription's `MATCH` target when it is a group, else the first group.
fn default_target(rules: &[&str], groups: &[String]) -> Option<String> {
    let final_policy = rules
        .iter()
        .rev()
        .find(|rule| {
            fields(rule)
                .first()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("MATCH"))
        })
        .and_then(|rule| proxy_policy(rule))
        .filter(|policy| groups.iter().any(|group| group.as_str() == *policy));
    final_policy.map(str::to_owned).or_else(|| groups.first().cloned())
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

fn target_policy(target: RuleTarget, default: &str) -> &str {
    match target {
        RuleTarget::Default => default,
        RuleTarget::Direct => "DIRECT",
        RuleTarget::Reject => "REJECT",
    }
}

/// Applies the rule pipeline to a subscription config before its rules-editor sequences.
pub(crate) fn apply(mut config: Mapping, text_rule_sets: &[TextRuleSet]) -> Mapping {
    let groups: Vec<String> = config
        .get("proxy-groups")
        .and_then(Value::as_sequence)
        .into_iter()
        .flatten()
        .filter_map(|group| group.as_mapping()?.get("name")?.as_str().map(str::to_owned))
        .collect();
    let rules_value = config.get("rules").and_then(Value::as_sequence);
    let subscription: Vec<String> = rules_value
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let rule_refs: Vec<&str> = subscription.iter().map(String::as_str).collect();

    let Some(default) = default_target(&rule_refs, &groups) else {
        return config;
    };

    let custom: Vec<String> = text_rule_sets
        .iter()
        .flat_map(|set| parse_text_rules(&set.content, target_policy(set.target, &default)).rules)
        .collect();
    let decoupled: Vec<String> = subscription.iter().map(|rule| decouple_rule(rule, &default)).collect();

    if decoupled.is_empty() && custom.is_empty() {
        return config;
    }

    let (merged, report) = merge_rules(custom, decoupled);
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
    config
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
    fn default_target_follows_the_final_match_then_the_first_group() {
        let groups = vec!["Auto".to_owned(), "Proxy".to_owned()];
        assert_eq!(default_target(&["MATCH,Proxy"], &groups).as_deref(), Some("Proxy"));
        assert_eq!(default_target(&["MATCH,DIRECT"], &groups).as_deref(), Some("Auto"));
        assert_eq!(default_target(&["MATCH,Missing"], &groups).as_deref(), Some("Auto"));
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
}
