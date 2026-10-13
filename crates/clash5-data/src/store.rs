use crate::{DataError, Node, StableId, identity::text};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path, time::Duration};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    Manual,
    Subscription { provider_id: String },
}
impl Source {
    fn scope(&self) -> Result<(&str, &str), DataError> {
        match self {
            Self::Manual => Ok(("manual", "")),
            Self::Subscription { provider_id } => {
                text(provider_id)?;
                Ok(("subscription", provider_id))
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub id: String,
    pub plugin_id: String,
    pub target_id: StableId,
    pub source: Source,
    pub priority: i64,
    pub enabled: bool,
    pub config: Value,
    #[serde(default)]
    pub orphan: bool,
    #[serde(default)]
    pub disappeared_at: Option<String>,
}
#[derive(Default, Debug, PartialEq, Eq, Serialize)]
pub struct ProviderDiff {
    pub matched: Vec<StableId>,
    pub added: Vec<StableId>,
    pub removed: Vec<StableId>,
    pub restored: Vec<StableId>,
    pub duplicates: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NodeRecord {
    pub stable_id: StableId,
    pub source: Source,
    pub display_name: String,
    pub kind: String,
    pub server: String,
    pub port: u16,
    pub active: bool,
    pub duplicate_count: usize,
    pub quality: Option<NodeQuality>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityStatus {
    Healthy,
    Slow,
    Failed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeQuality {
    pub status: QualityStatus,
    pub short_delay_ms: Option<u32>,
    pub handshake_delay_ms: Option<u32>,
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
) -> Result<(), DataError> {
    let (kind, provider) = source.scope()?;
    tx.execute("INSERT INTO nodes(source_kind,provider_id,stable_id,display_name,active,kind,server,port,duplicate_count) VALUES(?1,?2,?3,?4,1,?5,?6,?7,?8) ON CONFLICT(source_kind,provider_id,stable_id) DO UPDATE SET display_name=excluded.display_name,active=1,kind=excluded.kind,server=excluded.server,port=excluded.port,duplicate_count=excluded.duplicate_count",params![kind,provider,id,node.display_name,node.kind,node.server,node.port,duplicate_count])?;
    tx.execute("UPDATE bindings SET orphan=0,disappeared_at=NULL WHERE source_kind=?1 AND provider_id=?2 AND target_id=?3 AND orphan=1",params![kind,provider,id])?;
    Ok(())
}

impl BindingStore {
    pub const NAME_MATCHING_ENABLED: bool = false;

    pub fn open(path: impl AsRef<Path>) -> Result<Self, DataError> {
        Self::initialize(Connection::open(path)?)
    }
    pub fn memory() -> Result<Self, DataError> {
        Self::initialize(Connection::open_in_memory()?)
    }
    fn initialize(mut db: Connection) -> Result<Self, DataError> {
        db.pragma_update(None, "foreign_keys", true)?;
        db.busy_timeout(Duration::from_secs(5))?;
        let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if !(0..=2).contains(&version) {
            return Err(DataError::SchemaVersion);
        }
        if version == 0 {
            let tx = db.transaction()?;
            tx.execute_batch(
                r#"
CREATE TABLE nodes (
 source_kind TEXT NOT NULL CHECK(source_kind IN ('manual','subscription')),
 provider_id TEXT NOT NULL, stable_id TEXT NOT NULL, display_name TEXT NOT NULL,
 kind TEXT NOT NULL DEFAULT '', server TEXT NOT NULL DEFAULT '',
 port INTEGER NOT NULL DEFAULT 0, duplicate_count INTEGER NOT NULL DEFAULT 1 CHECK(duplicate_count>0),
 active INTEGER NOT NULL CHECK(active IN (0,1)),
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
CREATE TABLE chains(id TEXT PRIMARY KEY, definition TEXT NOT NULL CHECK(json_valid(definition))) STRICT;
CREATE TABLE quality_cache(
 source_kind TEXT NOT NULL, provider_id TEXT NOT NULL, stable_id TEXT NOT NULL,
 quality TEXT NOT NULL CHECK(json_valid(quality)), PRIMARY KEY(source_kind,provider_id,stable_id),
 FOREIGN KEY(source_kind,provider_id,stable_id) REFERENCES nodes(source_kind,provider_id,stable_id)
) STRICT;
CREATE TABLE audit_log(id INTEGER PRIMARY KEY,event TEXT NOT NULL,
 created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 counts TEXT NOT NULL CHECK(json_valid(counts))) STRICT;
PRAGMA user_version=2;
"#,
            )?;
            tx.commit()?;
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
        }
        Ok(Self { db })
    }
    pub fn upsert_manual(&mut self, node: &Node) -> Result<StableId, DataError> {
        let id = node.stable_id()?;
        let tx = self.db.transaction()?;
        write_node(&tx, &Source::Manual, &id, node, 1)?;
        audit(&tx, "manual.upsert", serde_json::json!({"nodes":1}))?;
        tx.commit()?;
        Ok(id)
    }
    pub fn upsert_manual_batch(&mut self, nodes: &[Node]) -> Result<Vec<StableId>, DataError> {
        let ids = nodes.iter().map(Node::stable_id).collect::<Result<Vec<_>, _>>()?;
        let tx = self.db.transaction()?;
        for (node, id) in nodes.iter().zip(&ids) {
            write_node(&tx, &Source::Manual, id, node, 1)?;
            tx.execute(
                "DELETE FROM quality_cache WHERE source_kind='manual' AND provider_id='' AND stable_id=?1",
                [id],
            )?;
        }
        audit(&tx, "manual.upsert", serde_json::json!({"nodes":ids.len()}))?;
        tx.commit()?;
        Ok(ids)
    }
    pub fn refresh_provider(&mut self, provider_id: &str, nodes: &[Node]) -> Result<ProviderDiff, DataError> {
        text(provider_id)?;
        let source = Source::Subscription {
            provider_id: provider_id.to_owned(),
        };
        let mut next: BTreeMap<StableId, (&Node, usize)> = BTreeMap::new();
        for node in nodes {
            let id = node.stable_id()?;
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
            write_node(&tx, &source, id, node, *count)?;
        }
        for (id, active) in &previous {
            if *active && !next.contains_key(id) {
                diff.removed.push(id.clone());
                tx.execute(
                    "UPDATE nodes SET active=0 WHERE source_kind='subscription' AND provider_id=?1 AND stable_id=?2",
                    params![provider_id, id],
                )?;
                tx.execute("UPDATE bindings SET orphan=1,disappeared_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE source_kind='subscription' AND provider_id=?1 AND target_id=?2 AND orphan=0",params![provider_id,id])?;
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
        text(&binding.id)?;
        text(&binding.plugin_id)?;
        let (kind, provider) = binding.source.scope()?;
        let tx = self.db.transaction()?;
        let active: Option<bool> = tx
            .query_row(
                "SELECT active FROM nodes WHERE source_kind=?1 AND provider_id=?2 AND stable_id=?3",
                params![kind, provider, binding.target_id],
                |r| r.get(0),
            )
            .optional()?;
        if active != Some(true) {
            return Err(DataError::NotFound);
        }
        tx.execute("INSERT INTO bindings VALUES(?1,?2,?3,?4,?5,?6,?7,?8,0,NULL) ON CONFLICT(source_kind,provider_id,id) DO UPDATE SET plugin_id=excluded.plugin_id,target_id=excluded.target_id,priority=excluded.priority,enabled=excluded.enabled,config=excluded.config,orphan=0,disappeared_at=NULL",params![kind,provider,binding.id,binding.plugin_id,binding.target_id,binding.priority,binding.enabled,binding.config.to_string()])?;
        audit(&tx, "binding.upsert", serde_json::json!({"bindings":1}))?;
        tx.commit()?;
        Ok(())
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
    pub fn effective_bindings(&self, source: &Source, id: &str) -> Result<Vec<Binding>, DataError> {
        Ok(self
            .bindings(Some(source))?
            .into_iter()
            .filter(|b| b.target_id == id && b.enabled && !b.orphan)
            .collect())
    }
    pub fn purge_orphans(&mut self, source: Option<&Source>) -> Result<usize, DataError> {
        let (kind, provider) = source.map(Source::scope).transpose()?.unwrap_or(("", ""));
        let tx = self.db.transaction()?;
        let count = tx.execute(
            "DELETE FROM bindings WHERE orphan=1 AND (?1='' OR (source_kind=?1 AND provider_id=?2))",
            params![kind, provider],
        )?;
        audit(&tx, "orphan.purge", serde_json::json!({"bindings":count}))?;
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
        let (kind, provider) = source.scope()?;
        let value = serde_json::to_string(quality).map_err(|_| DataError::InvalidInput)?;
        self.db.execute("INSERT INTO quality_cache(source_kind,provider_id,stable_id,quality) VALUES(?1,?2,?3,?4) ON CONFLICT(source_kind,provider_id,stable_id) DO UPDATE SET quality=excluded.quality",params![kind,provider,stable_id,value])?;
        Ok(())
    }
    pub fn save_chain(&mut self, id: &str, definition: &Value) -> Result<(), DataError> {
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
}
