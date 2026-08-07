//! CLI verb: replay the local SQLite + blob corpus into a remote store's HTTP API.
//!
//! There is no enumeration surface over HTTP (no list-all route, no trait method),
//! so this module reads the local database directly with raw SQL — kept out of the
//! `MetadataStore`/`BlobStore` traits, which stay enumeration-free. Replay itself goes
//! exclusively through the public HTTP API (`PUT /works`, `PUT /edges`,
//! `PUT .../content/{role}`), so the remote's own alias-derived identity resolution
//! does the merging. Idempotent by construction: re-running is safe.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use rusqlite::{Connection, OpenFlags};

use crate::provider::{Alias as EmitAlias, EdgeInput, WorkRecord};
use crate::store_client::{ContentOutcome, StoreClient};

/// Max ids per `POST /works/have` request.
const HAVE_BATCH: usize = 500;
/// Max edges per `PUT /edges` request.
const EDGE_BATCH: usize = 200;

#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    #[error("open local database at {path}: {source}")]
    OpenDb { path: PathBuf, source: rusqlite::Error },
    #[error("local database query failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Client(#[from] crate::store_client::ClientError),
    #[error("serialize failed: {0}")]
    Serde(#[from] serde_json::Error),
}

pub struct MigrateOpts {
    pub db_path: PathBuf,
    pub blob_root: PathBuf,
    pub dry_run: bool,
}

impl MigrateOpts {
    pub fn resolve(db: Option<String>, blobs: Option<String>, dry_run: bool) -> Self {
        MigrateOpts {
            db_path: db.map(PathBuf::from).unwrap_or_else(default_db_path),
            blob_root: blobs.map(PathBuf::from).unwrap_or_else(default_blob_root),
            dry_run,
        }
    }
}

fn home_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
}

pub fn default_db_path() -> PathBuf {
    home_dir().join(".local/share/braincrawl/braincrawl.db")
}

pub fn default_blob_root() -> PathBuf {
    home_dir().join(".local/share/braincrawl/blobs")
}

/// Per-category counts from a migration run (or dry-run plan).
#[derive(Debug, Default)]
pub struct MigrateReport {
    pub works_total: usize,
    pub works_pushed: usize,
    pub works_skipped_present: usize,
    pub works_skipped_no_alias: usize,
    pub edges_total: usize,
    pub edges_pushed: u64,
    pub edges_skipped_no_alias: usize,
    pub artifacts_total: usize,
    pub artifacts_pushed: usize,
    pub artifacts_skipped_present: usize,
    pub artifacts_skipped_no_alias: usize,
    pub artifacts_missing_blob: usize,
    pub artifacts_errors: usize,
    pub errors: Vec<String>,
}

pub fn print_report(report: &MigrateReport, dry_run: bool) {
    let label = if dry_run { "plan" } else { "migrated" };
    eprintln!(
        "works ({label}): {} pushed, {} already-present, {} no-alias (of {} live nodes)",
        report.works_pushed, report.works_skipped_present, report.works_skipped_no_alias, report.works_total
    );
    eprintln!(
        "edges ({label}): {} pushed, {} no-alias (of {} edge assertions)",
        report.edges_pushed, report.edges_skipped_no_alias, report.edges_total
    );
    eprintln!(
        "artifacts ({label}): {} pushed, {} already-present, {} no-alias, {} missing-blob, {} errors (of {} current artifacts)",
        report.artifacts_pushed,
        report.artifacts_skipped_present,
        report.artifacts_skipped_no_alias,
        report.artifacts_missing_blob,
        report.artifacts_errors,
        report.artifacts_total,
    );
    for e in &report.errors {
        eprintln!("migrate error: {e}");
    }
}

// ─── local row shapes ───────────────────────────────────────────────────────────

struct LocalNode {
    canonical_id: String,
    kind: String,
}

/// No `fetched_at` field: `put_work` always stamps assertions with the remote's own
/// "now" (`Store::put_work` → `clock.now_rfc3339()`), so the local timestamp has
/// nowhere to go over the HTTP replay path.
struct LocalAssertion {
    source: String,
    attrs: serde_json::Value,
}

struct LocalEdgeAssertion {
    src_id: String,
    dst_id: String,
    relation: String,
    source: String,
    attrs: Option<serde_json::Value>,
    fetched_at: String,
}

struct LocalArtifact {
    canonical_id: String,
    role: String,
    r2_key: String,
    mime: String,
    source: Option<String>,
    source_url: Option<String>,
    fetched_at: String,
}

// ─── enumeration (direct SQLite) ─────────────────────────────────────────────────

fn open_readonly(path: &std::path::Path) -> Result<Connection, MigrateError> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|source| MigrateError::OpenDb { path: path.to_path_buf(), source })
}

fn load_live_nodes(conn: &Connection) -> Result<Vec<LocalNode>, MigrateError> {
    let mut stmt = conn.prepare("SELECT canonical_id, kind FROM node WHERE merged_into IS NULL")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(LocalNode { canonical_id: row.get(0)?, kind: row.get(1)? })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn load_aliases(conn: &Connection) -> Result<HashMap<String, Vec<EmitAlias>>, MigrateError> {
    let mut stmt = conn.prepare("SELECT canonical_id, namespace, value FROM alias")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })?
        .collect::<Result<Vec<_>, rusqlite::Error>>()?;
    let mut map: HashMap<String, Vec<EmitAlias>> = HashMap::new();
    for (canonical_id, namespace, value) in rows {
        map.entry(canonical_id).or_default().push(EmitAlias { namespace, value });
    }
    Ok(map)
}

fn load_assertions(conn: &Connection) -> Result<HashMap<String, Vec<LocalAssertion>>, MigrateError> {
    let mut stmt = conn.prepare("SELECT canonical_id, source, attrs FROM node_assertion")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })?
        .collect::<Result<Vec<_>, rusqlite::Error>>()?;
    let mut map: HashMap<String, Vec<LocalAssertion>> = HashMap::new();
    for (canonical_id, source, attrs_s) in rows {
        let attrs: serde_json::Value =
            serde_json::from_str(&attrs_s).unwrap_or(serde_json::Value::Object(Default::default()));
        map.entry(canonical_id).or_default().push(LocalAssertion { source, attrs });
    }
    Ok(map)
}

/// `fetched_at` isn't in the plan's suggested column list, but `edge_assertion.fetched_at`
/// is `NOT NULL` and `EdgeInput` requires it — selecting it is necessary, not optional.
fn load_edge_assertions(conn: &Connection) -> Result<Vec<LocalEdgeAssertion>, MigrateError> {
    let mut stmt = conn.prepare(
        "SELECT src_id, dst_id, relation, source, attrs, fetched_at FROM edge_assertion",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, rusqlite::Error>>()?;
    Ok(rows
        .into_iter()
        .map(|(src_id, dst_id, relation, source, attrs_s, fetched_at)| LocalEdgeAssertion {
            src_id,
            dst_id,
            relation,
            source,
            attrs: attrs_s.and_then(|s| serde_json::from_str(&s).ok()),
            fetched_at,
        })
        .collect())
}

fn load_current_artifacts(conn: &Connection) -> Result<Vec<LocalArtifact>, MigrateError> {
    let mut stmt = conn.prepare(
        "SELECT canonical_id, role, r2_key, mime, source, source_url, fetched_at \
         FROM artifacts WHERE is_current = 1",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, rusqlite::Error>>()?;
    Ok(rows
        .into_iter()
        .map(
            |(canonical_id, role, r2_key, mime, source, source_url, fetched_at)| LocalArtifact {
                canonical_id,
                role,
                r2_key,
                mime,
                source,
                source_url,
                fetched_at,
            },
        )
        .collect())
}

// ─── pure mapping (row → wire shape) ─────────────────────────────────────────────

fn map_node_kind(local_kind: &str) -> Option<&'static str> {
    match local_kind {
        "work" => Some("Work"),
        "author" => Some("Author"),
        "venue" => Some("Venue"),
        "concept" => Some("Concept"),
        "topic" => Some("Topic"),
        _ => None,
    }
}

/// Build one `WorkRecord` per assertion for a node. A node with zero assertions but
/// live aliases gets a single empty-attrs record so its aliases still register —
/// mirroring how a stub node enters via `resolve_or_create_stub` on the normal path.
fn work_records_for_node(kind: &str, aliases: &[EmitAlias], assertions: &[LocalAssertion]) -> Vec<WorkRecord> {
    if assertions.is_empty() {
        return vec![WorkRecord {
            source: "migration".to_string(),
            kind: kind.to_string(),
            aliases: aliases.to_vec(),
            attrs: serde_json::json!({}),
        }];
    }
    assertions
        .iter()
        .map(|a| WorkRecord {
            source: a.source.clone(),
            kind: kind.to_string(),
            aliases: aliases.to_vec(),
            attrs: a.attrs.clone(),
        })
        .collect()
}

const ALIAS_PREFERENCE: [&str; 2] = ["openalex", "doi"];

/// Pick the alias used to address a node over HTTP: `openalex`, then `doi`, else
/// whichever alias comes first.
fn choose_alias(aliases: &[EmitAlias]) -> Option<EmitAlias> {
    for ns in ALIAS_PREFERENCE {
        if let Some(a) = aliases.iter().find(|a| a.namespace == ns) {
            return Some(a.clone());
        }
    }
    aliases.first().cloned()
}

fn edge_input_from_assertion(
    ea: &LocalEdgeAssertion,
    alias_by_node: &HashMap<String, Vec<EmitAlias>>,
) -> Option<EdgeInput> {
    let src = choose_alias(alias_by_node.get(&ea.src_id)?)?;
    let dst = choose_alias(alias_by_node.get(&ea.dst_id)?)?;
    Some(EdgeInput {
        src,
        dst,
        relation: ea.relation.clone(),
        source: ea.source.clone(),
        attrs: ea.attrs.clone().unwrap_or(serde_json::Value::Null),
        fetched_at: ea.fetched_at.clone(),
    })
}

fn alias_key(a: &EmitAlias) -> String {
    format!("{}:{}", a.namespace, a.value)
}

// ─── replay ───────────────────────────────────────────────────────────────────

pub fn run(opts: &MigrateOpts, client: &StoreClient) -> Result<MigrateReport, MigrateError> {
    let conn = open_readonly(&opts.db_path)?;
    let nodes = load_live_nodes(&conn)?;
    let alias_by_node = load_aliases(&conn)?;
    let assertions_by_node = load_assertions(&conn)?;
    let edge_assertions = load_edge_assertions(&conn)?;
    let artifacts = load_current_artifacts(&conn)?;
    drop(conn);

    let mut report = MigrateReport {
        works_total: nodes.len(),
        edges_total: edge_assertions.len(),
        artifacts_total: artifacts.len(),
        ..Default::default()
    };

    let empty_aliases: Vec<EmitAlias> = Vec::new();
    let empty_assertions: Vec<LocalAssertion> = Vec::new();

    // Prefilter: batch-check every known alias so nodes that are already fully
    // present remotely can be skipped without a PUT.
    let all_alias_ids: Vec<String> = alias_by_node.values().flatten().map(alias_key).collect();
    let present_aliases = have_batched(client, &all_alias_ids)?;

    // ---- Works ----
    for node in &nodes {
        let aliases = alias_by_node.get(&node.canonical_id).unwrap_or(&empty_aliases);
        if aliases.is_empty() {
            report.works_skipped_no_alias += 1;
            continue;
        }
        if aliases.iter().all(|a| present_aliases.contains(&alias_key(a))) {
            report.works_skipped_present += 1;
            continue;
        }
        let Some(kind) = map_node_kind(&node.kind) else {
            report.errors.push(format!("{}: unknown node kind '{}'", node.canonical_id, node.kind));
            continue;
        };
        let assertions = assertions_by_node.get(&node.canonical_id).unwrap_or(&empty_assertions);
        let records = work_records_for_node(kind, aliases, assertions);
        if opts.dry_run {
            report.works_pushed += records.len();
            continue;
        }
        for record in records {
            let v = serde_json::to_value(&record)?;
            match client.put_work(&v) {
                Ok(_) => report.works_pushed += 1,
                Err(e) => report.errors.push(format!("put_work {}: {e}", node.canonical_id)),
            }
        }
    }

    // ---- Edges ----
    let mut edge_batch: Vec<serde_json::Value> = Vec::new();
    for ea in &edge_assertions {
        let Some(edge) = edge_input_from_assertion(ea, &alias_by_node) else {
            report.edges_skipped_no_alias += 1;
            continue;
        };
        if opts.dry_run {
            report.edges_pushed += 1;
            continue;
        }
        edge_batch.push(serde_json::to_value(&edge)?);
        if edge_batch.len() >= EDGE_BATCH {
            flush_edges(client, &mut edge_batch, &mut report)?;
        }
    }
    if !edge_batch.is_empty() {
        flush_edges(client, &mut edge_batch, &mut report)?;
    }

    // ---- Artifacts ----
    for art in &artifacts {
        let aliases = alias_by_node.get(&art.canonical_id).unwrap_or(&empty_aliases);
        let Some(alias) = choose_alias(aliases) else {
            report.artifacts_skipped_no_alias += 1;
            continue;
        };
        let alias_str = alias_key(&alias);

        match client.get_content(&alias_str, &art.role) {
            Ok(ContentOutcome::Bytes { .. }) | Ok(ContentOutcome::Pending) => {
                report.artifacts_skipped_present += 1;
                continue;
            }
            Ok(ContentOutcome::Absent) => {}
            Err(e) => {
                report.errors.push(format!("get_content {alias_str}/{}: {e}", art.role));
                continue;
            }
        }

        if opts.dry_run {
            report.artifacts_pushed += 1;
            continue;
        }

        let blob_path = opts.blob_root.join(&art.r2_key);
        let bytes = match std::fs::read(&blob_path) {
            Ok(b) => b,
            Err(_) => {
                report.artifacts_missing_blob += 1;
                continue;
            }
        };

        match client.put_content_with_fetched_at(
            &alias_str,
            &art.role,
            bytes,
            &art.mime,
            art.source.as_deref(),
            art.source_url.as_deref(),
            &art.fetched_at,
            None,
        ) {
            Ok(()) => report.artifacts_pushed += 1,
            Err(e) => {
                report.artifacts_errors += 1;
                report.errors.push(format!("put_content {alias_str}/{}: {e}", art.role));
            }
        }
    }

    Ok(report)
}

fn have_batched(client: &StoreClient, ids: &[String]) -> Result<HashSet<String>, MigrateError> {
    let mut present = HashSet::new();
    for chunk in ids.chunks(HAVE_BATCH) {
        present.extend(client.have(chunk)?);
    }
    Ok(present)
}

fn flush_edges(
    client: &StoreClient,
    batch: &mut Vec<serde_json::Value>,
    report: &mut MigrateReport,
) -> Result<(), MigrateError> {
    match client.put_edges(batch) {
        Ok(n) => report.edges_pushed += n,
        Err(e) => report.errors.push(format!("put_edges batch of {}: {e}", batch.len())),
    }
    batch.clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        for sql in [
            include_str!("../../../migrations/0001_init.sql"),
            include_str!("../../../migrations/0002_graph.sql"),
            include_str!("../../../migrations/0004_rename_payloads_to_artifacts.sql"),
            include_str!("../../../migrations/0005_drop_artifact_rights.sql"),
        ] {
            conn.execute_batch(sql).unwrap();
        }
        conn
    }

    #[test]
    fn enumerates_only_live_nodes() {
        let conn = seeded_conn();
        conn.execute_batch(
            "INSERT INTO node (canonical_id, kind, created_at) VALUES ('guid:1', 'work', 't');
             INSERT INTO node (canonical_id, kind, created_at, merged_into) VALUES ('guid:2', 'work', 't', 'guid:1');",
        )
        .unwrap();

        let nodes = load_live_nodes(&conn).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].canonical_id, "guid:1");
    }

    #[test]
    fn enumerates_aliases_and_assertions_by_node() {
        let conn = seeded_conn();
        conn.execute_batch(
            "INSERT INTO node (canonical_id, kind, created_at) VALUES ('guid:1', 'work', 't');
             INSERT INTO alias (namespace, value, canonical_id) VALUES ('openalex', 'W1', 'guid:1');
             INSERT INTO alias (namespace, value, canonical_id) VALUES ('doi', '10.1/x', 'guid:1');
             INSERT INTO node_assertion (canonical_id, source, attrs, fetched_at) \
                VALUES ('guid:1', 'openalex', '{\"title\":\"t\"}', 'ts');",
        )
        .unwrap();

        let aliases = load_aliases(&conn).unwrap();
        assert_eq!(aliases.get("guid:1").unwrap().len(), 2);

        let assertions = load_assertions(&conn).unwrap();
        let a = &assertions.get("guid:1").unwrap()[0];
        assert_eq!(a.source, "openalex");
        assert_eq!(a.attrs["title"], "t");
    }

    #[test]
    fn enumerates_current_artifacts_only() {
        let conn = seeded_conn();
        conn.execute_batch(
            "INSERT INTO node (canonical_id, kind, created_at) VALUES ('guid:1', 'work', 't');
             INSERT INTO artifacts (canonical_id, role, version, r2_key, content_hash, byte_size, mime, source, source_url, fetched_at, is_current) \
               VALUES ('guid:1', 'fulltext', 1, 'guid:1/fulltext/v1', 'h1', 10, 'application/pdf', 'src', NULL, 't1', 0);
             INSERT INTO artifacts (canonical_id, role, version, r2_key, content_hash, byte_size, mime, source, source_url, fetched_at, is_current) \
               VALUES ('guid:1', 'fulltext', 2, 'guid:1/fulltext/v2', 'h2', 20, 'application/pdf', 'src', NULL, 't2', 1);",
        )
        .unwrap();

        let artifacts = load_current_artifacts(&conn).unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].r2_key, "guid:1/fulltext/v2");
    }

    #[test]
    fn map_node_kind_known_and_unknown() {
        assert_eq!(map_node_kind("work"), Some("Work"));
        assert_eq!(map_node_kind("topic"), Some("Topic"));
        assert_eq!(map_node_kind("bogus"), None);
    }

    #[test]
    fn work_records_one_per_assertion() {
        let aliases = vec![EmitAlias { namespace: "openalex".into(), value: "W1".into() }];
        let assertions = vec![
            LocalAssertion { source: "openalex".into(), attrs: serde_json::json!({"a": 1}) },
            LocalAssertion { source: "crossref".into(), attrs: serde_json::json!({"b": 2}) },
        ];
        let records = work_records_for_node("Work", &aliases, &assertions);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].source, "openalex");
        assert_eq!(records[1].source, "crossref");
        assert!(records.iter().all(|r| r.kind == "Work"));
        assert!(records.iter().all(|r| r.aliases.len() == 1));
    }

    #[test]
    fn work_records_stub_when_no_assertions() {
        let aliases = vec![EmitAlias { namespace: "openalex".into(), value: "W1".into() }];
        let records = work_records_for_node("Work", &aliases, &[]);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].source, "migration");
        assert_eq!(records[0].attrs, serde_json::json!({}));
    }

    #[test]
    fn choose_alias_prefers_openalex_then_doi_then_any() {
        let a = vec![
            EmitAlias { namespace: "isbn".into(), value: "X".into() },
            EmitAlias { namespace: "doi".into(), value: "10.1/y".into() },
        ];
        assert_eq!(choose_alias(&a).unwrap().namespace, "doi");

        let b = vec![
            EmitAlias { namespace: "doi".into(), value: "10.1/y".into() },
            EmitAlias { namespace: "openalex".into(), value: "W1".into() },
        ];
        assert_eq!(choose_alias(&b).unwrap().namespace, "openalex");

        let c = vec![EmitAlias { namespace: "isbn".into(), value: "X".into() }];
        assert_eq!(choose_alias(&c).unwrap().namespace, "isbn");

        assert!(choose_alias(&[]).is_none());
    }

    #[test]
    fn edge_input_resolves_both_endpoints_or_skips() {
        let mut map: HashMap<String, Vec<EmitAlias>> = HashMap::new();
        map.insert("guid:1".into(), vec![EmitAlias { namespace: "openalex".into(), value: "W1".into() }]);
        map.insert("guid:2".into(), vec![EmitAlias { namespace: "openalex".into(), value: "W2".into() }]);

        let ea = LocalEdgeAssertion {
            src_id: "guid:1".into(),
            dst_id: "guid:2".into(),
            relation: "cites".into(),
            source: "openalex".into(),
            attrs: None,
            fetched_at: "t".into(),
        };
        let edge = edge_input_from_assertion(&ea, &map).unwrap();
        assert_eq!(edge.src.value, "W1");
        assert_eq!(edge.dst.value, "W2");
        assert_eq!(edge.attrs, serde_json::Value::Null);

        let ea_missing_dst = LocalEdgeAssertion {
            src_id: "guid:1".into(),
            dst_id: "guid:missing".into(),
            relation: "cites".into(),
            source: "openalex".into(),
            attrs: None,
            fetched_at: "t".into(),
        };
        assert!(edge_input_from_assertion(&ea_missing_dst, &map).is_none());
    }
}
