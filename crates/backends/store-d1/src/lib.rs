//! `ArtifactStore` + `MetadataStore` backed by Cloudflare D1.
//!
//! Without the `cloudflare` feature, stubs are compiled for host builds.
//! With the `cloudflare` feature, the real implementation uses `workers-rs`
//! prepared statements with the shared `braincrawl_sql` query strings.
//!
//! ## Transactions
//!
//! D1 does not expose `BEGIN / COMMIT`. The `merge` operation uses
//! `D1Database::batch()` which executes a list of statements atomically.
//!
//! ## Path-compression in `resolve_live`
//!
//! Each hop requires one D1 round-trip. Chain length is normally 0–1 in practice.
//! Path-compression updates are batched after walking the chain.

#![allow(dead_code)]

use async_trait::async_trait;
use braincrawl_core::{
    traits::{MetadataStore, ArtifactStore},
    types::{Alias, CanonicalId, DomainError, EdgeDir, EdgeView, GraphStats, NodeKind, Artifact, ArtifactRole},
};
#[cfg(feature = "cloudflare")]
use braincrawl_core::types::Tally;

// ── shared helpers (no worker deps) ──────────────────────────────────────────

fn node_kind_str(k: &NodeKind) -> &'static str {
    match k {
        NodeKind::Work => "work",
        NodeKind::Author => "author",
        NodeKind::Venue => "venue",
        NodeKind::Concept => "concept",
        NodeKind::Topic => "topic",
    }
}

fn parse_node_kind(s: &str) -> Result<NodeKind, DomainError> {
    match s {
        "work" => Ok(NodeKind::Work),
        "author" => Ok(NodeKind::Author),
        "venue" => Ok(NodeKind::Venue),
        "concept" => Ok(NodeKind::Concept),
        "topic" => Ok(NodeKind::Topic),
        other => Err(DomainError::Backend(format!("unknown NodeKind: {other}"))),
    }
}

fn artifact_role_str(k: &ArtifactRole) -> &str {
    k.as_str()
}

fn parse_artifact_role(s: &str) -> Result<ArtifactRole, DomainError> {
    ArtifactRole::parse(s)
        .ok_or_else(|| DomainError::Backend(format!("unknown ArtifactRole: {s}")))
}

fn be(e: impl std::fmt::Display) -> DomainError {
    DomainError::Backend(e.to_string())
}

// ── stub (no `cloudflare` feature) ───────────────────────────────────────────

#[cfg(not(feature = "cloudflare"))]
pub struct D1Store;

#[cfg(not(feature = "cloudflare"))]
#[async_trait(?Send)]
impl ArtifactStore for D1Store {
    async fn current_artifact(&self, _id: &CanonicalId, _kind: ArtifactRole) -> Result<Option<Artifact>, DomainError> {
        Err(DomainError::Backend("D1Store: cloudflare feature not enabled".into()))
    }
    async fn next_version(&self, _id: &CanonicalId, _kind: ArtifactRole) -> Result<u32, DomainError> {
        Err(DomainError::Backend("D1Store: cloudflare feature not enabled".into()))
    }
    async fn record(&self, _descriptor: &Artifact) -> Result<(), DomainError> {
        Err(DomainError::Backend("D1Store: cloudflare feature not enabled".into()))
    }
}

#[cfg(not(feature = "cloudflare"))]
#[async_trait(?Send)]
impl MetadataStore for D1Store {
    async fn get_alias(&self, _alias: &Alias) -> Result<Option<CanonicalId>, DomainError> {
        Err(DomainError::Backend("D1Store: cloudflare feature not enabled".into()))
    }
    async fn get_or_create_alias(&self, _alias: &Alias, _candidate: &CanonicalId) -> Result<CanonicalId, DomainError> {
        Err(DomainError::Backend("D1Store: cloudflare feature not enabled".into()))
    }
    async fn create_node(&self, _id: &CanonicalId, _kind: NodeKind, _created_at: &str) -> Result<(), DomainError> {
        Err(DomainError::Backend("D1Store: cloudflare feature not enabled".into()))
    }
    async fn upsert_node_assertion(&self, _id: &CanonicalId, _source: &str, _attrs: &serde_json::Value, _fetched_at: &str) -> Result<(), DomainError> {
        Err(DomainError::Backend("D1Store: cloudflare feature not enabled".into()))
    }
    async fn resolve_live(&self, _id: &CanonicalId) -> Result<CanonicalId, DomainError> {
        Err(DomainError::Backend("D1Store: cloudflare feature not enabled".into()))
    }
    async fn merge(&self, _survivor: &CanonicalId, _loser: &CanonicalId) -> Result<(), DomainError> {
        Err(DomainError::Backend("D1Store: cloudflare feature not enabled".into()))
    }
    async fn read_node(&self, _id: &CanonicalId) -> Result<Option<(NodeKind, Vec<(String, serde_json::Value, String)>, Vec<Alias>)>, DomainError> {
        Err(DomainError::Backend("D1Store: cloudflare feature not enabled".into()))
    }
    async fn put_edge(&self, _src: &CanonicalId, _dst: &CanonicalId, _relation: &str, _source: &str, _attrs: Option<&serde_json::Value>, _fetched_at: &str) -> Result<(), DomainError> {
        Err(DomainError::Backend("D1Store: cloudflare feature not enabled".into()))
    }
    async fn read_edges(&self, _id: &CanonicalId, _dir: EdgeDir, _cursor: Option<&str>, _limit: u32) -> Result<(Vec<EdgeView>, Option<String>), DomainError> {
        Err(DomainError::Backend("D1Store: cloudflare feature not enabled".into()))
    }
    async fn present_aliases(&self, _aliases: &[Alias]) -> Result<Vec<Alias>, DomainError> {
        Err(DomainError::Backend("D1Store: cloudflare feature not enabled".into()))
    }
    async fn stats(&self) -> Result<GraphStats, DomainError> {
        Err(DomainError::Backend("D1Store: cloudflare feature not enabled".into()))
    }
}

// ── real impl (cloudflare feature — wasm32 only) ─────────────────────────────

#[cfg(feature = "cloudflare")]
use worker::wasm_bindgen::JsValue;

// Parameter-building helpers (cloudflare only).
#[cfg(feature = "cloudflare")]
fn s(v: &str) -> JsValue { JsValue::from_str(v) }
#[cfg(feature = "cloudflare")]
fn n(v: i64) -> JsValue { JsValue::from(v as f64) }
#[cfg(feature = "cloudflare")]
fn opt_s(v: Option<&str>) -> JsValue {
    match v { Some(x) => JsValue::from_str(x), None => JsValue::null() }
}
#[cfg(feature = "cloudflare")]
fn bool_int(b: bool) -> JsValue { JsValue::from(if b { 1.0f64 } else { 0.0 }) }

#[cfg(feature = "cloudflare")]
fn prep(
    db: &worker::D1Database,
    sql: &str,
    params: &[JsValue],
) -> Result<worker::D1PreparedStatement, DomainError> {
    db.prepare(sql).bind(params).map_err(be)
}

#[cfg(feature = "cloudflare")]
pub struct D1Store {
    db: worker::D1Database,
}

#[cfg(feature = "cloudflare")]
impl D1Store {
    pub fn new(db: worker::D1Database) -> Self {
        Self { db }
    }
}

// ── ArtifactStore ──────────────────────────────────────────────────────────────

#[cfg(feature = "cloudflare")]
#[async_trait(?Send)]
impl ArtifactStore for D1Store {
    async fn current_artifact(
        &self,
        id: &CanonicalId,
        kind: ArtifactRole,
    ) -> Result<Option<Artifact>, DomainError> {
        #[derive(serde::Deserialize)]
        struct Row {
            canonical_id: String,
            role: String,
            version: i64,
            r2_key: String,
            content_hash: String,
            byte_size: i64,
            mime: String,
            source: Option<String>,
            source_url: Option<String>,
            fetched_at: String,
            is_current: i32,
        }
        let stmt = prep(
            &self.db,
            braincrawl_sql::artifact::SELECT_CURRENT,
            &[s(&id.0), s(artifact_role_str(&kind))],
        )?;
        let row = stmt.first::<Row>(None).await.map_err(be)?;
        match row {
            None => Ok(None),
            Some(r) => Ok(Some(Artifact {
                canonical_id: CanonicalId(r.canonical_id),
                role: parse_artifact_role(&r.role)?,
                version: r.version as u32,
                r2_key: r.r2_key,
                content_hash: r.content_hash,
                byte_size: r.byte_size as u64,
                mime: r.mime,
                source: r.source,
                source_url: r.source_url,
                fetched_at: r.fetched_at,
                is_current: r.is_current != 0,
            })),
        }
    }

    async fn next_version(&self, id: &CanonicalId, kind: ArtifactRole) -> Result<u32, DomainError> {
        #[derive(serde::Deserialize)]
        struct Row { next_version: i64 }
        let stmt = prep(
            &self.db,
            braincrawl_sql::artifact::NEXT_VERSION,
            &[s(&id.0), s(artifact_role_str(&kind))],
        )?;
        let row = stmt.first::<Row>(None).await.map_err(be)?;
        Ok(row.map(|r| r.next_version as u32).unwrap_or(1))
    }

    async fn record(&self, d: &Artifact) -> Result<(), DomainError> {
        let mut stmts = Vec::new();
        if d.is_current {
            stmts.push(prep(
                &self.db,
                braincrawl_sql::artifact::FLIP_CURRENT_OFF,
                &[s(&d.canonical_id.0), s(artifact_role_str(&d.role))],
            )?);
        }
        stmts.push(prep(
            &self.db,
            braincrawl_sql::artifact::INSERT,
            &[
                s(&d.canonical_id.0),
                s(artifact_role_str(&d.role)),
                n(d.version as i64),
                s(&d.r2_key),
                s(&d.content_hash),
                n(d.byte_size as i64),
                s(&d.mime),
                opt_s(d.source.as_deref()),
                opt_s(d.source_url.as_deref()),
                s(&d.fetched_at),
                bool_int(d.is_current),
            ],
        )?);
        self.db.batch(stmts).await.map_err(be)?;
        Ok(())
    }
}

// ── MetadataStore ─────────────────────────────────────────────────────────────

#[cfg(feature = "cloudflare")]
#[async_trait(?Send)]
impl MetadataStore for D1Store {
    async fn get_alias(&self, alias: &Alias) -> Result<Option<CanonicalId>, DomainError> {
        #[derive(serde::Deserialize)]
        struct Row { canonical_id: String }
        let stmt = prep(
            &self.db,
            braincrawl_sql::alias::GET,
            &[s(&alias.namespace), s(&alias.value)],
        )?;
        let row = stmt.first::<Row>(None).await.map_err(be)?;
        Ok(row.map(|r| CanonicalId(r.canonical_id)))
    }

    async fn get_or_create_alias(
        &self,
        alias: &Alias,
        candidate: &CanonicalId,
    ) -> Result<CanonicalId, DomainError> {
        #[derive(serde::Deserialize)]
        struct Row { canonical_id: String }
        let insert = prep(
            &self.db,
            braincrawl_sql::alias::INSERT_IGNORE,
            &[s(&alias.namespace), s(&alias.value), s(&candidate.0)],
        )?;
        let select = prep(
            &self.db,
            braincrawl_sql::alias::GET,
            &[s(&alias.namespace), s(&alias.value)],
        )?;
        let results = self.db.batch(vec![insert, select]).await.map_err(be)?;
        let rows = results
            .into_iter()
            .nth(1)
            .ok_or_else(|| DomainError::Backend("D1 batch returned no results".into()))?
            .results::<Row>()
            .map_err(be)?;
        rows.into_iter()
            .next()
            .map(|r| CanonicalId(r.canonical_id))
            .ok_or(DomainError::Backend("get_or_create_alias: no row returned".into()))
    }

    async fn create_node(&self, id: &CanonicalId, kind: NodeKind, created_at: &str) -> Result<(), DomainError> {
        prep(
            &self.db,
            braincrawl_sql::node::INSERT_IGNORE,
            &[s(&id.0), s(node_kind_str(&kind)), s(created_at)],
        )?
        .run()
        .await
        .map_err(be)?;
        Ok(())
    }

    async fn upsert_node_assertion(
        &self,
        id: &CanonicalId,
        source: &str,
        attrs: &serde_json::Value,
        fetched_at: &str,
    ) -> Result<(), DomainError> {
        let attrs_s = serde_json::to_string(attrs).map_err(|e| DomainError::Serde(e.to_string()))?;
        prep(
            &self.db,
            braincrawl_sql::node_assertion::UPSERT,
            &[s(&id.0), s(source), s(&attrs_s), s(fetched_at)],
        )?
        .run()
        .await
        .map_err(be)?;
        Ok(())
    }

    async fn resolve_live(&self, id: &CanonicalId) -> Result<CanonicalId, DomainError> {
        #[derive(serde::Deserialize)]
        struct Row { merged_into: Option<String> }
        let mut chain: Vec<String> = Vec::new();
        let mut cur = id.0.clone();
        // Walk merged_into chain; guard against cycles (impossible by schema, but safe).
        for _ in 0..1_000 {
            let stmt = prep(
                &self.db,
                braincrawl_sql::node::SELECT_MERGED_INTO,
                &[s(&cur)],
            )?;
            let row = stmt.first::<Row>(None).await.map_err(be)?;
            let merged_into = row
                .ok_or(DomainError::NotFound)?
                .merged_into;
            match merged_into {
                None => break,
                Some(next) => {
                    chain.push(cur.clone());
                    cur = next;
                }
            }
        }
        // Path-compress: point all intermediates directly to the live root.
        if !chain.is_empty() {
            let stmts: Vec<worker::D1PreparedStatement> = chain
                .iter()
                .map(|intermediate| {
                    prep(&self.db, braincrawl_sql::node::TOMBSTONE, &[s(&cur), s(intermediate)])
                })
                .collect::<Result<_, _>>()?;
            self.db.batch(stmts).await.map_err(be)?;
        }
        Ok(CanonicalId(cur))
    }

    async fn merge(&self, survivor: &CanonicalId, loser: &CanonicalId) -> Result<(), DomainError> {
        let sv = &survivor.0;
        let ls = &loser.0;
        // All 16 merge steps executed as a single atomic D1 batch (doc02.04 ordering).
        let stmts = vec![
            // 1. Tombstone loser.
            prep(&self.db, braincrawl_sql::node::TOMBSTONE, &[s(sv), s(ls)])?,
            // 2. Alias merge.
            prep(&self.db, braincrawl_sql::alias::MERGE_DELETE_CONFLICTS, &[s(ls), s(sv)])?,
            prep(&self.db, braincrawl_sql::alias::MERGE_REPOINT, &[s(sv), s(ls)])?,
            // 3. Node assertion merge.
            prep(&self.db, braincrawl_sql::node_assertion::MERGE_UPSERT, &[s(sv), s(ls)])?,
            prep(&self.db, braincrawl_sql::node_assertion::MERGE_DELETE_LOSER, &[s(ls)])?,
            // 4. Edge assertions (src direction).
            prep(&self.db, braincrawl_sql::edge::MERGE_EA_SRC_UPSERT, &[s(sv), s(ls)])?,
            prep(&self.db, braincrawl_sql::edge::MERGE_EA_SRC_DELETE_LOSER, &[s(ls)])?,
            // 5. Edge src repoint.
            prep(&self.db, braincrawl_sql::edge::MERGE_EDGE_SRC_DELETE_CONFLICTS, &[s(ls), s(sv)])?,
            prep(&self.db, braincrawl_sql::edge::MERGE_EDGE_SRC_REPOINT, &[s(sv), s(ls)])?,
            // 6. Edge assertions (dst direction).
            prep(&self.db, braincrawl_sql::edge::MERGE_EA_DST_UPSERT, &[s(sv), s(ls)])?,
            prep(&self.db, braincrawl_sql::edge::MERGE_EA_DST_DELETE_LOSER, &[s(ls)])?,
            // 7. Edge dst repoint.
            prep(&self.db, braincrawl_sql::edge::MERGE_EDGE_DST_DELETE_CONFLICTS, &[s(ls), s(sv)])?,
            prep(&self.db, braincrawl_sql::edge::MERGE_EDGE_DST_REPOINT, &[s(sv), s(ls)])?,
            // 8. Artifact merge.
            prep(&self.db, braincrawl_sql::artifact::MERGE_DEMOTE_LOSER_CURRENT, &[s(ls), s(sv)])?,
            prep(&self.db, braincrawl_sql::artifact::MERGE_REPOINT, &[s(sv), s(ls)])?,
            prep(&self.db, braincrawl_sql::artifact::MERGE_DELETE_LOSER, &[s(ls)])?,
        ];
        self.db.batch(stmts).await.map_err(be)?;
        Ok(())
    }

    async fn read_node(
        &self,
        id: &CanonicalId,
    ) -> Result<Option<(NodeKind, Vec<(String, serde_json::Value, String)>, Vec<Alias>)>, DomainError> {
        #[derive(serde::Deserialize)]
        struct NodeRow { kind: String, merged_into: Option<String> }
        #[derive(serde::Deserialize)]
        struct AssertionRow { source: String, attrs: String, fetched_at: String }
        #[derive(serde::Deserialize)]
        struct AliasRow { namespace: String, value: String }

        let node_stmt = prep(&self.db, braincrawl_sql::node::SELECT, &[s(&id.0)])?;
        let assn_stmt = prep(&self.db, braincrawl_sql::node_assertion::SELECT_BY_NODE, &[s(&id.0)])?;
        let alias_stmt = prep(&self.db, braincrawl_sql::alias::LIST_BY_NODE, &[s(&id.0)])?;

        let results = self.db.batch(vec![node_stmt, assn_stmt, alias_stmt]).await.map_err(be)?;
        let mut iter = results.into_iter();

        let node_rows = iter.next().unwrap().results::<NodeRow>().map_err(be)?;
        let node_row = match node_rows.into_iter().next() {
            None => return Ok(None),
            Some(r) => r,
        };
        if node_row.merged_into.is_some() {
            return Ok(None); // tombstoned
        }
        let kind = parse_node_kind(&node_row.kind)?;

        let assertions: Vec<(String, serde_json::Value, String)> = iter
            .next()
            .unwrap()
            .results::<AssertionRow>()
            .map_err(be)?
            .into_iter()
            .map(|r| {
                let attrs = serde_json::from_str(&r.attrs)
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                (r.source, attrs, r.fetched_at)
            })
            .collect();

        let aliases: Vec<Alias> = iter
            .next()
            .unwrap()
            .results::<AliasRow>()
            .map_err(be)?
            .into_iter()
            .map(|r| Alias { namespace: r.namespace, value: r.value })
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
        let attrs_s: JsValue = match attrs {
            Some(v) => {
                let js = serde_json::to_string(v).map_err(|e| DomainError::Serde(e.to_string()))?;
                s(&js)
            }
            None => JsValue::null(),
        };
        let edge_stmt = prep(
            &self.db,
            braincrawl_sql::edge::INSERT_IGNORE,
            &[s(&src.0), s(&dst.0), s(relation)],
        )?;
        let ea_stmt = prep(
            &self.db,
            braincrawl_sql::edge_assertion::UPSERT,
            &[s(&src.0), s(&dst.0), s(relation), s(source), attrs_s, s(fetched_at)],
        )?;
        self.db.batch(vec![edge_stmt, ea_stmt]).await.map_err(be)?;
        Ok(())
    }

    async fn read_edges(
        &self,
        id: &CanonicalId,
        dir: EdgeDir,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<EdgeView>, Option<String>), DomainError> {
        #[derive(serde::Deserialize)]
        struct EdgeRow { src_id: String, dst_id: String, relation: String }
        #[derive(serde::Deserialize)]
        struct EaRow { source: String, attrs: Option<String>, fetched_at: String }

        let cursor_decoded: Option<(String, String)> = cursor.and_then(|c| {
            let v: serde_json::Value = serde_json::from_str(c).ok()?;
            Some((v["a"].as_str()?.to_string(), v["r"].as_str()?.to_string()))
        });

        let fetch_limit = limit + 1;
        let edge_rows: Vec<EdgeRow> = {
            let sql = match (&dir, cursor_decoded.is_some()) {
                (EdgeDir::Forward, false) => braincrawl_sql::edge::SELECT_FORWARD_FIRST,
                (EdgeDir::Forward, true) => braincrawl_sql::edge::SELECT_FORWARD_PAGE,
                (EdgeDir::Backward, false) => braincrawl_sql::edge::SELECT_BACKWARD_FIRST,
                (EdgeDir::Backward, true) => braincrawl_sql::edge::SELECT_BACKWARD_PAGE,
            };
            let params: Vec<JsValue> = match (&dir, &cursor_decoded) {
                (_, None) => vec![s(&id.0), n(fetch_limit as i64)],
                (_, Some((last_other, last_rel))) => {
                    vec![s(&id.0), s(last_other), s(last_other), s(last_rel), n(fetch_limit as i64)]
                }
            };
            let stmt = prep(&self.db, sql, &params)?;
            stmt.all().await.map_err(be)?.results::<EdgeRow>().map_err(be)?
        };

        let has_more = edge_rows.len() > limit as usize;
        let page: Vec<EdgeRow> = edge_rows.into_iter().take(limit as usize).collect();

        // Batch-fetch edge_assertions for all edges on this page.
        if page.is_empty() {
            return Ok((vec![], None));
        }
        let ea_stmts: Vec<worker::D1PreparedStatement> = page
            .iter()
            .map(|e| {
                prep(
                    &self.db,
                    braincrawl_sql::edge_assertion::SELECT_BY_EDGE,
                    &[s(&e.src_id), s(&e.dst_id), s(&e.relation)],
                )
            })
            .collect::<Result<_, _>>()?;
        let ea_results = self.db.batch(ea_stmts).await.map_err(be)?;

        let mut views = Vec::with_capacity(page.len());
        for (e, ea_result) in page.iter().zip(ea_results.into_iter()) {
            let assertions: Vec<serde_json::Value> = ea_result
                .results::<EaRow>()
                .map_err(be)?
                .into_iter()
                .map(|r| {
                    let mut obj = serde_json::Map::new();
                    obj.insert("source".into(), serde_json::Value::String(r.source));
                    obj.insert("fetched_at".into(), serde_json::Value::String(r.fetched_at));
                    if let Some(attrs_str) = r.attrs {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&attrs_str) {
                            if !v.is_null() {
                                obj.insert("attrs".into(), v);
                            }
                        }
                    }
                    serde_json::Value::Object(obj)
                })
                .collect();
            views.push(EdgeView {
                src: CanonicalId(e.src_id.clone()),
                dst: CanonicalId(e.dst_id.clone()),
                relation: e.relation.clone(),
                assertions,
            });
        }

        let next_cursor = if has_more {
            page.last().map(|e| {
                let other = match dir {
                    EdgeDir::Forward => &e.dst_id,
                    EdgeDir::Backward => &e.src_id,
                };
                serde_json::json!({"a": other, "r": e.relation}).to_string()
            })
        } else {
            None
        };

        Ok((views, next_cursor))
    }

    async fn present_aliases(&self, aliases: &[Alias]) -> Result<Vec<Alias>, DomainError> {
        if aliases.is_empty() {
            return Ok(Vec::new());
        }
        #[derive(serde::Deserialize)]
        struct Row { namespace: String, value: String }
        // Two bound params per pair; production D1 caps ~100 params per statement.
        const PAIRS_PER_CHUNK: usize = 45;
        let mut out = Vec::new();
        for chunk in aliases.chunks(PAIRS_PER_CHUNK) {
            let pair_list = braincrawl_sql::alias_pair_list(chunk.len());
            let query = format!(
                "SELECT namespace, value FROM alias WHERE (namespace, value) IN {pair_list}"
            );
            let params: Vec<JsValue> = chunk
                .iter()
                .flat_map(|a| [s(&a.namespace), s(&a.value)])
                .collect();
            let stmt = prep(&self.db, &query, &params)?;
            let rows = stmt.all().await.map_err(be)?.results::<Row>().map_err(be)?;
            out.extend(
                rows.into_iter()
                    .map(|r| Alias { namespace: r.namespace, value: r.value }),
            );
        }
        Ok(out)
    }

    async fn stats(&self) -> Result<GraphStats, DomainError> {
        #[derive(serde::Deserialize)]
        struct CountRow { count: i64 }
        #[derive(serde::Deserialize)]
        struct TallyRow { key: String, count: i64 }

        use braincrawl_sql::stats as q;

        // One D1 batch keeps all eight reads on a single round-trip, in order.
        let results = self
            .db
            .batch(vec![
                prep(&self.db, q::WORKS, &[])?,
                prep(&self.db, q::WORKS_DESCRIBED, &[])?,
                prep(&self.db, q::NODES_TOTAL, &[])?,
                prep(&self.db, q::TOMBSTONES, &[])?,
                prep(&self.db, q::EDGES_TOTAL, &[])?,
                prep(&self.db, q::LIBRARY_BYTES, &[])?,
                prep(&self.db, q::NODES_BY_KIND, &[])?,
                prep(&self.db, q::EDGES_BY_RELATION, &[])?,
                prep(&self.db, q::ASSERTIONS_BY_SOURCE, &[])?,
            ])
            .await
            .map_err(be)?;
        let mut iter = results.into_iter();

        let mut count = |label: &str| -> Result<u64, DomainError> {
            let row = iter
                .next()
                .ok_or_else(|| DomainError::Backend(format!("stats: missing result for {label}")))?
                .results::<CountRow>()
                .map_err(be)?
                .into_iter()
                .next()
                .ok_or_else(|| DomainError::Backend(format!("stats: empty result for {label}")))?;
            Ok(row.count as u64)
        };
        let works = count("works")?;
        let works_described = count("works_described")?;
        let nodes_total = count("nodes_total")?;
        let tombstones = count("tombstones")?;
        let edges_total = count("edges_total")?;
        let library_bytes = count("library_bytes")?;

        let mut tally = |label: &str| -> Result<Vec<Tally>, DomainError> {
            let rows = iter
                .next()
                .ok_or_else(|| DomainError::Backend(format!("stats: missing result for {label}")))?
                .results::<TallyRow>()
                .map_err(be)?;
            Ok(rows
                .into_iter()
                .map(|r| Tally { key: r.key, count: r.count as u64 })
                .collect())
        };
        let nodes_by_kind = tally("nodes_by_kind")?;
        let edges_by_relation = tally("edges_by_relation")?;
        let assertions_by_source = tally("assertions_by_source")?;

        Ok(GraphStats {
            works,
            works_described,
            works_stub: works.saturating_sub(works_described),
            nodes_total,
            nodes_by_kind,
            tombstones,
            edges_total,
            edges_by_relation,
            assertions_by_source,
            library_bytes,
            // D1 reports storage via its own platform metrics, not a queryable file size.
            catalog_bytes: 0,
            total_bytes: library_bytes,
        })
    }
}
