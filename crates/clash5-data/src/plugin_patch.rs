use crate::DataError;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub struct PluginPatch {
    pub binding_id: String,
    pub plugin_id: String,
    pub priority: i64,
    pub writable_paths: Vec<String>,
    pub patch: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConfigProvenance {
    Profile,
    Plugin {
        plugin_id: String,
        binding_id: String,
        #[specta(type = specta_typescript::Number)]
        priority: i64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectiveConfig {
    pub config: Value,
    /// JSON Pointer paths mapped to the source that produced the effective value.
    pub provenance: BTreeMap<String, ConfigProvenance>,
    pub conflicts: Vec<PluginConflict>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct PluginConflict {
    pub path: String,
    pub winner_plugin_id: String,
    pub winner_binding_id: String,
    #[specta(type = specta_typescript::Number)]
    pub winner_priority: i64,
    pub loser_plugin_id: String,
    pub loser_binding_id: String,
    #[specta(type = specta_typescript::Number)]
    pub loser_priority: i64,
}

/// Applies trusted plugin patches in priority order and retains a source for each field.
/// User-authored rules are reserved and cannot be changed by a plugin patch.
pub fn merge_plugin_patches(base: &Value, patches: &[PluginPatch]) -> Result<EffectiveConfig, DataError> {
    if !base.is_object() {
        return Err(DataError::InvalidInput);
    }
    let mut ordered = patches.iter().collect::<Vec<_>>();
    if ordered.iter().any(|candidate| {
        candidate.binding_id.trim().is_empty() || candidate.plugin_id.trim().is_empty() || !candidate.patch.is_object()
    }) {
        return Err(DataError::InvalidInput);
    }
    ordered.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.binding_id.cmp(&right.binding_id))
    });

    let mut config = base.clone();
    let mut provenance = BTreeMap::new();
    let mut locks = BTreeSet::new();
    let mut conflicts = Vec::new();
    collect_profile_paths(base, String::new(), &mut provenance);

    for candidate in ordered {
        let patch = candidate.patch.as_object().ok_or(DataError::InvalidInput)?;
        validate_plugin_patch(&candidate.patch, &candidate.writable_paths)?;
        for (key, value) in patch {
            if key == "rules" {
                return Err(DataError::InvalidInput);
            }
            apply_value(
                &mut config,
                value,
                std::slice::from_ref(key),
                candidate,
                &mut locks,
                &mut provenance,
                &mut conflicts,
            )?;
        }
    }

    conflicts.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.loser_binding_id.cmp(&right.loser_binding_id))
    });
    conflicts.dedup();
    Ok(EffectiveConfig {
        config,
        provenance,
        conflicts,
    })
}

pub fn validate_plugin_patch(patch: &Value, writable_paths: &[String]) -> Result<(), DataError> {
    let object = patch.as_object().ok_or(DataError::InvalidInput)?;
    if object.is_empty() {
        return Err(DataError::InvalidInput);
    }
    validate_writable_paths(writable_paths)?;
    validate_patch_object(object, "", writable_paths)
}

pub fn validate_writable_paths(writable_paths: &[String]) -> Result<(), DataError> {
    if writable_paths.is_empty() || writable_paths.iter().any(|path| !valid_pointer(path)) {
        return Err(DataError::InvalidInput);
    }
    Ok(())
}

fn validate_patch_object(
    object: &Map<String, Value>,
    parent: &str,
    writable_paths: &[String],
) -> Result<(), DataError> {
    for (key, value) in object {
        if key == "rules" {
            return Err(DataError::InvalidInput);
        }
        let path = join_pointer(parent, key);
        match value {
            Value::Object(children) if !children.is_empty() => {
                validate_patch_object(children, &path, writable_paths)?;
            }
            _ if writable_paths.iter().any(|allowed| {
                path == *allowed || path.strip_prefix(allowed).is_some_and(|suffix| suffix.starts_with('/'))
            }) => {}
            _ => return Err(DataError::InvalidInput),
        }
    }
    Ok(())
}

fn valid_pointer(pointer: &str) -> bool {
    pointer.starts_with('/')
        && pointer.len() > 1
        && !pointer.split('/').skip(1).any(|segment| segment.is_empty())
        && pointer
            .split('~')
            .skip(1)
            .all(|escape| matches!(escape.chars().next(), Some('0' | '1')))
}

fn collect_profile_paths(value: &Value, path: String, provenance: &mut BTreeMap<String, ConfigProvenance>) {
    match value {
        Value::Object(object) if !object.is_empty() => {
            for (key, child) in object {
                collect_profile_paths(child, join_pointer(&path, key), provenance);
            }
        }
        _ => {
            provenance.insert(path, ConfigProvenance::Profile);
        }
    }
}

fn apply_value(
    root: &mut Value,
    incoming: &Value,
    path: &[String],
    candidate: &PluginPatch,
    locks: &mut BTreeSet<Vec<String>>,
    provenance: &mut BTreeMap<String, ConfigProvenance>,
    conflicts: &mut Vec<PluginConflict>,
) -> Result<(), DataError> {
    if path.first().is_some_and(|segment| segment == "rules") {
        return Err(DataError::InvalidInput);
    }
    let blocking = locks
        .iter()
        .filter(|locked| is_prefix(locked, path))
        .max_by_key(|locked| locked.len());
    if let Some(locked) = blocking {
        record_conflict(locked, candidate, provenance, conflicts);
        return Ok(());
    }

    if let Value::Object(incoming_object) = incoming {
        if incoming_object.is_empty() {
            let descendants = locks
                .iter()
                .filter(|locked| is_prefix(path, locked))
                .cloned()
                .collect::<Vec<_>>();
            if !descendants.is_empty() {
                for locked in descendants {
                    record_conflict(&locked, candidate, provenance, conflicts);
                }
                return Ok(());
            }
            set_path(root, path, incoming.clone())?;
            locks.insert(path.to_vec());
            set_plugin_source(provenance, path, candidate);
            return Ok(());
        }
        for (key, value) in incoming_object {
            let mut child_path = path.to_vec();
            child_path.push(key.clone());
            apply_value(root, value, &child_path, candidate, locks, provenance, conflicts)?;
        }
        return Ok(());
    }

    let descendants = locks
        .iter()
        .filter(|locked| is_prefix(path, locked))
        .cloned()
        .collect::<Vec<_>>();
    if !descendants.is_empty() {
        for locked in descendants {
            record_conflict(&locked, candidate, provenance, conflicts);
        }
        return Ok(());
    }
    set_path(root, path, incoming.clone())?;
    locks.insert(path.to_vec());
    set_plugin_source(provenance, path, candidate);
    Ok(())
}

fn record_conflict(
    winner_path: &[String],
    loser: &PluginPatch,
    provenance: &BTreeMap<String, ConfigProvenance>,
    conflicts: &mut Vec<PluginConflict>,
) {
    let path = winner_path
        .iter()
        .fold(String::new(), |current, segment| join_pointer(&current, segment));
    let Some(ConfigProvenance::Plugin {
        plugin_id,
        binding_id,
        priority,
    }) = provenance.get(&path)
    else {
        return;
    };
    conflicts.push(PluginConflict {
        path,
        winner_plugin_id: plugin_id.clone(),
        winner_binding_id: binding_id.clone(),
        winner_priority: *priority,
        loser_plugin_id: loser.plugin_id.clone(),
        loser_binding_id: loser.binding_id.clone(),
        loser_priority: loser.priority,
    });
}

fn set_path(root: &mut Value, path: &[String], value: Value) -> Result<(), DataError> {
    let Some((last, parents)) = path.split_last() else {
        return Err(DataError::InvalidInput);
    };
    let mut current = root;
    for segment in parents {
        let object = current.as_object_mut().ok_or(DataError::InvalidInput)?;
        current = object
            .entry(segment.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        if !current.is_object() {
            *current = Value::Object(Map::new());
        }
    }
    let object = current.as_object_mut().ok_or(DataError::InvalidInput)?;
    object.insert(last.clone(), value);
    Ok(())
}

fn set_plugin_source(provenance: &mut BTreeMap<String, ConfigProvenance>, path: &[String], candidate: &PluginPatch) {
    let pointer = path
        .iter()
        .fold(String::new(), |current, segment| join_pointer(&current, segment));
    let prefix = format!("{pointer}/");
    provenance.retain(|existing, _| {
        existing != &pointer && !existing.starts_with(&prefix) && !pointer.starts_with(&format!("{existing}/"))
    });
    provenance.insert(
        pointer,
        ConfigProvenance::Plugin {
            plugin_id: candidate.plugin_id.clone(),
            binding_id: candidate.binding_id.clone(),
            priority: candidate.priority,
        },
    );
}

fn join_pointer(parent: &str, segment: &str) -> String {
    let escaped = segment.replace('~', "~0").replace('/', "~1");
    format!("{parent}/{escaped}")
}

fn is_prefix(prefix: &[String], path: &[String]) -> bool {
    prefix.len() <= path.len() && prefix.iter().zip(path).all(|(left, right)| left == right)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn patch(binding_id: &str, plugin_id: &str, priority: i64, patch: Value) -> PluginPatch {
        PluginPatch {
            binding_id: binding_id.into(),
            plugin_id: plugin_id.into(),
            priority,
            writable_paths: vec!["/tun".into()],
            patch,
        }
    }

    #[test]
    fn manifest_paths_limit_plugin_output() {
        let patch = json!({"dns": {"nameserver": ["1.1.1.1"], "fake-ip-range": "198.18.0.1/16"}});
        assert_eq!(
            validate_plugin_patch(&patch, &["/dns/nameserver".into()]),
            Err(DataError::InvalidInput)
        );
        assert_eq!(
            validate_plugin_patch(
                &json!({"dns": {"nameserver": ["1.1.1.1"]}}),
                &["/dns/nameserver".into()]
            ),
            Ok(())
        );
    }

    #[test]
    fn empty_plugin_set_still_reports_profile_field_sources() {
        let base = json!({"tun": {"enable": true, "mtu": 1280}});
        let effective = merge_plugin_patches(&base, &[]).unwrap();

        assert_eq!(effective.config, base);
        assert_eq!(effective.provenance["/tun/enable"], ConfigProvenance::Profile);
        assert_eq!(effective.provenance["/tun/mtu"], ConfigProvenance::Profile);
        assert!(effective.conflicts.is_empty());
    }

    #[test]
    fn priority_and_binding_id_choose_a_stable_winner_and_report_sources() {
        let base = json!({"tun": {"mtu": 1280}, "rules": ["DOMAIN,private,DIRECT"]});
        let patches = vec![
            patch(
                "z-binding",
                "plugin-z",
                80,
                json!({"tun": {"mtu": 1400, "stack": "system"}}),
            ),
            patch("a-binding", "plugin-a", 80, json!({"tun": {"mtu": 1350}})),
            patch("low-binding", "plugin-low", 10, json!({"tun": {"mtu": 1500}})),
        ];

        let merged = merge_plugin_patches(&base, &patches).unwrap();

        assert_eq!(merged.config["tun"]["mtu"], 1350);
        assert_eq!(merged.config["tun"]["stack"], "system");
        assert_eq!(merged.config["rules"], base["rules"]);
        assert_eq!(
            merged.provenance["/tun/mtu"],
            ConfigProvenance::Plugin {
                plugin_id: "plugin-a".into(),
                binding_id: "a-binding".into(),
                priority: 80,
            }
        );
        assert_eq!(
            merged.provenance["/tun/stack"],
            ConfigProvenance::Plugin {
                plugin_id: "plugin-z".into(),
                binding_id: "z-binding".into(),
                priority: 80,
            }
        );
        assert_eq!(merged.conflicts.len(), 2);
        assert!(merged.conflicts.iter().all(|conflict| {
            conflict.path == "/tun/mtu" && conflict.winner_binding_id == "a-binding" && conflict.winner_priority == 80
        }));
    }

    #[test]
    fn plugin_patches_cannot_write_user_owned_rules() {
        let base = json!({"rules": ["DOMAIN,private,DIRECT"]});
        let mut forbidden = patch("binding", "plugin", 1, json!({"rules": []}));
        forbidden.writable_paths = vec!["/rules".into()];

        assert_eq!(merge_plugin_patches(&base, &[forbidden]), Err(DataError::InvalidInput));
    }

    #[test]
    fn higher_priority_parent_value_wins_over_lower_nested_patch() {
        let base = json!({"tun": {"mtu": 1280, "stack": "gvisor"}});
        let patches = vec![
            patch("high", "plugin-high", 90, json!({"tun": false})),
            patch("low", "plugin-low", 10, json!({"tun": {"mtu": 1500}})),
        ];

        let merged = merge_plugin_patches(&base, &patches).unwrap();

        assert_eq!(merged.config["tun"], false);
        assert_eq!(
            merged.provenance["/tun"],
            ConfigProvenance::Plugin {
                plugin_id: "plugin-high".into(),
                binding_id: "high".into(),
                priority: 90,
            }
        );
        assert!(!merged.provenance.contains_key("/tun/mtu"));
        assert!(!merged.provenance.contains_key("/tun/stack"));
        assert_eq!(merged.conflicts.len(), 1);
        assert_eq!(merged.conflicts[0].path, "/tun");
        assert_eq!(merged.conflicts[0].loser_binding_id, "low");
    }
}
