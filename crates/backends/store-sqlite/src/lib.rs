//! `PayloadsRepo` + `MetadataStore` backed by local SQLite via rusqlite.
//!
//! Uses `std::sync::Mutex<Connection>` so that `SqliteStore: Send + Sync`.
//! On a single-threaded runtime the mutex is never actually contended.
//! Migrations are applied idempotently on construction via an internal `_migrations` table.

#![allow(dead_code)]

use std::sync::Mutex;

use async_trait::async_trait;
use braincrawl_core::{
    traits::{JobEnqueuer, JobQueue, MetadataStore, PayloadsRepo},
    types::{
        Alias, CanonicalId, DomainError, EdgeDir, EdgeView, GraphStats, Job, JobId, JobKind,
        JobSpec, NodeKind, PayloadDescriptor, PayloadKind, Rights, Tally,
    },
};
use rusqlite::{params, Connection, OptionalExtension};

// ─── string helpers ───────────────────────────────────────────────────────────

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

fn payload_kind_str(k: &PayloadKind) -> &'static str {
    match k {
        PayloadKind::Abstract => "abstract",
        PayloadKind::Fulltext => "fulltext",
    }
}

fn parse_payload_kind(s: &str) -> Result<PayloadKind, DomainError> {
    match s {
        "abstract" => Ok(PayloadKind::Abstract),
        "fulltext" => Ok(PayloadKind::Fulltext),
        other => Err(DomainError::Backend(format!("unknown PayloadKind: {other}"))),
    }
}

fn rights_str(r: &Rights) -> &'static str {
    match r {
        Rights::Open => "open",
        Rights::LinkOnly => "link_only",
        Rights::Restricted => "restricted",
    }
}

fn parse_rights(s: &str) -> Result<Rights, DomainError> {
    match s {
        "open" => Ok(Rights::Open),
        "link_only" => Ok(Rights::LinkOnly),
        "restricted" => Ok(Rights::Restricted),
        other => Err(DomainError::Backend(format!("unknown Rights: {other}"))),
    }
}

fn be(e: rusqlite::Error) -> DomainError {
    DomainError::Backend(e.to_string())
}

fn is_no_rows(e: &rusqlite::Error) -> bool {
    matches!(e, rusqlite::Error::QueryReturnedNoRows)
}

// ─── migrations ───────────────────────────────────────────────────────────────

fn apply_migrations(conn: &Connection) -> Result<(), DomainError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations \
         (name TEXT PRIMARY KEY, applied_at TEXT NOT NULL);",
    )
    .map_err(be)?;

    for (name, sql) in braincrawl_sql::migrations() {
        let already: bool = conn
            .query_row(
                "SELECT 1 FROM _migrations WHERE name = ?1",
                [name],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if !already {
            conn.execute_batch(sql).map_err(be)?;
            conn.execute(
                "INSERT INTO _migrations (name, applied_at) VALUES (?1, datetime('now'))",
                [name],
            )
            .map_err(be)?;
        }
    }
    Ok(())
}

// ─── SqliteStore ──────────────────────────────────────────────────────────────

/// SQLite-backed `MetadataStore` + `PayloadsRepo`.
///
/// `std::sync::Mutex<Connection>` makes the type `Send + Sync`; on a
/// single-threaded runtime the mutex is never actually contended.
pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    /// Open (or create) a SQLite database at `path` and apply migrations.
    pub fn open(path: &str) -> Result<Self, DomainError> {
        let conn = Connection::open(path).map_err(be)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=OFF;")
            .map_err(be)?;
        apply_migrations(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

// ─── PayloadsRepo ─────────────────────────────────────────────────────────────

#[async_trait(?Send)]
impl PayloadsRepo for SqliteStore {
    async fn current_payload(
        &self,
        id: &CanonicalId,
        kind: PayloadKind,
    ) -> Result<Option<PayloadDescriptor>, DomainError> {
        let conn = self.conn.lock().unwrap();
        let ks = payload_kind_str(&kind);
        conn.query_row(
            braincrawl_sql::payload::SELECT_CURRENT,
            params![id.0, ks],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, i32>(11)?,
                ))
            },
        )
        .map(|(cid, ks2, ver, r2k, ch, bs, mime, rs, src, su, fa, ic)| {
            Some(PayloadDescriptor {
                canonical_id: CanonicalId(cid),
                kind: parse_payload_kind(&ks2).unwrap_or(PayloadKind::Abstract),
                version: ver as u32,
                r2_key: r2k,
                content_hash: ch,
                byte_size: bs as u64,
                mime,
                rights: parse_rights(&rs).unwrap_or(Rights::Open),
                source: src,
                source_url: su,
                fetched_at: fa,
                is_current: ic != 0,
            })
        })
        .or_else(|e| if is_no_rows(&e) { Ok(None) } else { Err(be(e)) })
    }

    async fn next_version(&self, id: &CanonicalId, kind: PayloadKind) -> Result<u32, DomainError> {
        let conn = self.conn.lock().unwrap();
        let ks = payload_kind_str(&kind);
        let v: i64 = conn
            .query_row(
                braincrawl_sql::payload::NEXT_VERSION,
                params![id.0, ks],
                |row| row.get(0),
            )
            .map_err(be)?;
        Ok(v as u32)
    }

    async fn record(&self, d: &PayloadDescriptor) -> Result<(), DomainError> {
        let conn = self.conn.lock().unwrap();
        let ks = payload_kind_str(&d.kind);
        let rs = rights_str(&d.rights);
        if d.is_current {
            conn.execute(
                braincrawl_sql::payload::FLIP_CURRENT_OFF,
                params![d.canonical_id.0, ks],
            )
            .map_err(be)?;
        }
        conn.execute(
            braincrawl_sql::payload::INSERT,
            params![
                d.canonical_id.0,
                ks,
                d.version as i64,
                d.r2_key,
                d.content_hash,
                d.byte_size as i64,
                d.mime,
                rs,
                d.source,
                d.source_url,
                d.fetched_at,
                if d.is_current { 1i32 } else { 0 },
            ],
        )
        .map_err(be)?;
        Ok(())
    }
}

// ─── MetadataStore ────────────────────────────────────────────────────────────

#[async_trait(?Send)]
impl MetadataStore for SqliteStore {
    async fn get_alias(&self, alias: &Alias) -> Result<Option<CanonicalId>, DomainError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            braincrawl_sql::alias::GET,
            params![alias.namespace, alias.value],
            |row| row.get::<_, String>(0),
        )
        .map(|id| Some(CanonicalId(id)))
        .or_else(|e| if is_no_rows(&e) { Ok(None) } else { Err(be(e)) })
    }

    async fn get_or_create_alias(
        &self,
        alias: &Alias,
        candidate: &CanonicalId,
    ) -> Result<CanonicalId, DomainError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            braincrawl_sql::alias::INSERT_IGNORE,
            params![alias.namespace, alias.value, candidate.0],
        )
        .map_err(be)?;
        let id: String = conn
            .query_row(
                braincrawl_sql::alias::GET,
                params![alias.namespace, alias.value],
                |row| row.get(0),
            )
            .map_err(be)?;
        Ok(CanonicalId(id))
    }

    async fn mint_node(
        &self,
        id: &CanonicalId,
        kind: NodeKind,
        created_at: &str,
    ) -> Result<(), DomainError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            braincrawl_sql::node::INSERT_IGNORE,
            params![id.0, node_kind_str(&kind), created_at],
        )
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
        let conn = self.conn.lock().unwrap();
        let attrs_s =
            serde_json::to_string(attrs).map_err(|e| DomainError::Serde(e.to_string()))?;
        conn.execute(
            braincrawl_sql::node_assertion::UPSERT,
            params![id.0, source, attrs_s, fetched_at],
        )
        .map_err(be)?;
        Ok(())
    }

    async fn resolve_live(&self, id: &CanonicalId) -> Result<CanonicalId, DomainError> {
        let conn = self.conn.lock().unwrap();
        let mut chain: Vec<String> = Vec::new();
        let mut cur = id.0.clone();
        // Cycle guard: walk at most 1 000 hops (cycles cannot happen by schema, but guard anyway).
        for _ in 0..1_000 {
            let merged_into: Option<String> = conn
                .query_row(
                    braincrawl_sql::node::SELECT_MERGED_INTO,
                    params![cur],
                    |row| row.get(0),
                )
                .map_err(|e| if is_no_rows(&e) { DomainError::NotFound } else { be(e) })?;
            match merged_into {
                None => break,
                Some(next) => {
                    chain.push(cur.clone());
                    cur = next;
                }
            }
        }
        // Path-compress: point all intermediate nodes directly to the live root.
        for intermediate in &chain {
            conn.execute(braincrawl_sql::node::TOMBSTONE, params![cur, intermediate])
                .map_err(be)?;
        }
        Ok(CanonicalId(cur))
    }

    async fn merge(
        &self,
        survivor: &CanonicalId,
        loser: &CanonicalId,
    ) -> Result<(), DomainError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("BEGIN IMMEDIATE;").map_err(be)?;
        let result: Result<(), DomainError> = (|| {
            // 1. Tombstone loser.
            conn.execute(braincrawl_sql::node::TOMBSTONE, params![survivor.0, loser.0])
                .map_err(be)?;
            // 2. Alias merge.
            conn.execute(
                braincrawl_sql::alias::MERGE_DELETE_CONFLICTS,
                params![loser.0, survivor.0],
            )
            .map_err(be)?;
            conn.execute(
                braincrawl_sql::alias::MERGE_REPOINT,
                params![survivor.0, loser.0],
            )
            .map_err(be)?;
            // 3. Node assertion merge.
            conn.execute(
                braincrawl_sql::node_assertion::MERGE_UPSERT,
                params![survivor.0, loser.0],
            )
            .map_err(be)?;
            conn.execute(
                braincrawl_sql::node_assertion::MERGE_DELETE_LOSER,
                params![loser.0],
            )
            .map_err(be)?;
            // 4. Edge assertions (src direction) before edge row repoint.
            conn.execute(
                braincrawl_sql::edge::MERGE_EA_SRC_UPSERT,
                params![survivor.0, loser.0],
            )
            .map_err(be)?;
            conn.execute(
                braincrawl_sql::edge::MERGE_EA_SRC_DELETE_LOSER,
                params![loser.0],
            )
            .map_err(be)?;
            // 5. Edge src repoint.
            conn.execute(
                braincrawl_sql::edge::MERGE_EDGE_SRC_DELETE_CONFLICTS,
                params![loser.0, survivor.0],
            )
            .map_err(be)?;
            conn.execute(
                braincrawl_sql::edge::MERGE_EDGE_SRC_REPOINT,
                params![survivor.0, loser.0],
            )
            .map_err(be)?;
            // 6. Edge assertions (dst direction).
            conn.execute(
                braincrawl_sql::edge::MERGE_EA_DST_UPSERT,
                params![survivor.0, loser.0],
            )
            .map_err(be)?;
            conn.execute(
                braincrawl_sql::edge::MERGE_EA_DST_DELETE_LOSER,
                params![loser.0],
            )
            .map_err(be)?;
            // 7. Edge dst repoint.
            conn.execute(
                braincrawl_sql::edge::MERGE_EDGE_DST_DELETE_CONFLICTS,
                params![loser.0, survivor.0],
            )
            .map_err(be)?;
            conn.execute(
                braincrawl_sql::edge::MERGE_EDGE_DST_REPOINT,
                params![survivor.0, loser.0],
            )
            .map_err(be)?;
            // 8. Payload merge.
            conn.execute(
                braincrawl_sql::payload::MERGE_DEMOTE_LOSER_CURRENT,
                params![loser.0, survivor.0],
            )
            .map_err(be)?;
            conn.execute(
                braincrawl_sql::payload::MERGE_REPOINT,
                params![survivor.0, loser.0],
            )
            .map_err(be)?;
            conn.execute(braincrawl_sql::payload::MERGE_DELETE_LOSER, params![loser.0])
                .map_err(be)?;
            Ok(())
        })();
        match result {
            Ok(()) => conn.execute_batch("COMMIT;").map_err(be),
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK;");
                Err(e)
            }
        }
    }

    async fn read_node(
        &self,
        id: &CanonicalId,
    ) -> Result<
        Option<(NodeKind, Vec<(String, serde_json::Value, String)>, Vec<Alias>)>,
        DomainError,
    > {
        let conn = self.conn.lock().unwrap();

        let (kind_s, merged_into): (String, Option<String>) = match conn.query_row(
            braincrawl_sql::node::SELECT,
            params![id.0],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        ) {
            Err(e) if is_no_rows(&e) => return Ok(None),
            Err(e) => return Err(be(e)),
            Ok(v) => v,
        };
        if merged_into.is_some() {
            return Ok(None);
        }
        let kind = parse_node_kind(&kind_s)?;

        let mut stmt = conn
            .prepare(braincrawl_sql::node_assertion::SELECT_BY_NODE)
            .map_err(be)?;
        let assertions: Vec<(String, serde_json::Value, String)> = stmt
            .query_map(params![id.0], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(be)?
            .filter_map(|r| r.ok())
            .map(|(source, attrs_s, fetched_at)| {
                let attrs = serde_json::from_str(&attrs_s)
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                (source, attrs, fetched_at)
            })
            .collect();

        let mut stmt = conn.prepare(braincrawl_sql::alias::LIST_BY_NODE).map_err(be)?;
        let aliases: Vec<Alias> = stmt
            .query_map(params![id.0], |row| {
                Ok(Alias {
                    namespace: row.get(0)?,
                    value: row.get(1)?,
                })
            })
            .map_err(be)?
            .filter_map(|r| r.ok())
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
        let conn = self.conn.lock().unwrap();
        conn.execute(
            braincrawl_sql::edge::INSERT_IGNORE,
            params![src.0, dst.0, relation],
        )
        .map_err(be)?;
        let attrs_s: Option<String> = match attrs {
            Some(v) => {
                Some(serde_json::to_string(v).map_err(|e| DomainError::Serde(e.to_string()))?)
            }
            None => None,
        };
        conn.execute(
            braincrawl_sql::edge_assertion::UPSERT,
            params![src.0, dst.0, relation, source, attrs_s, fetched_at],
        )
        .map_err(be)?;
        Ok(())
    }

    async fn read_edges(
        &self,
        id: &CanonicalId,
        dir: EdgeDir,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<EdgeView>, Option<String>), DomainError> {
        let conn = self.conn.lock().unwrap();

        // Cursor: JSON {"a": "last_other_id", "r": "last_relation"}
        let cursor_decoded: Option<(String, String)> = cursor.and_then(|c| {
            let v: serde_json::Value = serde_json::from_str(c).ok()?;
            Some((v["a"].as_str()?.to_string(), v["r"].as_str()?.to_string()))
        });

        let fetch_limit = limit + 1;

        // Collect edge rows via explicit while-let to avoid borrow-checker issues with
        // `?` on iterators that borrow a local Statement inside match arms.
        let rows: Vec<(String, String, String)> = {
            let sql = match (&dir, cursor_decoded.is_some()) {
                (EdgeDir::Forward, false) => braincrawl_sql::edge::SELECT_FORWARD_FIRST,
                (EdgeDir::Forward, true) => braincrawl_sql::edge::SELECT_FORWARD_PAGE,
                (EdgeDir::Backward, false) => braincrawl_sql::edge::SELECT_BACKWARD_FIRST,
                (EdgeDir::Backward, true) => braincrawl_sql::edge::SELECT_BACKWARD_PAGE,
            };
            let mut st = conn.prepare(sql).map_err(be)?;
            let mut db_rows = match (&dir, &cursor_decoded) {
                (EdgeDir::Forward, None) => {
                    st.query(params![id.0, fetch_limit]).map_err(be)?
                }
                (EdgeDir::Forward, Some((last_dst, last_rel))) => {
                    st.query(params![id.0, last_dst, last_dst, last_rel, fetch_limit])
                        .map_err(be)?
                }
                (EdgeDir::Backward, None) => {
                    st.query(params![id.0, fetch_limit]).map_err(be)?
                }
                (EdgeDir::Backward, Some((last_src, last_rel))) => {
                    st.query(params![id.0, last_src, last_src, last_rel, fetch_limit])
                        .map_err(be)?
                }
            };
            let mut out = Vec::new();
            while let Some(row) = db_rows.next().map_err(be)? {
                out.push((
                    row.get::<_, String>(0).map_err(be)?,
                    row.get::<_, String>(1).map_err(be)?,
                    row.get::<_, String>(2).map_err(be)?,
                ));
            }
            out
        };

        let has_more = rows.len() > limit as usize;
        let page: Vec<(String, String, String)> = rows.into_iter().take(limit as usize).collect();

        let mut views = Vec::with_capacity(page.len());
        for (src_id, dst_id, relation) in &page {
            let mut st = conn
                .prepare(braincrawl_sql::edge_assertion::SELECT_BY_EDGE)
                .map_err(be)?;
            let assertions: Vec<serde_json::Value> = st
                .query_map(params![src_id, dst_id, relation], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(be)?
                .filter_map(|r| r.ok())
                .map(|(source, attrs_opt, fetched_at)| {
                    let mut obj = serde_json::Map::new();
                    obj.insert("source".into(), serde_json::Value::String(source));
                    obj.insert(
                        "fetched_at".into(),
                        serde_json::Value::String(fetched_at),
                    );
                    if let Some(s) = attrs_opt {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                            if !v.is_null() {
                                obj.insert("attrs".into(), v);
                            }
                        }
                    }
                    serde_json::Value::Object(obj)
                })
                .collect();
            views.push(EdgeView {
                src: CanonicalId(src_id.clone()),
                dst: CanonicalId(dst_id.clone()),
                relation: relation.clone(),
                assertions,
            });
        }

        let next_cursor = if has_more {
            page.last().map(|(src_id, dst_id, relation)| {
                let other = match dir {
                    EdgeDir::Forward => dst_id,
                    EdgeDir::Backward => src_id,
                };
                serde_json::json!({"a": other, "r": relation}).to_string()
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
        let conn = self.conn.lock().unwrap();
        let pair_list = braincrawl_sql::alias_pair_list(aliases.len());
        let query = format!(
            "SELECT namespace, value FROM alias WHERE (namespace, value) IN {pair_list}"
        );
        let mut stmt = conn.prepare(&query).map_err(be)?;
        let params_flat: Vec<String> = aliases
            .iter()
            .flat_map(|a| [a.namespace.clone(), a.value.clone()])
            .collect();
        let result: Vec<Alias> = stmt
            .query_map(rusqlite::params_from_iter(params_flat.iter()), |row| {
                Ok(Alias {
                    namespace: row.get(0)?,
                    value: row.get(1)?,
                })
            })
            .map_err(be)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(result)
    }

    async fn stats(&self) -> Result<GraphStats, DomainError> {
        let conn = self.conn.lock().unwrap();

        // Scalar count helper — runs a single COUNT(*) query.
        let count = |sql: &str| -> Result<u64, DomainError> {
            let n: i64 = conn.query_row(sql, [], |row| row.get(0)).map_err(be)?;
            Ok(n as u64)
        };

        // Grouped tally helper — `SELECT <key>, COUNT(*) ... GROUP BY <key>`,
        // already ordered count desc then key asc for determinism.
        let tally = |sql: &str| -> Result<Vec<Tally>, DomainError> {
            let mut stmt = conn.prepare(sql).map_err(be)?;
            let rows: Vec<Tally> = stmt
                .query_map([], |row| {
                    Ok(Tally {
                        key: row.get::<_, String>(0)?,
                        count: row.get::<_, i64>(1)? as u64,
                    })
                })
                .map_err(be)?
                .filter_map(|r| r.ok())
                .collect();
            Ok(rows)
        };

        use braincrawl_sql::stats as q;
        let works = count(q::WORKS)?;
        let works_described = count(q::WORKS_DESCRIBED)?;
        let nodes_total = count(q::NODES_TOTAL)?;
        let tombstones = count(q::TOMBSTONES)?;
        let edges_total = count(q::EDGES_TOTAL)?;

        let nodes_by_kind = tally(q::NODES_BY_KIND)?;
        let edges_by_relation = tally(q::EDGES_BY_RELATION)?;
        let assertions_by_source = tally(q::ASSERTIONS_BY_SOURCE)?;

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
        })
    }
}

// ─── JobEnqueuer ──────────────────────────────────────────────────────────────

#[async_trait(?Send)]
impl JobEnqueuer for SqliteStore {
    async fn enqueue(&self, spec: JobSpec) -> Result<JobId, DomainError> {
        let conn = self.conn.lock().unwrap();
        let kind_s = spec.kind.as_str();
        let params_s = serde_json::to_string(&spec.params)
            .map_err(|e| DomainError::Serde(e.to_string()))?;
        let now = now_rfc3339();
        let candidate_id = uuid::Uuid::new_v4().to_string();

        conn.execute(
            braincrawl_sql::job::ENQUEUE_INSERT,
            params![candidate_id, kind_s, spec.target_id, params_s, now, now, now],
        )
        .map_err(be)?;

        let id: Option<String> = conn
            .query_row(
                braincrawl_sql::job::ENQUEUE_SELECT,
                params![kind_s, spec.target_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(be)?;

        match id {
            Some(id) => Ok(JobId(id)),
            None => Err(DomainError::Backend(
                "enqueue: no active job found after insert".to_string(),
            )),
        }
    }
}

// ─── JobQueue ─────────────────────────────────────────────────────────────────

#[async_trait(?Send)]
impl JobQueue for SqliteStore {
    async fn claim(&self, limit: u32, now: &str) -> Result<Vec<Job>, DomainError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(braincrawl_sql::job::CLAIM).map_err(be)?;
        let rows: Vec<Result<(String, String, String, String, i64), rusqlite::Error>> = stmt
            .query_map(params![now, now, limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })
            .map_err(be)?
            .collect();

        let mut jobs = Vec::with_capacity(rows.len());
        for row in rows {
            let (id, kind_s, target_id, params_s, attempts) = row.map_err(be)?;
            let kind = kind_s.parse::<JobKind>()?;
            let params: serde_json::Value = serde_json::from_str(&params_s)
                .map_err(|e| DomainError::Serde(e.to_string()))?;
            jobs.push(Job {
                id: JobId(id),
                kind,
                target_id,
                params,
                attempts: attempts as u32,
            });
        }
        Ok(jobs)
    }

    async fn complete(&self, id: &JobId) -> Result<(), DomainError> {
        let conn = self.conn.lock().unwrap();
        let now = now_rfc3339();
        conn.execute(braincrawl_sql::job::COMPLETE, params![now, id.0])
            .map_err(be)?;
        Ok(())
    }

    async fn retry(&self, id: &JobId, run_after: &str, err: &str) -> Result<(), DomainError> {
        let conn = self.conn.lock().unwrap();
        let now = now_rfc3339();
        conn.execute(
            braincrawl_sql::job::RETRY,
            params![run_after, err, now, id.0],
        )
        .map_err(be)?;
        Ok(())
    }

    async fn fail(&self, id: &JobId, err: &str) -> Result<(), DomainError> {
        let conn = self.conn.lock().unwrap();
        let now = now_rfc3339();
        conn.execute(braincrawl_sql::job::FAIL, params![err, now, id.0])
            .map_err(be)?;
        Ok(())
    }
}

fn now_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3_600) % 24;
    let days = secs / 86_400;
    let (year, month, day) = sqlite_days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

fn sqlite_days_to_ymd(mut days: u64) -> (u32, u32, u32) {
    let mut year = 1970u32;
    loop {
        let dy = if sqlite_is_leap(year) { 366 } else { 365 };
        if days < dy {
            break;
        }
        days -= dy;
        year += 1;
    }
    let months: [u32; 12] = if sqlite_is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u32;
    for &dm in &months {
        if days < dm as u64 {
            break;
        }
        days -= dm as u64;
        month += 1;
    }
    (year, month, days as u32 + 1)
}

fn sqlite_is_leap(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}
