//! `ArtifactStore` + `MetadataStore` backed by local SQLite via rusqlite.
//!
//! Uses `std::sync::Mutex<Connection>` so that `SqliteStore: Send + Sync`.
//! On a single-threaded runtime the mutex is never actually contended.
//! Migrations are applied idempotently on construction via an internal `_migrations` table.

#![allow(dead_code)]

use std::sync::Mutex;

use async_trait::async_trait;
use braincrawl_core::{
    traits::{JobEnqueuer, JobQueue, MetadataStore, ArtifactStore},
    types::{
        Alias, CanonicalId, DomainError, EdgeDir, EdgeView, ExportAssertion,
        ExportEdgeAssertion, ExportNode, GraphStats, Job, JobId, JobKind, JobSpec, NodeKind,
        Artifact, ArtifactRole, Tally, WorkSearchFilter,
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

fn artifact_role_str(k: &ArtifactRole) -> &str {
    k.as_str()
}

fn parse_artifact_role(s: &str) -> Result<ArtifactRole, DomainError> {
    ArtifactRole::parse(s)
        .ok_or_else(|| DomainError::Backend(format!("unknown ArtifactRole: {s}")))
}

fn be(e: rusqlite::Error) -> DomainError {
    DomainError::Backend(e.to_string())
}

fn is_no_rows(e: &rusqlite::Error) -> bool {
    matches!(e, rusqlite::Error::QueryReturnedNoRows)
}

/// Map a row in `SELECT_CURRENT`/`LIST_*` column order to an `Artifact`.
fn row_to_artifact(row: &rusqlite::Row) -> rusqlite::Result<Artifact> {
    let canonical_id: String = row.get(0)?;
    let role: String = row.get(1)?;
    let version: i64 = row.get(2)?;
    let r2_key: String = row.get(3)?;
    let content_hash: String = row.get(4)?;
    let byte_size: i64 = row.get(5)?;
    let mime: String = row.get(6)?;
    let source: Option<String> = row.get(7)?;
    let source_url: Option<String> = row.get(8)?;
    let fetched_at: String = row.get(9)?;
    let is_current: i32 = row.get(10)?;
    let derived_from_role: Option<String> = row.get(11)?;
    let derived_from_version: Option<i64> = row.get(12)?;
    let derived_from = derived_from_role.and_then(|r| {
        derived_from_version.map(|v| (parse_artifact_role(&r).unwrap_or(ArtifactRole::Abstract), v as u32))
    });
    Ok(Artifact {
        canonical_id: CanonicalId(canonical_id),
        role: parse_artifact_role(&role).unwrap_or(ArtifactRole::Abstract),
        version: version as u32,
        r2_key,
        content_hash,
        byte_size: byte_size as u64,
        mime,
        source,
        source_url,
        fetched_at,
        is_current: is_current != 0,
        derived_from,
    })
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

/// SQLite-backed `MetadataStore` + `ArtifactStore`.
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

// ─── ArtifactStore ─────────────────────────────────────────────────────────────

#[async_trait(?Send)]
impl ArtifactStore for SqliteStore {
    async fn current_artifact(
        &self,
        id: &CanonicalId,
        kind: ArtifactRole,
    ) -> Result<Option<Artifact>, DomainError> {
        let conn = self.conn.lock().unwrap();
        let ks = artifact_role_str(&kind);
        conn.query_row(
            braincrawl_sql::artifact::SELECT_CURRENT,
            params![id.0, ks],
            row_to_artifact,
        )
        .map(Some)
        .or_else(|e| if is_no_rows(&e) { Ok(None) } else { Err(be(e)) })
    }

    async fn next_version(&self, id: &CanonicalId, kind: ArtifactRole) -> Result<u32, DomainError> {
        let conn = self.conn.lock().unwrap();
        let ks = artifact_role_str(&kind);
        let v: i64 = conn
            .query_row(
                braincrawl_sql::artifact::NEXT_VERSION,
                params![id.0, ks],
                |row| row.get(0),
            )
            .map_err(be)?;
        Ok(v as u32)
    }

    async fn record(&self, d: &Artifact) -> Result<(), DomainError> {
        let conn = self.conn.lock().unwrap();
        let ks = artifact_role_str(&d.role);
        if d.is_current {
            conn.execute(
                braincrawl_sql::artifact::FLIP_CURRENT_OFF,
                params![d.canonical_id.0, ks],
            )
            .map_err(be)?;
        }
        let (derived_from_role, derived_from_version): (Option<String>, Option<i64>) =
            match &d.derived_from {
                Some((role, version)) => (Some(role.as_str().to_string()), Some(*version as i64)),
                None => (None, None),
            };
        conn.execute(
            braincrawl_sql::artifact::INSERT,
            params![
                d.canonical_id.0,
                ks,
                d.version as i64,
                d.r2_key,
                d.content_hash,
                d.byte_size as i64,
                d.mime,
                d.source,
                d.source_url,
                d.fetched_at,
                if d.is_current { 1i32 } else { 0 },
                derived_from_role,
                derived_from_version,
            ],
        )
        .map_err(be)?;
        Ok(())
    }

    async fn list_artifacts(
        &self,
        id: &CanonicalId,
        role: Option<ArtifactRole>,
        all_versions: bool,
    ) -> Result<Vec<Artifact>, DomainError> {
        let conn = self.conn.lock().unwrap();
        let rows: Vec<Artifact> = match &role {
            Some(r) => {
                let ks = artifact_role_str(r);
                let sql = if all_versions {
                    braincrawl_sql::artifact::LIST_ALL_VERSIONS_BY_ROLE
                } else {
                    braincrawl_sql::artifact::LIST_CURRENT_BY_ROLE
                };
                let mut stmt = conn.prepare(sql).map_err(be)?;
                let mapped = stmt
                    .query_map(params![id.0, ks], row_to_artifact)
                    .map_err(be)?
                    .collect::<Result<_, _>>()
                    .map_err(be)?;
                mapped
            }
            None => {
                let sql = if all_versions {
                    braincrawl_sql::artifact::LIST_ALL_VERSIONS
                } else {
                    braincrawl_sql::artifact::LIST_CURRENT
                };
                let mut stmt = conn.prepare(sql).map_err(be)?;
                let mapped = stmt
                    .query_map(params![id.0], row_to_artifact)
                    .map_err(be)?
                    .collect::<Result<_, _>>()
                    .map_err(be)?;
                mapped
            }
        };
        Ok(rows)
    }

    async fn export_artifacts(
        &self,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<Artifact>, Option<String>), DomainError> {
        let conn = self.conn.lock().unwrap();
        // Cursor: JSON {"c": last_canonical_id, "r": last_role}
        let decoded: Option<(String, String)> = cursor.and_then(|c| {
            let v: serde_json::Value = serde_json::from_str(c).ok()?;
            Some((v["c"].as_str()?.to_string(), v["r"].as_str()?.to_string()))
        });
        let fetch_limit = (limit + 1) as i64;
        let rows: Vec<Artifact> = match &decoded {
            None => {
                let mut stmt = conn.prepare(braincrawl_sql::export::ARTIFACTS_FIRST).map_err(be)?;
                let mapped: Vec<Artifact> = stmt
                    .query_map(params![fetch_limit], row_to_artifact)
                    .map_err(be)?
                    .collect::<Result<_, _>>()
                    .map_err(be)?;
                mapped
            }
            Some((last_id, last_role)) => {
                let mut stmt = conn.prepare(braincrawl_sql::export::ARTIFACTS_PAGE).map_err(be)?;
                let mapped: Vec<Artifact> = stmt
                    .query_map(params![last_id, last_role, fetch_limit], row_to_artifact)
                    .map_err(be)?
                    .collect::<Result<_, _>>()
                    .map_err(be)?;
                mapped
            }
        };
        let has_more = rows.len() > limit as usize;
        let page: Vec<Artifact> = rows.into_iter().take(limit as usize).collect();
        let next_cursor = if has_more {
            page.last().map(|a| {
                serde_json::json!({"c": a.canonical_id.0, "r": a.role.as_str()}).to_string()
            })
        } else {
            None
        };
        Ok((page, next_cursor))
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

    async fn create_node(
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
            // 8. Artifact merge.
            conn.execute(
                braincrawl_sql::artifact::MERGE_DEMOTE_LOSER_CURRENT,
                params![loser.0, survivor.0],
            )
            .map_err(be)?;
            conn.execute(
                braincrawl_sql::artifact::MERGE_REPOINT,
                params![survivor.0, loser.0],
            )
            .map_err(be)?;
            conn.execute(braincrawl_sql::artifact::MERGE_DELETE_LOSER, params![loser.0])
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

    async fn search_work_ids(
        &self,
        filter: &WorkSearchFilter,
        limit: u32,
    ) -> Result<Vec<CanonicalId>, DomainError> {
        let conn = self.conn.lock().unwrap();
        let sql = braincrawl_sql::search::works(
            filter.author.is_some(),
            filter.title.is_some(),
            filter.year.is_some(),
        );
        let mut params: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(a) = &filter.author {
            params.push(a.clone().into());
        }
        if let Some(t) = &filter.title {
            params.push(t.clone().into());
            params.push(t.clone().into());
        }
        if let Some(y) = filter.year {
            params.push((y as i64).into());
        }
        params.push((limit as i64).into());
        let mut stmt = conn.prepare(&sql).map_err(be)?;
        let ids: Vec<CanonicalId> = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), |row| {
                Ok(CanonicalId(row.get::<_, String>(0)?))
            })
            .map_err(be)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(ids)
    }

    async fn present_aliases(&self, aliases: &[Alias]) -> Result<Vec<Alias>, DomainError> {
        if aliases.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn.lock().unwrap();
        // Chunked to match the D1 backend's bound-parameter ceiling (parity of shape;
        // native SQLite's own limit is far higher).
        const PAIRS_PER_CHUNK: usize = 45;
        let mut out = Vec::new();
        for chunk in aliases.chunks(PAIRS_PER_CHUNK) {
            let pair_list = braincrawl_sql::alias_pair_list(chunk.len());
            let query = format!(
                "SELECT namespace, value FROM alias WHERE (namespace, value) IN {pair_list}"
            );
            let mut stmt = conn.prepare(&query).map_err(be)?;
            let params_flat: Vec<String> = chunk
                .iter()
                .flat_map(|a| [a.namespace.clone(), a.value.clone()])
                .collect();
            let chunk_rows: Vec<Alias> = stmt
                .query_map(rusqlite::params_from_iter(params_flat.iter()), |row| {
                    Ok(Alias {
                        namespace: row.get(0)?,
                        value: row.get(1)?,
                    })
                })
                .map_err(be)?
                .filter_map(|r| r.ok())
                .collect();
            out.extend(chunk_rows);
        }
        Ok(out)
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

        let library_bytes = count(q::LIBRARY_BYTES)?;
        // Real on-disk size of the SQLite file (data + indexes + free pages).
        let catalog_bytes = count("PRAGMA page_count")? * count("PRAGMA page_size")?;

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
            catalog_bytes,
            total_bytes: library_bytes + catalog_bytes,
        })
    }

    async fn applied_migrations(&self) -> Result<Vec<String>, DomainError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT name FROM _migrations ORDER BY name")
            .map_err(be)?;
        let rows: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(be)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    async fn export_nodes(
        &self,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<ExportNode>, Option<String>), DomainError> {
        let conn = self.conn.lock().unwrap();
        let fetch_limit = (limit + 1) as i64;
        // Cursor: the last canonical_id, verbatim.
        let node_rows: Vec<(String, String)> = match cursor {
            None => {
                let mut stmt = conn.prepare(braincrawl_sql::export::NODES_FIRST).map_err(be)?;
                let mapped: Vec<(String, String)> = stmt
                    .query_map(params![fetch_limit], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })
                    .map_err(be)?
                    .collect::<Result<_, _>>()
                    .map_err(be)?;
                mapped
            }
            Some(last_id) => {
                let mut stmt = conn.prepare(braincrawl_sql::export::NODES_PAGE).map_err(be)?;
                let mapped: Vec<(String, String)> = stmt
                    .query_map(params![last_id, fetch_limit], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })
                    .map_err(be)?
                    .collect::<Result<_, _>>()
                    .map_err(be)?;
                mapped
            }
        };
        let has_more = node_rows.len() > limit as usize;
        let page: Vec<(String, String)> = node_rows.into_iter().take(limit as usize).collect();
        if page.is_empty() {
            return Ok((Vec::new(), None));
        }

        // Aliases and assertions for the whole page, chunked to stay under the
        // D1-parity bound-parameter ceiling (shape parity with present_aliases).
        const IDS_PER_CHUNK: usize = 45;
        let ids: Vec<&String> = page.iter().map(|(id, _)| id).collect();
        let mut aliases_by_node: std::collections::HashMap<String, Vec<Alias>> = Default::default();
        let mut assertions_by_node: std::collections::HashMap<String, Vec<ExportAssertion>> =
            Default::default();
        for chunk in ids.chunks(IDS_PER_CHUNK) {
            let alias_q = braincrawl_sql::export::aliases_for_nodes(chunk.len());
            let mut stmt = conn.prepare(&alias_q).map_err(be)?;
            let rows: Vec<(String, String, String)> = stmt
                .query_map(rusqlite::params_from_iter(chunk.iter()), |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .map_err(be)?
                .collect::<Result<_, _>>()
                .map_err(be)?;
            for (cid, namespace, value) in rows {
                aliases_by_node.entry(cid).or_default().push(Alias { namespace, value });
            }

            let assn_q = braincrawl_sql::export::assertions_for_nodes(chunk.len());
            let mut stmt = conn.prepare(&assn_q).map_err(be)?;
            let rows: Vec<(String, String, String, String)> = stmt
                .query_map(rusqlite::params_from_iter(chunk.iter()), |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })
                .map_err(be)?
                .collect::<Result<_, _>>()
                .map_err(be)?;
            for (cid, source, attrs_s, fetched_at) in rows {
                let attrs = serde_json::from_str(&attrs_s)
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                assertions_by_node
                    .entry(cid)
                    .or_default()
                    .push(ExportAssertion { source, attrs, fetched_at });
            }
        }

        let nodes: Vec<ExportNode> = page
            .iter()
            .map(|(id, kind_s)| {
                Ok(ExportNode {
                    canonical_id: CanonicalId(id.clone()),
                    kind: parse_node_kind(kind_s)?,
                    aliases: aliases_by_node.remove(id).unwrap_or_default(),
                    assertions: assertions_by_node.remove(id).unwrap_or_default(),
                })
            })
            .collect::<Result<_, DomainError>>()?;

        let next_cursor = if has_more {
            page.last().map(|(id, _)| id.clone())
        } else {
            None
        };
        Ok((nodes, next_cursor))
    }

    async fn export_edge_assertions(
        &self,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<ExportEdgeAssertion>, Option<String>), DomainError> {
        let conn = self.conn.lock().unwrap();
        // Cursor: JSON {"s": src, "d": dst, "r": relation, "o": source}
        let decoded: Option<(String, String, String, String)> = cursor.and_then(|c| {
            let v: serde_json::Value = serde_json::from_str(c).ok()?;
            Some((
                v["s"].as_str()?.to_string(),
                v["d"].as_str()?.to_string(),
                v["r"].as_str()?.to_string(),
                v["o"].as_str()?.to_string(),
            ))
        });
        let fetch_limit = (limit + 1) as i64;
        type Row = (String, String, String, String, Option<String>, String);
        let rows: Vec<Row> = match &decoded {
            None => {
                let mut stmt =
                    conn.prepare(braincrawl_sql::export::EDGE_ASSERTIONS_FIRST).map_err(be)?;
                let mapped: Vec<Row> = stmt
                    .query_map(params![fetch_limit], |row| {
                        Ok((
                            row.get(0)?, row.get(1)?, row.get(2)?,
                            row.get(3)?, row.get(4)?, row.get(5)?,
                        ))
                    })
                    .map_err(be)?
                    .collect::<Result<_, _>>()
                    .map_err(be)?;
                mapped
            }
            Some((s0, d0, r0, o0)) => {
                let mut stmt =
                    conn.prepare(braincrawl_sql::export::EDGE_ASSERTIONS_PAGE).map_err(be)?;
                let mapped: Vec<Row> = stmt
                    .query_map(params![s0, d0, r0, o0, fetch_limit], |row| {
                        Ok((
                            row.get(0)?, row.get(1)?, row.get(2)?,
                            row.get(3)?, row.get(4)?, row.get(5)?,
                        ))
                    })
                    .map_err(be)?
                    .collect::<Result<_, _>>()
                    .map_err(be)?;
                mapped
            }
        };
        let has_more = rows.len() > limit as usize;
        let page: Vec<Row> = rows.into_iter().take(limit as usize).collect();
        let next_cursor = if has_more {
            page.last().map(|(s, d, r, o, _, _)| {
                serde_json::json!({"s": s, "d": d, "r": r, "o": o}).to_string()
            })
        } else {
            None
        };
        let items = page
            .into_iter()
            .map(|(src, dst, relation, source, attrs_s, fetched_at)| ExportEdgeAssertion {
                src_id: CanonicalId(src),
                dst_id: CanonicalId(dst),
                relation,
                source,
                attrs: attrs_s.and_then(|s| serde_json::from_str(&s).ok()),
                fetched_at,
            })
            .collect();
        Ok((items, next_cursor))
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
