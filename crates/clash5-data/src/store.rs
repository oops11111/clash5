use crate::{DataError, IdentityKey, Node, ProxyChain, StableId, identity::text};
use rusqlite::{Connection, DatabaseName, OptionalExtension, Transaction, backup::Progress, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::Duration,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    Manual,
    Subscription { provider_id: String },
}
impl Source {
    pub(crate) fn scope(&self) -> Result<(&str, &str), DataError> {
        match self {
            Self::Manual => Ok(("manual", "")),
            Self::Subscription { provider_id } => {
                text(provider_id)?;
                Ok(("subscription", provider_id))
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Binding {
    pub id: String,
    pub plugin_id: String,
    pub target_id: StableId,
    pub source: Source,
    #[specta(type = specta_typescript::Number)]
    pub priority: i64,
    pub enabled: bool,
    #[specta(type = specta_typescript::Unknown)]
    pub config: Value,
    #[serde(default)]
    pub orphan: bool,
    #[serde(default)]
    pub disappeared_at: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PluginBindingTarget {
    Node { source: Source, stable_id: StableId },
    Chain { chain_id: String },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct PluginBinding {
    pub id: String,
    pub plugin_id: String,
    pub target: PluginBindingTarget,
    #[specta(type = specta_typescript::Number)]
    pub priority: i64,
    pub enabled: bool,
    #[specta(type = specta_typescript::Unknown)]
    pub config: Value,
    #[serde(default)]
    pub orphan: bool,
    #[serde(default)]
    pub disappeared_at: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct PluginAuditEvent {
    #[specta(type = specta_typescript::Number)]
    pub id: i64,
    pub event: String,
    pub created_at: String,
    #[specta(type = specta_typescript::Unknown)]
    pub details: Value,
}
#[derive(Default, Debug, PartialEq, Eq, Serialize)]
pub struct ProviderDiff {
    pub matched: Vec<StableId>,
    pub added: Vec<StableId>,
    pub removed: Vec<StableId>,
    pub restored: Vec<StableId>,
    pub duplicates: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
pub struct NodeRecord {
    pub stable_id: StableId,
    pub source: Source,
    pub display_name: String,
    pub kind: String,
    pub server: String,
    pub port: u16,
    pub active: bool,
    #[specta(type = specta_typescript::Number)]
    pub duplicate_count: usize,
    pub quality: Option<NodeQuality>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct PendingIdentityRebind {
    pub source: Source,
    pub old_stable_id: StableId,
    pub candidate_ids: Vec<StableId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct OrphanCleanupPreview {
    pub retention_days: u16,
    #[specta(type = specta_typescript::Number)]
    pub total_orphans: usize,
    #[specta(type = specta_typescript::Number)]
    pub eligible_orphans: usize,
}

/// Complete raw definitions for one source during a planned identity-key rotation.
/// Callers must only provide a source snapshot after parsing it successfully.
pub struct IdentitySourceSnapshot {
    pub source: Source,
    pub nodes: Vec<Node>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum QualityStatus {
    Healthy,
    Slow,
    Failed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct NodeQuality {
    pub status: QualityStatus,
    pub short_delay_ms: Option<u32>,
    pub handshake_delay_ms: Option<u32>,
    #[specta(type = specta_typescript::Number)]
    pub checked_at_ms: u64,
    #[serde(default)]
    pub failure_streak: u8,
}
impl NodeQuality {
    pub const MIN_INTERVAL_MS: u64 = 600_000;
    pub const MAX_INTERVAL_MS: u64 = 3_600_000;

    pub fn from_probe(short_delay_ms: Option<u32>, handshake_delay_ms: Option<u32>, checked_at_ms: u64) -> Self {
        Self::from_probe_after(short_delay_ms, handshake_delay_ms, checked_at_ms, None)
    }

    pub fn from_probe_after(
        short_delay_ms: Option<u32>,
        handshake_delay_ms: Option<u32>,
        checked_at_ms: u64,
        previous: Option<&Self>,
    ) -> Self {
        let status = match (short_delay_ms, handshake_delay_ms) {
            (Some(_), Some(_)) => QualityStatus::Healthy,
            (None, Some(_)) => QualityStatus::Slow,
            _ => QualityStatus::Failed,
        };
        let failure_streak = if status == QualityStatus::Failed {
            previous
                .filter(|quality| quality.status == QualityStatus::Failed)
                .map_or(1, |quality| quality.failure_streak.saturating_add(1))
        } else {
            0
        };
        Self {
            status,
            short_delay_ms,
            handshake_delay_ms,
            checked_at_ms,
            failure_streak,
        }
    }

    pub fn is_fresh(&self, now_ms: u64) -> bool {
        let interval = Self::MIN_INTERVAL_MS
            .saturating_mul(1_u64 << self.failure_streak.min(3))
            .min(Self::MAX_INTERVAL_MS);
        now_ms >= self.checked_at_ms && now_ms - self.checked_at_ms < interval
    }
}
pub struct BindingStore {
    db: Connection,
    identity_key: Option<IdentityKey>,
    identity_generation: Option<i64>,
    identity_recovery: bool,
}

fn audit(tx: &Transaction<'_>, event: &str, counts: Value) -> Result<(), DataError> {
    tx.execute(
        "INSERT INTO audit_log(event,counts) VALUES(?1,?2)",
        params![event, counts.to_string()],
    )?;
    Ok(())
}
fn write_node(
    tx: &Transaction<'_>,
    source: &Source,
    id: &str,
    node: &Node,
    duplicate_count: usize,
    identity_generation: Option<i64>,
) -> Result<(), DataError> {
    let (kind, provider) = source.scope()?;
    tx.execute("INSERT INTO nodes(source_kind,provider_id,stable_id,display_name,active,kind,server,port,duplicate_count,stable_id_version,key_generation,transport,sni,identity_context_known) VALUES(?1,?2,?3,?4,1,?5,?6,?7,?8,?9,?10,?11,?12,1) ON CONFLICT(source_kind,provider_id,stable_id) DO UPDATE SET display_name=excluded.display_name,active=1,kind=excluded.kind,server=excluded.server,port=excluded.port,duplicate_count=excluded.duplicate_count,stable_id_version=excluded.stable_id_version,key_generation=excluded.key_generation,transport=excluded.transport,sni=excluded.sni,identity_context_known=1",params![kind,provider,id,node.display_name,node.kind,node.server,node.port,duplicate_count,if id.starts_with("v2:") {2} else {1},identity_generation,node.transport,node.sni])?;
    tx.execute("UPDATE bindings SET orphan=0,disappeared_at=NULL WHERE source_kind=?1 AND provider_id=?2 AND target_id=?3 AND orphan=1",params![kind,provider,id])?;
    tx.execute("UPDATE plugin_bindings SET orphan=0,disappeared_at=NULL WHERE target_type='node' AND source_kind=?1 AND provider_id=?2 AND target_id=?3 AND orphan=1",params![kind,provider,id])?;
    Ok(())
}

fn migrate_exact_v1_identity(
    tx: &Transaction<'_>,
    source: &Source,
    node: &Node,
    old_id: &str,
    new_id: &str,
    duplicate_count: usize,
    generation: i64,
) -> Result<bool, DataError> {
    if old_id == new_id {
        return Ok(false);
    }
    let (kind, provider) = source.scope()?;
    let old_context: Option<(String, String, u16, String, String, bool)> = tx.query_row(
        "SELECT kind,server,port,transport,sni,identity_context_known FROM nodes WHERE source_kind=?1 AND provider_id=?2 AND stable_id=?3 AND stable_id_version=1",
        params![kind,provider,old_id],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?)),
    ).optional()?;
    let Some((old_kind, old_server, old_port, old_transport, old_sni, context_known)) = old_context else {
        return Ok(false);
    };
    if !context_known {
        return Ok(false);
    }
    let old_node = Node {
        kind: old_kind,
        server: old_server,
        port: old_port,
        transport: old_transport,
        sni: old_sni,
        credentials: BTreeMap::new(),
        display_name: String::new(),
    };
    if old_node.endpoint_compatibility()? != node.endpoint_compatibility()? {
        return Ok(false);
    }
    let new_exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM nodes WHERE source_kind=?1 AND provider_id=?2 AND stable_id=?3)",
        params![kind, provider, new_id],
        |row| row.get(0),
    )?;
    if new_exists {
        return Err(DataError::Database);
    }
    tx.execute(
        "INSERT INTO nodes(source_kind,provider_id,stable_id,display_name,active,kind,server,port,duplicate_count,stable_id_version,key_generation,transport,sni,identity_context_known) VALUES(?1,?2,?3,?4,1,?5,?6,?7,?8,2,?9,?10,?11,1)",
        params![kind,provider,new_id,node.display_name,node.kind,node.server,node.port,duplicate_count,generation,node.transport,node.sni],
    )?;
    tx.execute(
        "UPDATE bindings SET target_id=?4,orphan=0,disappeared_at=NULL WHERE source_kind=?1 AND provider_id=?2 AND target_id=?3",
        params![kind,provider,old_id,new_id],
    )?;
    tx.execute(
        "INSERT INTO quality_cache(source_kind,provider_id,stable_id,quality) SELECT source_kind,provider_id,?4,quality FROM quality_cache WHERE source_kind=?1 AND provider_id=?2 AND stable_id=?3",
        params![kind,provider,old_id,new_id],
    )?;
    tx.execute(
        "DELETE FROM quality_cache WHERE source_kind=?1 AND provider_id=?2 AND stable_id=?3",
        params![kind, provider, old_id],
    )?;
    tx.execute(
        "DELETE FROM nodes WHERE source_kind=?1 AND provider_id=?2 AND stable_id=?3",
        params![kind, provider, old_id],
    )?;
    Ok(true)
}

fn reconcile_rotated_credentials(
    tx: &Transaction<'_>,
    provider_id: &str,
    next: &BTreeMap<StableId, (&Node, usize)>,
    current_generation: i64,
) -> Result<(), DataError> {
    let old_nodes: Vec<(String, i64, Node)> = {
        let mut statement = tx.prepare(
            "SELECT stable_id,key_generation,kind,server,port,transport,sni FROM nodes WHERE source_kind='subscription' AND provider_id=?1 AND identity_context_known=1",
        )?;
        statement
            .query_map([provider_id], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    Node {
                        kind: row.get(2)?,
                        server: row.get(3)?,
                        port: row.get(4)?,
                        transport: row.get(5)?,
                        sni: row.get(6)?,
                        credentials: BTreeMap::new(),
                        display_name: String::new(),
                    },
                ))
            })?
            .collect::<Result<_, _>>()?
    };
    let mut candidates: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut target_counts: BTreeMap<String, usize> = BTreeMap::new();
    for (old_id, old_generation, old_node) in old_nodes {
        if old_generation != current_generation || next.contains_key(&old_id) || !old_id.starts_with("v2:") {
            continue;
        }
        let old_endpoint = old_node.endpoint_compatibility()?;
        for (new_id, (new_node, _)) in next {
            if old_id != *new_id && old_endpoint == new_node.endpoint_compatibility()? {
                candidates.entry(old_id.clone()).or_default().push(new_id.clone());
                *target_counts.entry(new_id.clone()).or_default() += 1;
            }
        }
    }
    for (old_id, mut new_ids) in candidates {
        new_ids.sort();
        new_ids.dedup();
        if new_ids.len() == 1 && target_counts.get(&new_ids[0]) == Some(&1) {
            tx.execute("UPDATE bindings SET target_id=?3,orphan=0,disappeared_at=NULL WHERE source_kind='subscription' AND provider_id=?1 AND target_id=?2", params![provider_id,old_id,new_ids[0]])?;
            tx.execute(
                "DELETE FROM quality_cache WHERE source_kind='subscription' AND provider_id=?1 AND stable_id=?2",
                params![provider_id, old_id],
            )?;
            tx.execute("DELETE FROM pending_identity_rebinds WHERE source_kind='subscription' AND provider_id=?1 AND old_stable_id=?2", params![provider_id,old_id])?;
        } else {
            let encoded = serde_json::to_string(&new_ids).map_err(|_| DataError::InvalidInput)?;
            tx.execute("INSERT INTO pending_identity_rebinds(source_kind,provider_id,old_stable_id,candidate_ids) VALUES('subscription',?1,?2,?3) ON CONFLICT(source_kind,provider_id,old_stable_id) DO UPDATE SET candidate_ids=excluded.candidate_ids,created_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')", params![provider_id,old_id,encoded])?;
        }
    }
    Ok(())
}

impl BindingStore {
    pub const NAME_MATCHING_ENABLED: bool = false;
    pub const DEFAULT_ORPHAN_RETENTION_DAYS: u16 = 30;
    pub const MIN_ORPHAN_RETENTION_DAYS: u16 = 1;
    pub const MAX_ORPHAN_RETENTION_DAYS: u16 = 365;

    pub fn open(path: impl AsRef<Path>) -> Result<Self, DataError> {
        Self::initialize(Connection::open(path)?)
    }
    pub fn open_with_identity(path: impl AsRef<Path>, key: IdentityKey) -> Result<Self, DataError> {
        let mut store = Self::initialize(Connection::open(path)?)?;
        store.install_identity_key(key)?;
        Ok(store)
    }
    pub fn memory() -> Result<Self, DataError> {
        Self::initialize(Connection::open_in_memory()?)
    }
    pub fn memory_with_identity(key: IdentityKey) -> Result<Self, DataError> {
        let mut store = Self::initialize(Connection::open_in_memory()?)?;
        store.install_identity_key(key)?;
        Ok(store)
    }

    pub fn snapshot_to(&self, path: impl AsRef<Path>) -> Result<(), DataError> {
        self.db.backup(DatabaseName::Main, path, None)?;
        Ok(())
    }

    pub fn restore_from(&mut self, path: impl AsRef<Path>) -> Result<(), DataError> {
        self.db.restore(DatabaseName::Main, path, None::<fn(Progress)>)?;
        Ok(())
    }

    fn install_identity_key(&mut self, key: IdentityKey) -> Result<(), DataError> {
        let active: Option<(i64, String)> = self
            .db
            .query_row(
                "SELECT generation,key_id FROM identity_keys WHERE active=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let key_id = key.key_id();
        let generation = match active {
            Some((generation, stored_key_id)) if stored_key_id == key_id => generation,
            Some(_) => return Err(DataError::IdentityRecovery),
            None => {
                let hmac_nodes: i64 =
                    self.db
                        .query_row("SELECT count(*) FROM nodes WHERE stable_id_version=2", [], |row| {
                            row.get(0)
                        })?;
                if hmac_nodes != 0 {
                    return Err(DataError::IdentityRecovery);
                }
                let tx = self
                    .db
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                let generation: i64 =
                    tx.query_row("SELECT COALESCE(MAX(generation),0)+1 FROM identity_keys", [], |row| {
                        row.get(0)
                    })?;
                tx.execute(
                    "INSERT INTO identity_keys(generation,key_id,active) VALUES(?1,?2,1)",
                    params![generation, key_id],
                )?;
                tx.commit()?;
                generation
            }
        };
        self.identity_key = Some(key);
        self.identity_generation = Some(generation);
        self.identity_recovery = false;
        Ok(())
    }

    pub fn active_identity_key(&self) -> Result<Option<(i64, String)>, DataError> {
        self.db
            .query_row(
                "SELECT generation,key_id FROM identity_keys WHERE active=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn next_identity_generation(&self) -> Result<i64, DataError> {
        self.db
            .query_row("SELECT COALESCE(MAX(generation),0)+1 FROM identity_keys", [], |row| {
                row.get(0)
            })
            .map_err(Into::into)
    }

    pub fn has_v2_identities(&self) -> Result<bool, DataError> {
        self.db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM nodes WHERE stable_id_version=2)",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    fn identity_for(&self, node: &Node) -> Result<StableId, DataError> {
        match &self.identity_key {
            Some(key) => node.stable_id_v2(key),
            None => node.stable_id(),
        }
    }

    fn ensure_writable(&self) -> Result<(), DataError> {
        if self.identity_recovery {
            Err(DataError::IdentityRecovery)
        } else {
            Ok(())
        }
    }

    pub fn identity_recovery(&self) -> bool {
        self.identity_recovery
    }

    pub fn enter_identity_recovery(&mut self) {
        self.identity_recovery = true;
    }

    pub fn reset_identity_key(&mut self, key: IdentityKey) -> Result<i64, DataError> {
        if !self.identity_recovery {
            return Err(DataError::InvalidInput);
        }
        let key_id = key.key_id();
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let generation: i64 = tx.query_row("SELECT COALESCE(MAX(generation),0)+1 FROM identity_keys", [], |row| {
            row.get(0)
        })?;
        tx.execute("UPDATE identity_keys SET active=0 WHERE active=1", [])?;
        tx.execute(
            "INSERT INTO identity_keys(generation,key_id,active) VALUES(?1,?2,1)",
            params![generation, key_id],
        )?;
        let nodes = tx.execute(
            "UPDATE nodes SET active=0 WHERE stable_id_version=2 AND key_generation<>?1",
            [generation],
        )?;
        let bindings = tx.execute("UPDATE bindings SET orphan=1,disappeared_at=COALESCE(disappeared_at,strftime('%Y-%m-%dT%H:%M:%fZ','now')) WHERE EXISTS(SELECT 1 FROM nodes n WHERE n.source_kind=bindings.source_kind AND n.provider_id=bindings.provider_id AND n.stable_id=bindings.target_id AND n.stable_id_version=2 AND n.key_generation<>?1)",[generation])?;
        tx.execute("DELETE FROM pending_identity_rebinds", [])?;
        audit(
            &tx,
            "identity.reset",
            serde_json::json!({"nodes":nodes,"bindings":bindings}),
        )?;
        tx.commit()?;
        self.identity_key = Some(key);
        self.identity_generation = Some(generation);
        self.identity_recovery = false;
        Ok(generation)
    }

    /// Rotate the installation key while the current key is still available.
    /// Exact source definitions are re-keyed in one SQLite transaction; unmatched
    /// old identities remain as inactive orphan records for explicit recovery.
    pub fn rotate_identity_key(
        &mut self,
        new_key: IdentityKey,
        snapshots: &[IdentitySourceSnapshot],
    ) -> Result<i64, DataError> {
        self.ensure_writable()?;
        let old_key = self.identity_key.as_ref().ok_or(DataError::IdentityRecovery)?;
        if old_key.key_id() == new_key.key_id() {
            return Err(DataError::InvalidInput);
        }
        let old_generation = self.identity_generation.ok_or(DataError::IdentityRecovery)?;
        let mut seen_sources = std::collections::BTreeSet::new();
        for snapshot in snapshots {
            let scope = snapshot.source.scope()?;
            if !seen_sources.insert((scope.0.to_owned(), scope.1.to_owned())) {
                return Err(DataError::InvalidInput);
            }
        }

        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let new_generation: i64 =
            tx.query_row("SELECT COALESCE(MAX(generation),0)+1 FROM identity_keys", [], |row| {
                row.get(0)
            })?;
        let mut migrated_nodes = 0usize;
        let mut migrated_bindings = 0usize;
        let mut unresolved_bindings = 0usize;
        let mut rekeyed_ids = BTreeMap::<(String, String, StableId), StableId>::new();

        for snapshot in snapshots {
            let (kind, provider) = snapshot.source.scope()?;
            let mut next = BTreeMap::<StableId, (&Node, usize, StableId)>::new();
            for node in &snapshot.nodes {
                let old_id = node.stable_id_v2(old_key)?;
                let new_id = node.stable_id_v2(&new_key)?;
                if old_id != new_id {
                    rekeyed_ids.insert((kind.to_owned(), provider.to_owned(), old_id.clone()), new_id.clone());
                }
                next.entry(new_id)
                    .and_modify(|(selected, count, _)| {
                        *count += 1;
                        if node.display_name < selected.display_name {
                            *selected = node;
                        }
                    })
                    .or_insert((node, 1, old_id));
            }

            for (new_id, (node, duplicate_count, old_id)) in next {
                if new_id == old_id {
                    return Err(DataError::InvalidInput);
                }
                let old_matches: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM nodes WHERE source_kind=?1 AND provider_id=?2 AND stable_id=?3 AND stable_id_version=2 AND key_generation=?4 AND identity_context_known=1)",
                    params![kind, provider, old_id, old_generation],
                    |row| row.get(0),
                )?;
                if old_matches {
                    let context: (String, String, u16, String, String) = tx.query_row(
                        "SELECT kind,server,port,transport,sni FROM nodes WHERE source_kind=?1 AND provider_id=?2 AND stable_id=?3",
                        params![kind, provider, old_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
                    )?;
                    let previous = Node {
                        kind: context.0,
                        server: context.1,
                        port: context.2,
                        transport: context.3,
                        sni: context.4,
                        credentials: BTreeMap::new(),
                        display_name: String::new(),
                    };
                    if previous.endpoint_compatibility()? != node.endpoint_compatibility()? {
                        return Err(DataError::Database);
                    }
                    let collision: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM nodes WHERE source_kind=?1 AND provider_id=?2 AND stable_id=?3)",
                        params![kind, provider, new_id],
                        |row| row.get(0),
                    )?;
                    if collision {
                        return Err(DataError::Database);
                    }
                    write_node(
                        &tx,
                        &snapshot.source,
                        &new_id,
                        node,
                        duplicate_count,
                        Some(new_generation),
                    )?;
                    migrated_bindings += tx.execute(
                        "UPDATE bindings SET target_id=?4,orphan=0,disappeared_at=NULL WHERE source_kind=?1 AND provider_id=?2 AND target_id=?3",
                        params![kind, provider, old_id, new_id],
                    )?;
                    tx.execute(
                        "UPDATE quality_cache SET stable_id=?4 WHERE source_kind=?1 AND provider_id=?2 AND stable_id=?3",
                        params![kind, provider, old_id, new_id],
                    )?;
                    tx.execute(
                        "DELETE FROM pending_identity_rebinds WHERE source_kind=?1 AND provider_id=?2 AND old_stable_id=?3",
                        params![kind, provider, old_id],
                    )?;
                    tx.execute(
                        "DELETE FROM nodes WHERE source_kind=?1 AND provider_id=?2 AND stable_id=?3",
                        params![kind, provider, old_id],
                    )?;
                    migrated_nodes += 1;
                } else {
                    let old_v1_id = node.stable_id()?;
                    let old_v1_bindings: usize = tx.query_row(
                        "SELECT count(*) FROM bindings WHERE source_kind=?1 AND provider_id=?2 AND target_id=?3",
                        params![kind, provider, old_v1_id],
                        |row| row.get(0),
                    )?;
                    if migrate_exact_v1_identity(
                        &tx,
                        &snapshot.source,
                        node,
                        &old_v1_id,
                        &new_id,
                        duplicate_count,
                        new_generation,
                    )? {
                        migrated_nodes += 1;
                        migrated_bindings += old_v1_bindings;
                    } else {
                        write_node(
                            &tx,
                            &snapshot.source,
                            &new_id,
                            node,
                            duplicate_count,
                            Some(new_generation),
                        )?;
                    }
                }
            }
        }

        let stored_chains = {
            let mut statement = tx.prepare(
                "SELECT id,definition FROM chains WHERE json_extract(definition,'$.schema_version') IS NOT NULL",
            )?;
            statement
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut chains_rekeyed = 0usize;
        for (id, definition) in stored_chains {
            let mut chain: ProxyChain = serde_json::from_str(&definition).map_err(|_| DataError::Database)?;
            let mut changed = false;
            for reference in [&mut chain.front, &mut chain.landing] {
                let (kind, provider) = reference.source.scope()?;
                if let Some(new_id) =
                    rekeyed_ids.get(&(kind.to_owned(), provider.to_owned(), reference.stable_id.clone()))
                {
                    reference.stable_id = new_id.clone();
                    changed = true;
                }
            }
            if changed {
                chain.validate()?;
                let definition = serde_json::to_string(&chain).map_err(|_| DataError::InvalidInput)?;
                tx.execute("UPDATE chains SET definition=?2 WHERE id=?1", params![id, definition])?;
                chains_rekeyed += 1;
            }
        }

        tx.execute("UPDATE identity_keys SET active=0 WHERE active=1", [])?;
        tx.execute(
            "INSERT INTO identity_keys(generation,key_id,active) VALUES(?1,?2,1)",
            params![new_generation, new_key.key_id()],
        )?;
        tx.execute(
            "UPDATE nodes SET active=0 WHERE stable_id_version=2 AND key_generation=?1",
            [old_generation],
        )?;
        tx.execute("UPDATE nodes SET active=0 WHERE stable_id_version=1", [])?;
        unresolved_bindings += tx.execute(
            "UPDATE bindings SET orphan=1,disappeared_at=COALESCE(disappeared_at,strftime('%Y-%m-%dT%H:%M:%fZ','now')) WHERE EXISTS(SELECT 1 FROM nodes n WHERE n.source_kind=bindings.source_kind AND n.provider_id=bindings.provider_id AND n.stable_id=bindings.target_id AND n.stable_id_version=2 AND n.key_generation=?1)",
            [old_generation],
        )?;
        unresolved_bindings += tx.execute(
            "UPDATE bindings SET orphan=1,disappeared_at=COALESCE(disappeared_at,strftime('%Y-%m-%dT%H:%M:%fZ','now')) WHERE EXISTS(SELECT 1 FROM nodes n WHERE n.source_kind=bindings.source_kind AND n.provider_id=bindings.provider_id AND n.stable_id=bindings.target_id AND n.stable_id_version=1)",
            [],
        )?;
        tx.execute("DELETE FROM pending_identity_rebinds", [])?;
        audit(
            &tx,
            "identity.rotate",
            serde_json::json!({
                "nodes_migrated": migrated_nodes,
                "bindings_migrated": migrated_bindings,
                "bindings_orphaned": unresolved_bindings,
                "chains_rekeyed": chains_rekeyed
            }),
        )?;
        tx.commit()?;
        self.identity_key = Some(new_key);
        self.identity_generation = Some(new_generation);
        Ok(new_generation)
    }

    pub fn identity_key(&self) -> Result<&IdentityKey, DataError> {
        self.identity_key.as_ref().ok_or(DataError::IdentityRecovery)
    }
    fn initialize(mut db: Connection) -> Result<Self, DataError> {
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "foreign_keys", true)?;
        db.busy_timeout(Duration::from_secs(5))?;
        let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if !(0..=5).contains(&version) {
            return Err(DataError::SchemaVersion);
        }
        let mut version = version;
        if version == 0 {
            let tx = db.transaction()?;
            tx.execute_batch(
                r#"
CREATE TABLE nodes (
 source_kind TEXT NOT NULL CHECK(source_kind IN ('manual','subscription')),
 provider_id TEXT NOT NULL, stable_id TEXT NOT NULL, display_name TEXT NOT NULL,
 kind TEXT NOT NULL DEFAULT '', server TEXT NOT NULL DEFAULT '',
 port INTEGER NOT NULL DEFAULT 0, duplicate_count INTEGER NOT NULL DEFAULT 1 CHECK(duplicate_count>0),
 stable_id_version INTEGER NOT NULL DEFAULT 1 CHECK(stable_id_version IN (1,2)),
 key_generation INTEGER, transport TEXT NOT NULL DEFAULT '', sni TEXT NOT NULL DEFAULT '',
 identity_context_known INTEGER NOT NULL DEFAULT 0 CHECK(identity_context_known IN (0,1)),
 active INTEGER NOT NULL CHECK(active IN (0,1)),
 CHECK((stable_id_version=1 AND key_generation IS NULL) OR (stable_id_version=2 AND key_generation IS NOT NULL)),
 CHECK((source_kind='manual' AND provider_id='') OR (source_kind='subscription' AND length(provider_id)>0)),
 PRIMARY KEY(source_kind,provider_id,stable_id)
) STRICT;
CREATE TABLE bindings (
 source_kind TEXT NOT NULL, provider_id TEXT NOT NULL, id TEXT NOT NULL,
 plugin_id TEXT NOT NULL, target_id TEXT NOT NULL, priority INTEGER NOT NULL,
 enabled INTEGER NOT NULL CHECK(enabled IN (0,1)), config TEXT NOT NULL CHECK(json_valid(config)),
 orphan INTEGER NOT NULL CHECK(orphan IN (0,1)), disappeared_at TEXT,
 CHECK((orphan=0 AND disappeared_at IS NULL) OR (orphan=1 AND disappeared_at IS NOT NULL)),
 PRIMARY KEY(source_kind,provider_id,id),
 FOREIGN KEY(source_kind,provider_id,target_id) REFERENCES nodes(source_kind,provider_id,stable_id)
) STRICT;
CREATE INDEX bindings_target ON bindings(source_kind,provider_id,target_id);
CREATE TABLE plugin_bindings(
 target_type TEXT NOT NULL CHECK(target_type IN ('node','chain')),
 source_kind TEXT NOT NULL CHECK(source_kind IN ('manual','subscription','profile')),
 provider_id TEXT NOT NULL, target_id TEXT NOT NULL, binding_id TEXT NOT NULL,
 plugin_id TEXT NOT NULL, priority INTEGER NOT NULL,
 enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
 config TEXT NOT NULL CHECK(json_valid(config)),
 orphan INTEGER NOT NULL CHECK(orphan IN (0,1)), disappeared_at TEXT,
 CHECK((orphan=0 AND disappeared_at IS NULL) OR (orphan=1 AND disappeared_at IS NOT NULL)),
 CHECK((target_type='node' AND source_kind IN ('manual','subscription')) OR (target_type='chain' AND source_kind='profile')),
 CHECK((source_kind='manual' AND provider_id='') OR (source_kind='subscription' AND length(provider_id)>0) OR source_kind='profile'),
 PRIMARY KEY(target_type,source_kind,provider_id,target_id,binding_id)
) STRICT;
CREATE INDEX plugin_bindings_target ON plugin_bindings(target_type,source_kind,provider_id,target_id,enabled,orphan);
CREATE TABLE chains(id TEXT PRIMARY KEY, definition TEXT NOT NULL CHECK(json_valid(definition))) STRICT;
CREATE TABLE quality_cache(
 source_kind TEXT NOT NULL, provider_id TEXT NOT NULL, stable_id TEXT NOT NULL,
 quality TEXT NOT NULL CHECK(json_valid(quality)), PRIMARY KEY(source_kind,provider_id,stable_id),
 FOREIGN KEY(source_kind,provider_id,stable_id) REFERENCES nodes(source_kind,provider_id,stable_id)
) STRICT;
CREATE TABLE audit_log(id INTEGER PRIMARY KEY,event TEXT NOT NULL,
 created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 counts TEXT NOT NULL CHECK(json_valid(counts))) STRICT;
CREATE TABLE identity_keys (
 generation INTEGER PRIMARY KEY CHECK(generation>0), key_id TEXT NOT NULL UNIQUE,
 active INTEGER NOT NULL CHECK(active IN (0,1)),
 created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
CREATE UNIQUE INDEX identity_keys_one_active ON identity_keys(active) WHERE active=1;
CREATE TABLE pending_identity_rebinds (
 source_kind TEXT NOT NULL CHECK(source_kind='subscription'), provider_id TEXT NOT NULL,
 old_stable_id TEXT NOT NULL, candidate_ids TEXT NOT NULL CHECK(json_valid(candidate_ids)),
 created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 PRIMARY KEY(source_kind,provider_id,old_stable_id)
) STRICT;
PRAGMA user_version=4;
"#,
            )?;
            tx.commit()?;
            version = 4;
        } else if version == 1 {
            let tx = db.transaction()?;
            tx.execute_batch(
                "ALTER TABLE nodes ADD COLUMN kind TEXT NOT NULL DEFAULT '';\
                 ALTER TABLE nodes ADD COLUMN server TEXT NOT NULL DEFAULT '';\
                 ALTER TABLE nodes ADD COLUMN port INTEGER NOT NULL DEFAULT 0;\
                 ALTER TABLE nodes ADD COLUMN duplicate_count INTEGER NOT NULL DEFAULT 1 CHECK(duplicate_count>0);\
                 PRAGMA user_version=2;",
            )?;
            tx.commit()?;
            version = 2;
        }
        if version == 2 {
            let tx = db.transaction()?;
            tx.execute_batch(
                r#"
CREATE TABLE plugin_bindings(
 target_type TEXT NOT NULL CHECK(target_type IN ('node','chain')),
 source_kind TEXT NOT NULL CHECK(source_kind IN ('manual','subscription','profile')),
 provider_id TEXT NOT NULL, target_id TEXT NOT NULL, binding_id TEXT NOT NULL,
 plugin_id TEXT NOT NULL, priority INTEGER NOT NULL,
 enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
 config TEXT NOT NULL CHECK(json_valid(config)),
 orphan INTEGER NOT NULL CHECK(orphan IN (0,1)), disappeared_at TEXT,
 CHECK((orphan=0 AND disappeared_at IS NULL) OR (orphan=1 AND disappeared_at IS NOT NULL)),
 CHECK((target_type='node' AND source_kind IN ('manual','subscription')) OR (target_type='chain' AND source_kind='profile')),
 CHECK((source_kind='manual' AND provider_id='') OR (source_kind='subscription' AND length(provider_id)>0) OR source_kind='profile'),
 PRIMARY KEY(target_type,source_kind,provider_id,target_id,binding_id)
) STRICT;
CREATE INDEX plugin_bindings_target ON plugin_bindings(target_type,source_kind,provider_id,target_id,enabled,orphan);
PRAGMA user_version=3;
"#,
            )?;
            tx.commit()?;
        }
        if version > 0 && version < 4 {
            let tx = db.transaction()?;
            tx.execute_batch(
                r#"
ALTER TABLE nodes ADD COLUMN stable_id_version INTEGER NOT NULL DEFAULT 1 CHECK(stable_id_version IN (1,2));
ALTER TABLE nodes ADD COLUMN key_generation INTEGER;
ALTER TABLE nodes ADD COLUMN transport TEXT NOT NULL DEFAULT '';
ALTER TABLE nodes ADD COLUMN sni TEXT NOT NULL DEFAULT '';
ALTER TABLE nodes ADD COLUMN identity_context_known INTEGER NOT NULL DEFAULT 0 CHECK(identity_context_known IN (0,1));
CREATE TABLE identity_keys (
 generation INTEGER PRIMARY KEY CHECK(generation>0),
 key_id TEXT NOT NULL UNIQUE,
 active INTEGER NOT NULL CHECK(active IN (0,1)),
 created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
CREATE UNIQUE INDEX identity_keys_one_active ON identity_keys(active) WHERE active=1;
CREATE TABLE pending_identity_rebinds (
 source_kind TEXT NOT NULL CHECK(source_kind='subscription'),
 provider_id TEXT NOT NULL,
 old_stable_id TEXT NOT NULL,
 candidate_ids TEXT NOT NULL CHECK(json_valid(candidate_ids)),
 created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 PRIMARY KEY(source_kind,provider_id,old_stable_id)
) STRICT;
PRAGMA user_version=4;
"#,
            )?;
            tx.commit()?;
        }
        if version < 5 {
            let tx = db.transaction()?;
            tx.execute_batch(
                r#"
CREATE TABLE IF NOT EXISTS app_settings (setting_key TEXT PRIMARY KEY, setting_value TEXT NOT NULL) STRICT;
INSERT OR IGNORE INTO app_settings(setting_key,setting_value) VALUES('orphan_retention_days','30');
CREATE TABLE IF NOT EXISTS plugin_bindings(
 target_type TEXT NOT NULL CHECK(target_type IN ('node','chain')),
 source_kind TEXT NOT NULL CHECK(source_kind IN ('manual','subscription','profile')),
 provider_id TEXT NOT NULL, target_id TEXT NOT NULL, binding_id TEXT NOT NULL,
 plugin_id TEXT NOT NULL, priority INTEGER NOT NULL,
 enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
 config TEXT NOT NULL CHECK(json_valid(config)),
 orphan INTEGER NOT NULL CHECK(orphan IN (0,1)), disappeared_at TEXT,
 CHECK((orphan=0 AND disappeared_at IS NULL) OR (orphan=1 AND disappeared_at IS NOT NULL)),
 CHECK((target_type='node' AND source_kind IN ('manual','subscription')) OR (target_type='chain' AND source_kind='profile')),
 CHECK((source_kind='manual' AND provider_id='') OR (source_kind='subscription' AND length(provider_id)>0) OR source_kind='profile'),
 PRIMARY KEY(target_type,source_kind,provider_id,target_id,binding_id)
) STRICT;
CREATE INDEX IF NOT EXISTS plugin_bindings_target ON plugin_bindings(target_type,source_kind,provider_id,target_id,enabled,orphan);
PRAGMA user_version=5;
"#,
            )?;
            tx.commit()?;
        }
        let hmac_nodes: i64 = db.query_row("SELECT count(*) FROM nodes WHERE stable_id_version=2", [], |row| {
            row.get(0)
        })?;
        Ok(Self {
            db,
            identity_key: None,
            identity_generation: None,
            identity_recovery: hmac_nodes != 0,
        })
    }
    pub fn upsert_manual(&mut self, node: &Node) -> Result<StableId, DataError> {
        self.ensure_writable()?;
        let id = self.identity_for(node)?;
        let tx = self.db.transaction()?;
        let migrated = if let Some(generation) = self.identity_generation {
            migrate_exact_v1_identity(&tx, &Source::Manual, node, &node.stable_id()?, &id, 1, generation)?
        } else {
            false
        };
        if !migrated {
            write_node(&tx, &Source::Manual, &id, node, 1, self.identity_generation)?;
        }
        audit(&tx, "manual.upsert", serde_json::json!({"nodes":1}))?;
        tx.commit()?;
        Ok(id)
    }
    pub fn upsert_manual_batch(
        &mut self,
        nodes: &[Node],
        changed_ids: &[StableId],
    ) -> Result<Vec<StableId>, DataError> {
        self.ensure_writable()?;
        let ids = nodes
            .iter()
            .map(|node| self.identity_for(node))
            .collect::<Result<Vec<_>, _>>()?;
        let tx = self.db.transaction()?;
        for (node, id) in nodes.iter().zip(&ids) {
            let migrated = if let Some(generation) = self.identity_generation {
                migrate_exact_v1_identity(&tx, &Source::Manual, node, &node.stable_id()?, id, 1, generation)?
            } else {
                false
            };
            if !migrated {
                write_node(&tx, &Source::Manual, id, node, 1, self.identity_generation)?;
            }
            if changed_ids.contains(id) {
                tx.execute(
                    "DELETE FROM quality_cache WHERE source_kind='manual' AND provider_id='' AND stable_id=?1",
                    [id],
                )?;
            }
        }
        audit(&tx, "manual.upsert", serde_json::json!({"nodes":ids.len()}))?;
        tx.commit()?;
        Ok(ids)
    }
    pub fn refresh_provider(&mut self, provider_id: &str, nodes: &[Node]) -> Result<ProviderDiff, DataError> {
        self.ensure_writable()?;
        text(provider_id)?;
        let source = Source::Subscription {
            provider_id: provider_id.to_owned(),
        };
        let mut next: BTreeMap<StableId, (&Node, usize)> = BTreeMap::new();
        for node in nodes {
            let id = self.identity_for(node)?;
            next.entry(id)
                .and_modify(|(selected, count)| {
                    *count += 1;
                    if node.display_name < selected.display_name {
                        *selected = node;
                    }
                })
                .or_insert((node, 1));
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let previous: BTreeMap<String, bool> = {
            let mut statement =
                tx.prepare("SELECT stable_id,active FROM nodes WHERE source_kind='subscription' AND provider_id=?1")?;
            statement
                .query_map([provider_id], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<_, _>>()?
        };
        let mut diff = ProviderDiff::default();
        let mut migrated_old_ids = std::collections::BTreeSet::new();
        for (id, (node, count)) in &next {
            diff.duplicates += count - 1;
            if previous.get(id) == Some(&true) {
                diff.matched.push(id.clone());
            } else {
                diff.added.push(id.clone());
            }
            if previous.get(id) == Some(&false) {
                diff.restored.push(id.clone());
            }
            let migrated = if let Some(generation) = self.identity_generation {
                let old_id = node.stable_id()?;
                let migrated = migrate_exact_v1_identity(&tx, &source, node, &old_id, id, *count, generation)?;
                if migrated {
                    migrated_old_ids.insert(old_id);
                }
                migrated
            } else {
                false
            };
            if !migrated {
                write_node(&tx, &source, id, node, *count, self.identity_generation)?;
            }
        }
        if self.identity_generation.is_some() {
            reconcile_rotated_credentials(&tx, provider_id, &next, self.identity_generation.unwrap_or_default())?;
        }
        for (id, active) in &previous {
            if *active && !next.contains_key(id) && !migrated_old_ids.contains(id) {
                diff.removed.push(id.clone());
                tx.execute(
                    "UPDATE nodes SET active=0 WHERE source_kind='subscription' AND provider_id=?1 AND stable_id=?2",
                    params![provider_id, id],
                )?;
                tx.execute("UPDATE bindings SET orphan=1,disappeared_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE source_kind='subscription' AND provider_id=?1 AND target_id=?2 AND orphan=0",params![provider_id,id])?;
                tx.execute("UPDATE plugin_bindings SET orphan=1,disappeared_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE target_type='node' AND source_kind='subscription' AND provider_id=?1 AND target_id=?2 AND orphan=0",params![provider_id,id])?;
            }
        }
        audit(
            &tx,
            "provider.refresh",
            serde_json::json!({"matched":diff.matched.len(),"added":diff.added.len(),"removed":diff.removed.len(),"restored":diff.restored.len(),"duplicates":diff.duplicates}),
        )?;
        tx.commit()?;
        Ok(diff)
    }
    pub fn upsert_binding(&mut self, binding: &Binding) -> Result<(), DataError> {
        self.ensure_writable()?;
        text(&binding.id)?;
        text(&binding.plugin_id)?;
        let (kind, provider) = binding.source.scope()?;
        let tx = self.db.transaction()?;
        let active_version: Option<i64> = tx
            .query_row(
                "SELECT stable_id_version FROM nodes WHERE source_kind=?1 AND provider_id=?2 AND stable_id=?3 AND active=1",
                params![kind, provider, binding.target_id],
                |r| r.get(0),
            )
            .optional()?;
        if active_version.is_none() {
            return Err(DataError::NotFound);
        }
        if self.identity_key.is_some() && active_version != Some(2) {
            return Err(DataError::IdentityRecovery);
        }
        tx.execute("INSERT INTO bindings VALUES(?1,?2,?3,?4,?5,?6,?7,?8,0,NULL) ON CONFLICT(source_kind,provider_id,id) DO UPDATE SET plugin_id=excluded.plugin_id,target_id=excluded.target_id,priority=excluded.priority,enabled=excluded.enabled,config=excluded.config,orphan=0,disappeared_at=NULL",params![kind,provider,binding.id,binding.plugin_id,binding.target_id,binding.priority,binding.enabled,binding.config.to_string()])?;
        audit(&tx, "binding.upsert", serde_json::json!({"bindings":1}))?;
        tx.commit()?;
        Ok(())
    }
    pub fn upsert_plugin_binding(&mut self, binding: &PluginBinding) -> Result<(), DataError> {
        self.upsert_plugin_bindings(std::slice::from_ref(binding))
    }
    pub fn upsert_plugin_bindings(&mut self, bindings: &[PluginBinding]) -> Result<(), DataError> {
        if bindings.is_empty() || bindings.len() > 32 {
            return Err(DataError::InvalidInput);
        }
        let tx = self.db.transaction()?;
        let mut keys = BTreeSet::new();
        for binding in bindings {
            text(&binding.id)?;
            text(&binding.plugin_id)?;
            if !binding.config.is_object() {
                return Err(DataError::InvalidInput);
            }
            let (target_type, source_kind, provider_id, target_id) = match &binding.target {
                PluginBindingTarget::Node { source, stable_id } => {
                    text(stable_id)?;
                    let (source_kind, provider_id) = source.scope()?;
                    let active: Option<bool> = tx
                        .query_row(
                            "SELECT active FROM nodes WHERE source_kind=?1 AND provider_id=?2 AND stable_id=?3",
                            params![source_kind, provider_id, stable_id],
                            |row| row.get(0),
                        )
                        .optional()?;
                    if active != Some(true) {
                        return Err(DataError::NotFound);
                    }
                    ("node", source_kind, provider_id.to_owned(), stable_id.as_str())
                }
                PluginBindingTarget::Chain { chain_id } => {
                    text(chain_id)?;
                    let raw: Option<String> = tx
                        .query_row("SELECT definition FROM chains WHERE id=?1", [chain_id], |row| {
                            row.get(0)
                        })
                        .optional()?;
                    let raw = raw.ok_or(DataError::NotFound)?;
                    let chain: ProxyChain = serde_json::from_str(&raw).map_err(|_| DataError::Database)?;
                    chain.validate()?;
                    (
                        "chain",
                        "profile",
                        chain.profile_id.unwrap_or_default(),
                        chain_id.as_str(),
                    )
                }
            };
            if !keys.insert((
                target_type,
                source_kind,
                provider_id.clone(),
                target_id.to_owned(),
                binding.id.clone(),
            )) {
                return Err(DataError::InvalidInput);
            }
            tx.execute(
                "INSERT INTO plugin_bindings(target_type,source_kind,provider_id,target_id,binding_id,plugin_id,priority,enabled,config,orphan,disappeared_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,0,NULL) ON CONFLICT(target_type,source_kind,provider_id,target_id,binding_id) DO UPDATE SET plugin_id=excluded.plugin_id,priority=excluded.priority,enabled=excluded.enabled,config=excluded.config,orphan=0,disappeared_at=NULL",
                params![target_type, source_kind, provider_id, target_id, binding.id, binding.plugin_id, binding.priority, binding.enabled, binding.config.to_string()],
            )?;
        }
        audit(
            &tx,
            "plugin_binding.upsert",
            serde_json::json!({"bindings":bindings.len()}),
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn delete_plugin_binding(&mut self, target: &PluginBindingTarget, binding_id: &str) -> Result<(), DataError> {
        self.delete_plugin_bindings(&[(target.clone(), binding_id.to_owned())])?;
        Ok(())
    }
    pub fn delete_plugin_bindings(&mut self, bindings: &[(PluginBindingTarget, String)]) -> Result<usize, DataError> {
        let tx = self.db.transaction()?;
        let mut changed = 0;
        for (target, binding_id) in bindings {
            text(binding_id)?;
            changed += match target {
                PluginBindingTarget::Node { source, stable_id } => {
                    let (source_kind, provider_id) = source.scope()?;
                    tx.execute(
                        "DELETE FROM plugin_bindings WHERE target_type='node' AND source_kind=?1 AND provider_id=?2 AND target_id=?3 AND binding_id=?4",
                        params![source_kind, provider_id, stable_id, binding_id],
                    )?
                }
                PluginBindingTarget::Chain { chain_id } => tx.execute(
                    "DELETE FROM plugin_bindings WHERE target_type='chain' AND target_id=?1 AND binding_id=?2",
                    params![chain_id, binding_id],
                )?,
            };
        }
        if changed > 0 {
            audit(&tx, "plugin_binding.delete", serde_json::json!({"bindings":changed}))?;
        }
        tx.commit()?;
        Ok(changed)
    }
    pub fn plugin_bindings(&self, target: Option<&PluginBindingTarget>) -> Result<Vec<PluginBinding>, DataError> {
        let scope = target
            .map(|target| -> Result<(&str, String, Option<String>, String), DataError> {
                match target {
                    PluginBindingTarget::Node { source, stable_id } => {
                        let (source_kind, provider_id) = source.scope()?;
                        Ok((
                            "node",
                            source_kind.to_owned(),
                            Some(provider_id.to_owned()),
                            stable_id.clone(),
                        ))
                    }
                    PluginBindingTarget::Chain { chain_id } => {
                        text(chain_id)?;
                        let raw: Option<String> = self
                            .db
                            .query_row("SELECT definition FROM chains WHERE id=?1", [chain_id], |row| {
                                row.get(0)
                            })
                            .optional()?;
                        let provider_id = raw
                            .map(|raw| -> Result<String, DataError> {
                                let chain: ProxyChain = serde_json::from_str(&raw).map_err(|_| DataError::Database)?;
                                chain.validate()?;
                                Ok(chain.profile_id.unwrap_or_default())
                            })
                            .transpose()?;
                        Ok(("chain", "profile".into(), provider_id, chain_id.clone()))
                    }
                }
            })
            .transpose()?;
        let (target_type, source_kind, provider_id, target_id) =
            scope.as_ref().map_or(("", "", None, ""), |(a, b, c, d)| {
                (*a, b.as_str(), c.as_deref(), d.as_str())
            });
        let mut statement = self.db.prepare("SELECT target_type,source_kind,provider_id,target_id,binding_id,plugin_id,priority,enabled,config,orphan,disappeared_at FROM plugin_bindings WHERE (?1='' OR (target_type=?1 AND source_kind=?2 AND (?3 IS NULL OR provider_id=?3) AND target_id=?4)) ORDER BY source_kind,provider_id,target_type,target_id,binding_id")?;
        let rows = statement.query_map(params![target_type, source_kind, provider_id, target_id], |row| {
            let target_type: String = row.get(0)?;
            let source_kind: String = row.get(1)?;
            let provider_id: String = row.get(2)?;
            let target_id: String = row.get(3)?;
            let config: String = row.get(8)?;
            let target = if target_type == "node" {
                PluginBindingTarget::Node {
                    source: if source_kind == "manual" {
                        Source::Manual
                    } else {
                        Source::Subscription { provider_id }
                    },
                    stable_id: target_id,
                }
            } else {
                PluginBindingTarget::Chain { chain_id: target_id }
            };
            Ok(PluginBinding {
                target,
                id: row.get(4)?,
                plugin_id: row.get(5)?,
                priority: row.get(6)?,
                enabled: row.get(7)?,
                config: serde_json::from_str(&config).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(error))
                })?,
                orphan: row.get(9)?,
                disappeared_at: row.get(10)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
    pub fn bindings(&self, source: Option<&Source>) -> Result<Vec<Binding>, DataError> {
        let scope = source.map(Source::scope).transpose()?;
        let (kind, provider) = scope.unwrap_or(("", ""));
        let mut statement = self.db.prepare("SELECT source_kind,provider_id,id,plugin_id,target_id,priority,enabled,config,orphan,disappeared_at FROM bindings WHERE (?1='' OR (source_kind=?1 AND provider_id=?2)) ORDER BY source_kind,provider_id,id")?;
        let rows = statement.query_map(params![kind, provider], |r| {
            let kind: String = r.get(0)?;
            let raw: String = r.get(7)?;
            let config = serde_json::from_str(&raw)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(e)))?;
            Ok(Binding {
                source: if kind == "manual" {
                    Source::Manual
                } else {
                    Source::Subscription { provider_id: r.get(1)? }
                },
                id: r.get(2)?,
                plugin_id: r.get(3)?,
                target_id: r.get(4)?,
                priority: r.get(5)?,
                enabled: r.get(6)?,
                config,
                orphan: r.get(8)?,
                disappeared_at: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn pending_identity_rebinds(&self, source: Option<&Source>) -> Result<Vec<PendingIdentityRebind>, DataError> {
        let (kind, provider) = source.map(Source::scope).transpose()?.unwrap_or(("", ""));
        let mut statement = self.db.prepare("SELECT source_kind,provider_id,old_stable_id,candidate_ids FROM pending_identity_rebinds WHERE (?1='' OR (source_kind=?1 AND provider_id=?2)) ORDER BY provider_id,old_stable_id")?;
        let rows = statement.query_map(params![kind, provider], |row| {
            let source_kind: String = row.get(0)?;
            let raw: String = row.get(3)?;
            let candidate_ids = serde_json::from_str(&raw).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(error))
            })?;
            Ok(PendingIdentityRebind {
                source: if source_kind == "subscription" {
                    Source::Subscription {
                        provider_id: row.get(1)?,
                    }
                } else {
                    Source::Manual
                },
                old_stable_id: row.get(2)?,
                candidate_ids,
            })
        })?;
        rows.collect::<Result<_, _>>().map_err(Into::into)
    }

    pub fn resolve_identity_rebind(&mut self, source: &Source, old_id: &str, new_id: &str) -> Result<(), DataError> {
        self.ensure_writable()?;
        let (kind, provider) = source.scope()?;
        if kind != "subscription" {
            return Err(DataError::InvalidInput);
        }
        let tx = self.db.transaction()?;
        let raw: Option<String> = tx.query_row("SELECT candidate_ids FROM pending_identity_rebinds WHERE source_kind=?1 AND provider_id=?2 AND old_stable_id=?3",params![kind,provider,old_id],|row|row.get(0)).optional()?;
        let raw = raw.ok_or(DataError::NotFound)?;
        let candidates: Vec<String> = serde_json::from_str(&raw).map_err(|_| DataError::Database)?;
        if !candidates.iter().any(|candidate| candidate == new_id) {
            return Err(DataError::InvalidInput);
        }
        let active: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM nodes WHERE source_kind=?1 AND provider_id=?2 AND stable_id=?3 AND active=1)",
            params![kind, provider, new_id],
            |row| row.get(0),
        )?;
        if !active {
            return Err(DataError::NotFound);
        }
        let moved = tx.execute("UPDATE bindings SET target_id=?4,orphan=0,disappeared_at=NULL WHERE source_kind=?1 AND provider_id=?2 AND target_id=?3",params![kind,provider,old_id,new_id])?;
        tx.execute(
            "DELETE FROM pending_identity_rebinds WHERE source_kind=?1 AND provider_id=?2 AND old_stable_id=?3",
            params![kind, provider, old_id],
        )?;
        audit(&tx, "identity.rebind", serde_json::json!({"bindings":moved}))?;
        tx.commit()?;
        Ok(())
    }
    pub fn effective_bindings(&self, source: &Source, id: &str) -> Result<Vec<Binding>, DataError> {
        Ok(self
            .bindings(Some(source))?
            .into_iter()
            .filter(|b| b.target_id == id && b.enabled && !b.orphan)
            .collect())
    }
    pub fn effective_plugin_bindings(&self, target: &PluginBindingTarget) -> Result<Vec<PluginBinding>, DataError> {
        Ok(self
            .plugin_bindings(Some(target))?
            .into_iter()
            .filter(|binding| binding.enabled && !binding.orphan)
            .collect())
    }
    pub fn record_plugin_audit(&mut self, event: &str, details: &Value) -> Result<(), DataError> {
        if !matches!(
            event,
            "plugin.install" | "plugin.permission.use" | "plugin.execute" | "plugin.failure"
        ) || !details.is_object()
        {
            return Err(DataError::InvalidInput);
        }
        let tx = self.db.transaction()?;
        audit(&tx, event, details.clone())?;
        tx.commit()?;
        Ok(())
    }
    pub fn record_plugin_audit_batch(&mut self, events: &[(String, Value)]) -> Result<(), DataError> {
        if events.is_empty() || events.len() > 32 {
            return Err(DataError::InvalidInput);
        }
        if events.iter().any(|(event, details)| {
            !matches!(
                event.as_str(),
                "plugin.install" | "plugin.permission.use" | "plugin.execute" | "plugin.failure"
            ) || !details.is_object()
        }) {
            return Err(DataError::InvalidInput);
        }
        let tx = self.db.transaction()?;
        for (event, details) in events {
            audit(&tx, event, details.clone())?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn plugin_audit_events(&self, limit: usize) -> Result<Vec<PluginAuditEvent>, DataError> {
        if !(1..=200).contains(&limit) {
            return Err(DataError::InvalidInput);
        }
        let mut statement = self.db.prepare(
            "SELECT id,event,created_at,counts FROM audit_log WHERE event LIKE 'plugin.%' ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit as i64], |row| {
            let details: String = row.get(3)?;
            Ok(PluginAuditEvent {
                id: row.get(0)?,
                event: row.get(1)?,
                created_at: row.get(2)?,
                details: serde_json::from_str(&details).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(error))
                })?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
    pub fn purge_orphans(&mut self, source: Option<&Source>) -> Result<usize, DataError> {
        self.ensure_writable()?;
        let (kind, provider) = source.map(Source::scope).transpose()?.unwrap_or(("", ""));
        let tx = self.db.transaction()?;
        let count = tx.execute(
            "DELETE FROM bindings WHERE orphan=1 AND (?1='' OR (source_kind=?1 AND provider_id=?2))",
            params![kind, provider],
        )?;
        let plugin_count = tx.execute(
            "DELETE FROM plugin_bindings WHERE orphan=1 AND target_type='node' AND (?1='' OR (source_kind=?1 AND provider_id=?2))",
            params![kind, provider],
        )?;
        let chain_count = if kind.is_empty() {
            tx.execute("DELETE FROM plugin_bindings WHERE orphan=1 AND target_type='chain'", [])?
        } else {
            0
        };
        let total = count + plugin_count + chain_count;
        audit(&tx, "orphan.purge", serde_json::json!({"bindings":total}))?;
        tx.commit()?;
        Ok(total)
    }
    pub fn orphan_retention_days(&self) -> Result<u16, DataError> {
        let value: String = self.db.query_row(
            "SELECT setting_value FROM app_settings WHERE setting_key='orphan_retention_days'",
            [],
            |row| row.get(0),
        )?;
        let days = value.parse::<u16>().map_err(|_| DataError::Database)?;
        if !(Self::MIN_ORPHAN_RETENTION_DAYS..=Self::MAX_ORPHAN_RETENTION_DAYS).contains(&days) {
            return Err(DataError::Database);
        }
        Ok(days)
    }
    pub fn set_orphan_retention_days(&mut self, days: u16) -> Result<(), DataError> {
        self.ensure_writable()?;
        if !(Self::MIN_ORPHAN_RETENTION_DAYS..=Self::MAX_ORPHAN_RETENTION_DAYS).contains(&days) {
            return Err(DataError::InvalidInput);
        }
        let tx = self.db.transaction()?;
        tx.execute(
            "UPDATE app_settings SET setting_value=?1 WHERE setting_key='orphan_retention_days'",
            [days.to_string()],
        )?;
        audit(&tx, "orphan.retention.update", serde_json::json!({"days":days}))?;
        tx.commit()?;
        Ok(())
    }
    pub fn orphan_cleanup_preview(&self) -> Result<OrphanCleanupPreview, DataError> {
        let retention_days = self.orphan_retention_days()?;
        let total_orphans: i64 = self
            .db
            .query_row("SELECT count(*) FROM bindings WHERE orphan=1", [], |row| row.get(0))?;
        let eligible_orphans: i64 = self.db.query_row(
            "SELECT count(*) FROM bindings WHERE orphan=1 AND julianday(disappeared_at)<=julianday('now',printf('-%d days',?1))",
            [retention_days],
            |row| row.get(0),
        )?;
        Ok(OrphanCleanupPreview {
            retention_days,
            total_orphans: total_orphans.try_into().map_err(|_| DataError::Database)?,
            eligible_orphans: eligible_orphans.try_into().map_err(|_| DataError::Database)?,
        })
    }
    pub fn purge_expired_orphans(&mut self, source: Option<&Source>) -> Result<usize, DataError> {
        self.ensure_writable()?;
        let retention_days = self.orphan_retention_days()?;
        let (kind, provider) = source.map(Source::scope).transpose()?.unwrap_or(("", ""));
        let tx = self.db.transaction()?;
        let count = tx.execute(
            "DELETE FROM bindings WHERE orphan=1 AND julianday(disappeared_at)<=julianday('now',printf('-%d days',?1)) AND (?2='' OR (source_kind=?2 AND provider_id=?3))",
            params![retention_days, kind, provider],
        )?;
        audit(
            &tx,
            "orphan.purge_expired",
            serde_json::json!({"bindings":count,"retention_days":retention_days}),
        )?;
        tx.commit()?;
        Ok(count)
    }
    pub fn nodes(&self, source: &Source) -> Result<Vec<StableId>, DataError> {
        let (kind, provider) = source.scope()?;
        let mut statement = self.db.prepare(
            "SELECT stable_id FROM nodes WHERE source_kind=?1 AND provider_id=?2 AND active=1 ORDER BY stable_id",
        )?;
        Ok(statement
            .query_map(params![kind, provider], |r| r.get(0))?
            .collect::<Result<_, _>>()?)
    }
    pub fn node_records(&self, include_inactive: bool) -> Result<Vec<NodeRecord>, DataError> {
        let mut statement = self.db.prepare("SELECT n.source_kind,n.provider_id,n.stable_id,n.display_name,n.kind,n.server,n.port,n.active,n.duplicate_count,q.quality FROM nodes n LEFT JOIN quality_cache q ON q.source_kind=n.source_kind AND q.provider_id=n.provider_id AND q.stable_id=n.stable_id WHERE (?1=1 OR n.active=1) ORDER BY n.display_name,n.source_kind,n.provider_id,n.stable_id")?;
        let rows = statement.query_map([include_inactive], |row| {
            let kind: String = row.get(0)?;
            let quality: Option<String> = row.get(9)?;
            let quality = quality
                .map(|raw| {
                    serde_json::from_str(&raw).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(9, rusqlite::types::Type::Text, Box::new(error))
                    })
                })
                .transpose()?;
            Ok(NodeRecord {
                source: if kind == "manual" {
                    Source::Manual
                } else {
                    Source::Subscription {
                        provider_id: row.get(1)?,
                    }
                },
                stable_id: row.get(2)?,
                display_name: row.get(3)?,
                kind: row.get(4)?,
                server: row.get(5)?,
                port: row.get(6)?,
                active: row.get(7)?,
                duplicate_count: row.get(8)?,
                quality,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
    pub fn save_quality(&mut self, source: &Source, stable_id: &str, quality: &NodeQuality) -> Result<(), DataError> {
        self.ensure_writable()?;
        let (kind, provider) = source.scope()?;
        let value = serde_json::to_string(quality).map_err(|_| DataError::InvalidInput)?;
        self.db.execute("INSERT INTO quality_cache(source_kind,provider_id,stable_id,quality) VALUES(?1,?2,?3,?4) ON CONFLICT(source_kind,provider_id,stable_id) DO UPDATE SET quality=excluded.quality",params![kind,provider,stable_id,value])?;
        Ok(())
    }
    pub fn save_chain(&mut self, id: &str, definition: &Value) -> Result<(), DataError> {
        self.ensure_writable()?;
        text(id)?;
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT INTO chains VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition",
            params![id, definition.to_string()],
        )?;
        audit(&tx, "chain.upsert", serde_json::json!({"chains":1}))?;
        tx.commit()?;
        Ok(())
    }
    pub fn chain(&self, id: &str) -> Result<Option<Value>, DataError> {
        text(id)?;
        let raw: Option<String> = self
            .db
            .query_row("SELECT definition FROM chains WHERE id=?1", [id], |r| r.get(0))
            .optional()?;
        raw.map(|s| serde_json::from_str(&s).map_err(|_| DataError::Database))
            .transpose()
    }
    /// Save a validated M2 definition without storing proxy credentials.
    pub fn save_proxy_chain(&mut self, chain: &ProxyChain) -> Result<(), DataError> {
        chain.validate()?;
        let definition = serde_json::to_value(chain).map_err(|_| DataError::InvalidInput)?;
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT INTO chains VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition",
            params![chain.id, definition.to_string()],
        )?;
        tx.execute(
            "UPDATE plugin_bindings SET provider_id=?1,orphan=0,disappeared_at=NULL WHERE target_type='chain' AND target_id=?2",
            params![chain.profile_id.as_deref().unwrap_or_default(), chain.id],
        )?;
        audit(&tx, "chain.upsert", serde_json::json!({"chains":1}))?;
        tx.commit()?;
        Ok(())
    }
    /// Return versioned M2 definitions; legacy opaque M0 records remain available through `chain`.
    pub fn proxy_chains(&self) -> Result<Vec<ProxyChain>, DataError> {
        let mut statement = self.db.prepare(
            "SELECT definition FROM chains WHERE json_extract(definition,'$.schema_version') IS NOT NULL ORDER BY id",
        )?;
        let raw = statement.query_map([], |row| row.get::<_, String>(0))?;
        raw.map(|raw| {
            let chain: ProxyChain = serde_json::from_str(&raw?).map_err(|_| DataError::Database)?;
            chain.validate()?;
            Ok(chain)
        })
        .collect()
    }
    pub fn delete_proxy_chain(&mut self, id: &str) -> Result<(), DataError> {
        text(id)?;
        let tx = self.db.transaction()?;
        let deleted = tx.execute("DELETE FROM chains WHERE id=?1", [id])?;
        if deleted == 0 {
            return Err(DataError::NotFound);
        }
        tx.execute("UPDATE plugin_bindings SET orphan=1,disappeared_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE target_type='chain' AND target_id=?1 AND orphan=0", [id])?;
        audit(&tx, "chain.delete", serde_json::json!({"chains":deleted}))?;
        tx.commit()?;
        Ok(())
    }
}
