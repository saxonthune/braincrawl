//! `PayloadsRepo` + `MetadataStore` backed by `RefCell<HashMap>` (for tests).
//!
//! Implements real union-find/merge/dedup semantics against in-memory tables that mirror
//! the SQL schema in migrations/0001_init.sql + 0002_graph.sql.

#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use braincrawl_core::{
    traits::{MetadataStore, PayloadsRepo},
    types::{
        Alias, CanonicalId, DomainError, EdgeDir, EdgeView, GraphStats, NodeKind, PayloadDescriptor,
        PayloadKind, Tally,
    },
};

// ---------------------------------------------------------------------------
// Internal row types (mirror SQL tables)
// ---------------------------------------------------------------------------

struct NodeRow {
    kind: NodeKind,
    created_at: String,
    /// Tombstone pointer; None means live.
    merged_into: Option<CanonicalId>,
}

struct NodeAssertionRow {
    attrs: serde_json::Value,
    fetched_at: String,
}

/// Per-source assertion on an edge.
struct EdgeAssertionRow {
    attrs: Option<serde_json::Value>,
    fetched_at: String,
}

// Edge PK: (src_id, dst_id, relation)
// Assertions keyed by source within an edge.
type EdgeAssertions = BTreeMap<String, EdgeAssertionRow>;

// ---------------------------------------------------------------------------
// MemStoreInner
// ---------------------------------------------------------------------------

struct MemStoreInner {
    /// node table: canonical_id.0 → row
    nodes: HashMap<String, NodeRow>,
    /// alias table: (namespace, value) → canonical_id.0
    aliases: HashMap<(String, String), String>,
    /// node_assertion: (canonical_id.0, source) → row
    node_assertions: HashMap<(String, String), NodeAssertionRow>,
    /// edge + edge_assertion: (src.0, dst.0, relation) → {source → assertion}
    edges: HashMap<(String, String, String), EdgeAssertions>,
    /// payloads: (canonical_id.0, kind_str) → versions (ordered by version)
    payloads: HashMap<(String, String), Vec<PayloadDescriptor>>,
}

impl MemStoreInner {
    fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            aliases: HashMap::new(),
            node_assertions: HashMap::new(),
            edges: HashMap::new(),
            payloads: HashMap::new(),
        }
    }

    fn kind_str(kind: &PayloadKind) -> &'static str {
        match kind {
            PayloadKind::Abstract => "abstract",
            PayloadKind::Fulltext => "fulltext",
        }
    }

    /// Follow the `merged_into` chain to the live representative with path compression.
    /// Returns `Err(NotFound)` if `id` is not in the node table.
    fn resolve_live_sync(&mut self, id: &str) -> Result<String, DomainError> {
        let mut chain: Vec<String> = Vec::new();
        let mut cur = id.to_string();
        loop {
            match self.nodes.get(&cur) {
                None => return Err(DomainError::NotFound),
                Some(row) => match &row.merged_into {
                    None => break,
                    Some(next) => {
                        chain.push(cur.clone());
                        cur = next.0.clone();
                    }
                },
            }
        }
        // Path compress: point all intermediate nodes directly to the root.
        for c in &chain {
            if let Some(row) = self.nodes.get_mut(c) {
                row.merged_into = Some(CanonicalId(cur.clone()));
            }
        }
        Ok(cur)
    }
}

// ---------------------------------------------------------------------------
// MemStore
// ---------------------------------------------------------------------------

pub struct MemStore {
    inner: RefCell<MemStoreInner>,
}

impl MemStore {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(MemStoreInner::new()),
        }
    }
}

impl Default for MemStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// PayloadsRepo
// ---------------------------------------------------------------------------

#[async_trait(?Send)]
impl PayloadsRepo for MemStore {
    async fn current_payload(
        &self,
        id: &CanonicalId,
        kind: PayloadKind,
    ) -> Result<Option<PayloadDescriptor>, DomainError> {
        let inner = self.inner.borrow();
        let key = (id.0.clone(), MemStoreInner::kind_str(&kind).to_string());
        Ok(inner
            .payloads
            .get(&key)
            .and_then(|rows| rows.iter().find(|d| d.is_current))
            .cloned())
    }

    async fn next_version(
        &self,
        id: &CanonicalId,
        kind: PayloadKind,
    ) -> Result<u32, DomainError> {
        let inner = self.inner.borrow();
        let key = (id.0.clone(), MemStoreInner::kind_str(&kind).to_string());
        let next = inner
            .payloads
            .get(&key)
            .map(|rows| rows.iter().map(|d| d.version).max().unwrap_or(0) + 1)
            .unwrap_or(1);
        Ok(next)
    }

    async fn record(&self, descriptor: &PayloadDescriptor) -> Result<(), DomainError> {
        let mut inner = self.inner.borrow_mut();
        let kind_str = MemStoreInner::kind_str(&descriptor.kind).to_string();
        let key = (descriptor.canonical_id.0.clone(), kind_str);
        let rows = inner.payloads.entry(key).or_default();
        if descriptor.is_current {
            for row in rows.iter_mut() {
                row.is_current = false;
            }
        }
        rows.push(descriptor.clone());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// MetadataStore
// ---------------------------------------------------------------------------

#[async_trait(?Send)]
impl MetadataStore for MemStore {
    async fn get_alias(&self, alias: &Alias) -> Result<Option<CanonicalId>, DomainError> {
        let inner = self.inner.borrow();
        Ok(inner
            .aliases
            .get(&(alias.namespace.clone(), alias.value.clone()))
            .map(|id| CanonicalId(id.clone())))
    }

    /// INSERT … ON CONFLICT DO NOTHING then SELECT; returns the winner's canonical id.
    async fn get_or_create_alias(
        &self,
        alias: &Alias,
        candidate: &CanonicalId,
    ) -> Result<CanonicalId, DomainError> {
        let mut inner = self.inner.borrow_mut();
        let key = (alias.namespace.clone(), alias.value.clone());
        match inner.aliases.get(&key).cloned() {
            Some(existing) => Ok(CanonicalId(existing)),
            None => {
                inner.aliases.insert(key, candidate.0.clone());
                Ok(candidate.clone())
            }
        }
    }

    /// Idempotent: does nothing if the node already exists.
    async fn mint_node(
        &self,
        id: &CanonicalId,
        kind: NodeKind,
        created_at: &str,
    ) -> Result<(), DomainError> {
        let mut inner = self.inner.borrow_mut();
        inner.nodes.entry(id.0.clone()).or_insert_with(|| NodeRow {
            kind,
            created_at: created_at.to_string(),
            merged_into: None,
        });
        Ok(())
    }

    /// Always overwrites for the given (id, source) pair.
    async fn upsert_node_assertion(
        &self,
        id: &CanonicalId,
        source: &str,
        attrs: &serde_json::Value,
        fetched_at: &str,
    ) -> Result<(), DomainError> {
        let mut inner = self.inner.borrow_mut();
        inner.node_assertions.insert(
            (id.0.clone(), source.to_string()),
            NodeAssertionRow {
                attrs: attrs.clone(),
                fetched_at: fetched_at.to_string(),
            },
        );
        Ok(())
    }

    /// Follow `merged_into` chain to the live representative with path compression.
    async fn resolve_live(&self, id: &CanonicalId) -> Result<CanonicalId, DomainError> {
        let mut inner = self.inner.borrow_mut();
        inner.resolve_live_sync(&id.0).map(CanonicalId)
    }

    /// Repoints aliases/assertions/edges/payloads from loser to survivor, folds PK
    /// collisions, tombstones the loser.
    async fn merge(
        &self,
        survivor: &CanonicalId,
        loser: &CanonicalId,
    ) -> Result<(), DomainError> {
        let mut inner = self.inner.borrow_mut();

        // 1. Tombstone loser.
        if let Some(row) = inner.nodes.get_mut(&loser.0) {
            row.merged_into = Some(survivor.clone());
        }

        // 2. Repoint aliases.
        for val in inner.aliases.values_mut() {
            if *val == loser.0 {
                *val = survivor.0.clone();
            }
        }

        // 3. Repoint node_assertions (fold: newer fetched_at wins per source).
        let loser_na_keys: Vec<(String, String)> = inner
            .node_assertions
            .keys()
            .filter(|(id, _)| *id == loser.0)
            .cloned()
            .collect();
        for key in loser_na_keys {
            let row = inner.node_assertions.remove(&key).unwrap();
            let new_key = (survivor.0.clone(), key.1.clone());
            let keep = match inner.node_assertions.get(&new_key) {
                Some(existing) => existing.fetched_at < row.fetched_at,
                None => true,
            };
            if keep {
                inner.node_assertions.insert(new_key, row);
            }
        }

        // 4. Repoint edges (fold PK collisions, merge assertions per source).
        let loser_edge_keys: Vec<(String, String, String)> = inner
            .edges
            .keys()
            .filter(|(s, d, _)| *s == loser.0 || *d == loser.0)
            .cloned()
            .collect();
        for old_key in loser_edge_keys {
            let assertions = inner.edges.remove(&old_key).unwrap();
            let new_key = (
                if old_key.0 == loser.0 {
                    survivor.0.clone()
                } else {
                    old_key.0.clone()
                },
                if old_key.1 == loser.0 {
                    survivor.0.clone()
                } else {
                    old_key.1.clone()
                },
                old_key.2.clone(),
            );
            let target = inner.edges.entry(new_key).or_default();
            for (source, row) in assertions {
                let keep = match target.get(&source) {
                    Some(existing) => existing.fetched_at < row.fetched_at,
                    None => true,
                };
                if keep {
                    target.insert(source, row);
                }
            }
        }

        // 5. Repoint payloads; mark loser versions not-current if survivor has a current one.
        let loser_payload_keys: Vec<(String, String)> = inner
            .payloads
            .keys()
            .filter(|(id, _)| *id == loser.0)
            .cloned()
            .collect();
        for old_key in loser_payload_keys {
            let mut rows = inner.payloads.remove(&old_key).unwrap();
            let new_key = (survivor.0.clone(), old_key.1.clone());
            let survivor_has_current = inner
                .payloads
                .get(&new_key)
                .map(|rs| rs.iter().any(|d| d.is_current))
                .unwrap_or(false);
            if survivor_has_current {
                for row in &mut rows {
                    row.is_current = false;
                }
            }
            inner
                .payloads
                .entry(new_key)
                .or_default()
                .extend(rows);
        }

        Ok(())
    }

    async fn read_node(
        &self,
        id: &CanonicalId,
    ) -> Result<
        Option<(NodeKind, Vec<(String, serde_json::Value, String)>, Vec<Alias>)>,
        DomainError,
    > {
        let inner = self.inner.borrow();
        let node = match inner.nodes.get(&id.0) {
            None => return Ok(None),
            Some(row) if row.merged_into.is_some() => return Ok(None),
            Some(row) => row,
        };
        let kind = node.kind.clone();

        let assertions: Vec<(String, serde_json::Value, String)> = inner
            .node_assertions
            .iter()
            .filter(|((nid, _), _)| *nid == id.0)
            .map(|((_, source), row)| (source.clone(), row.attrs.clone(), row.fetched_at.clone()))
            .collect();

        let aliases: Vec<Alias> = inner
            .aliases
            .iter()
            .filter(|(_, cid)| **cid == id.0)
            .map(|((ns, val), _)| Alias {
                namespace: ns.clone(),
                value: val.clone(),
            })
            .collect();

        Ok(Some((kind, assertions, aliases)))
    }

    async fn put_edge(
        &self,
        src: &CanonicalId,
        dst: &CanonicalId,
        relation: &str,
        source: &str,
        attrs: Option<&serde_json::Value>,
        fetched_at: &str,
    ) -> Result<(), DomainError> {
        let mut inner = self.inner.borrow_mut();
        let key = (src.0.clone(), dst.0.clone(), relation.to_string());
        let assertions = inner.edges.entry(key).or_default();
        assertions.insert(
            source.to_string(),
            EdgeAssertionRow {
                attrs: attrs.cloned(),
                fetched_at: fetched_at.to_string(),
            },
        );
        Ok(())
    }

    /// Stable-sorted pagination via opaque decimal-offset cursor.
    async fn read_edges(
        &self,
        id: &CanonicalId,
        dir: EdgeDir,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<EdgeView>, Option<String>), DomainError> {
        let inner = self.inner.borrow();

        let mut matching: Vec<(&(String, String, String), &EdgeAssertions)> = inner
            .edges
            .iter()
            .filter(|((src, dst, _), _)| match dir {
                EdgeDir::Forward => *src == id.0,
                EdgeDir::Backward => *dst == id.0,
            })
            .collect();

        // Stable sort for deterministic pagination: (other_end, relation).
        matching.sort_by(|(ka, _), (kb, _)| {
            let oa = match dir {
                EdgeDir::Forward => &ka.1,
                EdgeDir::Backward => &ka.0,
            };
            let ob = match dir {
                EdgeDir::Forward => &kb.1,
                EdgeDir::Backward => &kb.0,
            };
            oa.cmp(ob).then(ka.2.cmp(&kb.2))
        });

        let offset: usize = cursor.and_then(|c| c.parse().ok()).unwrap_or(0);
        let page = &matching[offset.min(matching.len())..];
        let limit = limit as usize;
        let has_more = page.len() > limit;
        let page = &page[..page.len().min(limit)];

        let views: Vec<EdgeView> = page
            .iter()
            .map(|((src, dst, rel), assertions)| {
                let assertion_list: Vec<serde_json::Value> = assertions
                    .iter()
                    .map(|(source, row)| {
                        let mut obj = serde_json::Map::new();
                        obj.insert(
                            "source".to_string(),
                            serde_json::Value::String(source.clone()),
                        );
                        obj.insert(
                            "fetched_at".to_string(),
                            serde_json::Value::String(row.fetched_at.clone()),
                        );
                        if let Some(a) = &row.attrs {
                            obj.insert("attrs".to_string(), a.clone());
                        }
                        serde_json::Value::Object(obj)
                    })
                    .collect();
                EdgeView {
                    src: CanonicalId(src.clone()),
                    dst: CanonicalId(dst.clone()),
                    relation: rel.clone(),
                    assertions: assertion_list,
                }
            })
            .collect();

        let next_cursor = if has_more {
            Some((offset + limit).to_string())
        } else {
            None
        };

        Ok((views, next_cursor))
    }

    async fn present_aliases(&self, aliases: &[Alias]) -> Result<Vec<Alias>, DomainError> {
        let inner = self.inner.borrow();
        let present: Vec<Alias> = aliases
            .iter()
            .filter(|a| {
                inner
                    .aliases
                    .contains_key(&(a.namespace.clone(), a.value.clone()))
            })
            .cloned()
            .collect();
        Ok(present)
    }

    async fn stats(&self) -> Result<GraphStats, DomainError> {
        let inner = self.inner.borrow();

        // Sort a (key → count) map into Tally rows: count desc, then key asc.
        let into_tallies = |map: HashMap<String, u64>| -> Vec<Tally> {
            let mut rows: Vec<Tally> = map
                .into_iter()
                .map(|(key, count)| Tally { key, count })
                .collect();
            rows.sort_by(|a, b| b.count.cmp(&a.count).then(a.key.cmp(&b.key)));
            rows
        };

        let kind_str = |k: &NodeKind| match k {
            NodeKind::Work => "work",
            NodeKind::Author => "author",
            NodeKind::Venue => "venue",
            NodeKind::Concept => "concept",
            NodeKind::Topic => "topic",
        };

        let mut nodes_total = 0u64;
        let mut tombstones = 0u64;
        let mut works = 0u64;
        let mut nodes_by_kind: HashMap<String, u64> = HashMap::new();
        for row in inner.nodes.values() {
            if row.merged_into.is_some() {
                tombstones += 1;
                continue;
            }
            nodes_total += 1;
            *nodes_by_kind.entry(kind_str(&row.kind).to_string()).or_insert(0) += 1;
            if row.kind == NodeKind::Work {
                works += 1;
            }
        }

        // Works with at least one assertion, restricted to live work nodes.
        let mut described: std::collections::HashSet<&String> = std::collections::HashSet::new();
        for (id, _source) in inner.node_assertions.keys() {
            match inner.nodes.get(id) {
                Some(row) if row.merged_into.is_none() && row.kind == NodeKind::Work => {
                    described.insert(id);
                }
                _ => {}
            }
        }
        let works_described = described.len() as u64;

        let mut edges_by_relation: HashMap<String, u64> = HashMap::new();
        for (_src, _dst, relation) in inner.edges.keys() {
            *edges_by_relation.entry(relation.clone()).or_insert(0) += 1;
        }
        let edges_total = inner.edges.len() as u64;

        let mut assertions_by_source: HashMap<String, u64> = HashMap::new();
        for (_id, source) in inner.node_assertions.keys() {
            *assertions_by_source.entry(source.clone()).or_insert(0) += 1;
        }

        Ok(GraphStats {
            works,
            works_described,
            works_stub: works.saturating_sub(works_described),
            nodes_total,
            nodes_by_kind: into_tallies(nodes_by_kind),
            tombstones,
            edges_total,
            edges_by_relation: into_tallies(edges_by_relation),
            assertions_by_source: into_tallies(assertions_by_source),
        })
    }
}
