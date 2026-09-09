//! `store diff` / `store sync`: compare two stores and replay what the
//! destination lacks from the source, entirely over the public HTTP surface
//! (`GET /export/*` to enumerate, `PUT /works` / `PUT /edges` /
//! `PUT .../content/{role}` to write). The destination's own alias-derived
//! identity resolution does the merging, so replay is idempotent and both
//! directions use the same code path.

use std::collections::{HashMap, HashSet};

use crate::store_client::{ContentOutcome, StoreClient};

/// Page size for `/export/*` drains (server caps at 200).
const EXPORT_PAGE: u32 = 200;
/// Node pages carry full assertions (multi-megabyte abstracts), and a big page
/// can trip the worker's per-request CPU limit — keep them small.
const NODE_PAGE: u32 = 50;
/// Max ids per `POST /works/have` request.
const HAVE_BATCH: usize = 500;
/// Max edges per `PUT /edges` request. Each edge costs the server several
/// store queries (stub resolution per endpoint plus the upsert), and the
/// worker's free-plan cap is ~50 subrequests per invocation — batches must
/// stay well under that.
const EDGE_BATCH: usize = 8;

// ─── wire shapes (from /export/*) ────────────────────────────────────────────

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct WireAlias {
    scheme: String,
    value: String,
}

#[derive(Debug, serde::Deserialize)]
struct WireAssertion {
    source: String,
    attrs: serde_json::Value,
    fetched_at: String,
}

#[derive(Debug, serde::Deserialize)]
struct WireNode {
    canonical_id: String,
    kind: String,
    aliases: Vec<WireAlias>,
    #[serde(default)]
    assertions: Vec<WireAssertion>,
}

#[derive(Debug, serde::Deserialize)]
struct WireEdge {
    src_id: String,
    dst_id: String,
    relation: String,
    source: String,
    #[serde(default)]
    attrs: Option<serde_json::Value>,
    fetched_at: String,
}

#[derive(Debug, serde::Deserialize)]
struct WireArtifact {
    canonical_id: String,
    role: String,
    mime: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    source_url: Option<String>,
    fetched_at: String,
}

fn parse_items<T: serde::de::DeserializeOwned>(
    items: Vec<serde_json::Value>,
    what: &str,
) -> Result<Vec<T>, String> {
    items
        .into_iter()
        .map(|v| serde_json::from_value(v).map_err(|e| format!("parse {what}: {e}")))
        .collect()
}

// ─── alias selection (mirrors the retired migrate-store rules) ───────────────

const ALIAS_PREFERENCE: [&str; 2] = ["openalex", "doi"];

/// Pick the alias used to address a node over HTTP: `openalex`, then `doi`,
/// else whichever alias comes first.
fn choose_alias(aliases: &[WireAlias]) -> Option<&WireAlias> {
    for ns in ALIAS_PREFERENCE {
        if let Some(a) = aliases.iter().find(|a| a.scheme == ns) {
            return Some(a);
        }
    }
    aliases.first()
}

fn alias_key(a: &WireAlias) -> String {
    format!("{}:{}", a.scheme, a.value)
}

fn have_batched(client: &StoreClient, ids: &[String]) -> Result<HashSet<String>, String> {
    let mut present = HashSet::new();
    for chunk in ids.chunks(HAVE_BATCH) {
        present.extend(client.have(chunk).map_err(|e| e.to_string())?);
    }
    Ok(present)
}

fn drain_nodes(client: &StoreClient, label: &str) -> Result<Vec<WireNode>, String> {
    let items = client
        .export_all("nodes", NODE_PAGE)
        .map_err(|e| format!("{label}: export nodes: {e}"))?;
    parse_items(items, "node")
}

// ─── diff ────────────────────────────────────────────────────────────────────

/// One side of a diff: headline stats plus what this side holds that the
/// other lacks.
pub struct DiffSide {
    pub label: String,
    pub stats: serde_json::Value,
    pub artifacts: usize,
    /// Nodes on this side with no alias known to the other side.
    pub only_here: usize,
    /// Work nodes among `only_here`.
    pub works_only_here: usize,
    /// Example alias keys (up to 5) from `only_here`.
    pub examples: Vec<String>,
    /// Nodes with no alias at all — invisible to alias-based comparison.
    pub unaddressable: usize,
}

pub struct DiffReport {
    pub a: DiffSide,
    pub b: DiffSide,
}

fn diff_side(
    label: &str,
    stats: serde_json::Value,
    artifacts: usize,
    nodes: &[WireNode],
    other_aliases: &HashSet<String>,
) -> DiffSide {
    let mut only_here = 0;
    let mut works_only_here = 0;
    let mut unaddressable = 0;
    let mut examples = Vec::new();
    for n in nodes {
        if n.aliases.is_empty() {
            unaddressable += 1;
            continue;
        }
        if n.aliases.iter().all(|a| !other_aliases.contains(&alias_key(a))) {
            only_here += 1;
            if n.kind == "Work" {
                works_only_here += 1;
            }
            if examples.len() < 5 {
                if let Some(a) = choose_alias(&n.aliases) {
                    examples.push(alias_key(a));
                }
            }
        }
    }
    DiffSide {
        label: label.to_string(),
        stats,
        artifacts,
        only_here,
        works_only_here,
        examples,
        unaddressable,
    }
}

/// Compare two stores: headline stats per side, plus alias-set difference
/// (which nodes exist on one side only).
pub fn diff(
    a: &StoreClient,
    a_label: &str,
    b: &StoreClient,
    b_label: &str,
) -> Result<DiffReport, String> {
    let stats_a = a.stats().map_err(|e| format!("{a_label}: stats: {e}"))?;
    let stats_b = b.stats().map_err(|e| format!("{b_label}: stats: {e}"))?;
    let artifacts_a = a
        .export_all("artifacts", EXPORT_PAGE)
        .map_err(|e| format!("{a_label}: export artifacts: {e}"))?
        .len();
    let artifacts_b = b
        .export_all("artifacts", EXPORT_PAGE)
        .map_err(|e| format!("{b_label}: export artifacts: {e}"))?
        .len();
    let nodes_a = drain_nodes(a, a_label)?;
    let nodes_b = drain_nodes(b, b_label)?;

    let aliases_a: HashSet<String> =
        nodes_a.iter().flat_map(|n| n.aliases.iter().map(alias_key)).collect();
    let aliases_b: HashSet<String> =
        nodes_b.iter().flat_map(|n| n.aliases.iter().map(alias_key)).collect();

    Ok(DiffReport {
        a: diff_side(a_label, stats_a, artifacts_a, &nodes_a, &aliases_b),
        b: diff_side(b_label, stats_b, artifacts_b, &nodes_b, &aliases_a),
    })
}

pub fn print_diff(report: &DiffReport) {
    let stat = |s: &serde_json::Value, k: &str| s[k].as_u64().unwrap_or(0);
    let header = format!("{:<28} {:>14} {:>14}", "", report.a.label, report.b.label);
    eprintln!("{header}");
    for key in ["works", "works_described", "works_stub", "nodes_total", "edges_total", "library_bytes", "catalog_bytes"] {
        eprintln!(
            "{key:<28} {:>14} {:>14}",
            stat(&report.a.stats, key),
            stat(&report.b.stats, key)
        );
    }
    eprintln!("{:<28} {:>14} {:>14}", "artifacts (current)", report.a.artifacts, report.b.artifacts);
    if stat(&report.a.stats, "catalog_bytes") == 0 || stat(&report.b.stats, "catalog_bytes") == 0 {
        eprintln!("note: a catalog_bytes of 0 means that backend has no on-disk file; bytes are not comparable");
    }
    for side in [&report.a, &report.b] {
        eprintln!(
            "only in {}: {} nodes ({} works){}",
            side.label,
            side.only_here,
            side.works_only_here,
            if side.examples.is_empty() {
                String::new()
            } else {
                format!(" — e.g. {}", side.examples.join(", "))
            }
        );
        if side.unaddressable > 0 {
            eprintln!(
                "note: {} nodes in {} have no alias and cannot be compared or synced",
                side.unaddressable, side.label
            );
        }
    }
    if report.a.only_here == 0 && report.b.only_here == 0 {
        eprintln!("stores agree on every addressable node");
    }
}

// ─── sync ────────────────────────────────────────────────────────────────────

/// Per-category counts from a sync run (or dry-run plan).
#[derive(Debug, Default)]
pub struct SyncReport {
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

impl SyncReport {
    /// Anything that kept content from reaching the destination.
    pub fn is_partial(&self) -> bool {
        self.works_skipped_no_alias > 0
            || self.edges_skipped_no_alias > 0
            || self.artifacts_skipped_no_alias > 0
            || self.artifacts_missing_blob > 0
            || self.artifacts_errors > 0
            || !self.errors.is_empty()
    }
}

pub fn print_report(report: &SyncReport, dry_run: bool) {
    let label = if dry_run { "plan" } else { "synced" };
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
        eprintln!("sync error: {e}");
    }
    if report.is_partial() {
        eprintln!("WARNING: sync is PARTIAL — some content did not reach the destination (see counts above)");
    }
}

/// Replay everything the destination lacks from the source. Idempotent:
/// re-running pushes nothing that already arrived.
pub fn sync(src: &StoreClient, dst: &StoreClient, dry_run: bool) -> Result<SyncReport, String> {
    let nodes = drain_nodes(src, "source")?;
    let edge_items = src
        .export_all("edges", EXPORT_PAGE)
        .map_err(|e| format!("source: export edges: {e}"))?;
    let edges: Vec<WireEdge> = parse_items(edge_items, "edge")?;
    let artifact_items = src
        .export_all("artifacts", EXPORT_PAGE)
        .map_err(|e| format!("source: export artifacts: {e}"))?;
    let artifacts: Vec<WireArtifact> = parse_items(artifact_items, "artifact")?;

    let alias_by_node: HashMap<&str, &[WireAlias]> = nodes
        .iter()
        .map(|n| (n.canonical_id.as_str(), n.aliases.as_slice()))
        .collect();

    let mut report = SyncReport {
        works_total: nodes.len(),
        edges_total: edges.len(),
        artifacts_total: artifacts.len(),
        ..Default::default()
    };

    // Prefilter: batch-check every source alias so nodes that are already
    // fully present in the destination are skipped without a PUT.
    let all_alias_ids: Vec<String> =
        nodes.iter().flat_map(|n| n.aliases.iter().map(alias_key)).collect();
    let present = have_batched(dst, &all_alias_ids)?;

    // ---- Works ----
    for node in &nodes {
        if node.aliases.is_empty() {
            report.works_skipped_no_alias += 1;
            continue;
        }
        if node.aliases.iter().all(|a| present.contains(&alias_key(a))) {
            report.works_skipped_present += 1;
            continue;
        }
        // One record per assertion preserves per-provider provenance; a node
        // with zero assertions (a cited-only stub) still registers its aliases
        // via a single empty-attrs record.
        let records: Vec<serde_json::Value> = if node.assertions.is_empty() {
            vec![serde_json::json!({
                "source": "migration",
                "kind": node.kind,
                "aliases": node.aliases,
                "attrs": {},
            })]
        } else {
            node.assertions
                .iter()
                .map(|a| {
                    serde_json::json!({
                        "source": a.source,
                        "kind": node.kind,
                        "aliases": node.aliases,
                        "attrs": a.attrs,
                        "fetched_at": a.fetched_at,
                    })
                })
                .collect()
        };
        if dry_run {
            report.works_pushed += records.len();
            continue;
        }
        for record in records {
            match dst.put_work(&record) {
                Ok(_) => report.works_pushed += 1,
                Err(e) => report.errors.push(format!("put_work {}: {e}", node.canonical_id)),
            }
        }
    }

    // ---- Edges ----
    let mut edge_batch: Vec<serde_json::Value> = Vec::new();
    for ea in &edges {
        let src_alias = alias_by_node.get(ea.src_id.as_str()).and_then(|a| choose_alias(a));
        let dst_alias = alias_by_node.get(ea.dst_id.as_str()).and_then(|a| choose_alias(a));
        let (Some(src_alias), Some(dst_alias)) = (src_alias, dst_alias) else {
            report.edges_skipped_no_alias += 1;
            continue;
        };
        if dry_run {
            report.edges_pushed += 1;
            continue;
        }
        edge_batch.push(serde_json::json!({
            "src": src_alias,
            "dst": dst_alias,
            "relation": ea.relation,
            "source": ea.source,
            "attrs": ea.attrs,
            "fetched_at": ea.fetched_at,
        }));
        if edge_batch.len() >= EDGE_BATCH {
            flush_edges(dst, &mut edge_batch, &mut report);
        }
    }
    if !edge_batch.is_empty() {
        flush_edges(dst, &mut edge_batch, &mut report);
    }

    // ---- Artifacts ----
    for art in &artifacts {
        let alias = alias_by_node
            .get(art.canonical_id.as_str())
            .and_then(|a| choose_alias(a));
        let Some(alias) = alias else {
            report.artifacts_skipped_no_alias += 1;
            continue;
        };
        let alias_str = alias_key(alias);

        match dst.get_content(&alias_str, &art.role) {
            Ok(ContentOutcome::Bytes { .. }) | Ok(ContentOutcome::Pending) => {
                report.artifacts_skipped_present += 1;
                continue;
            }
            Ok(ContentOutcome::Absent) => {}
            Err(e) => {
                report.errors.push(format!("dest get_content {alias_str}/{}: {e}", art.role));
                continue;
            }
        }

        if dry_run {
            report.artifacts_pushed += 1;
            continue;
        }

        // Pull the bytes from the source; a Pending source descriptor has no
        // blob behind it and cannot be carried over.
        let bytes = match src.get_content(&alias_str, &art.role) {
            Ok(ContentOutcome::Bytes { bytes, .. }) => bytes,
            Ok(ContentOutcome::Pending) | Ok(ContentOutcome::Absent) => {
                report.artifacts_missing_blob += 1;
                continue;
            }
            Err(e) => {
                report.artifacts_errors += 1;
                report.errors.push(format!("source get_content {alias_str}/{}: {e}", art.role));
                continue;
            }
        };

        match dst.put_content_with_fetched_at(
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

fn flush_edges(dst: &StoreClient, batch: &mut Vec<serde_json::Value>, report: &mut SyncReport) {
    match dst.put_edges(batch) {
        Ok(n) => report.edges_pushed += n,
        Err(e) => report.errors.push(format!("put_edges batch of {}: {e}", batch.len())),
    }
    batch.clear();
    // Pace the write bursts — the worker 503s under sustained load.
    std::thread::sleep(std::time::Duration::from_millis(150));
}

// ─── L3 (Research Collection) phase ──────────────────────────────────────────
//
// The diff/merge rules live in `braincrawl-l3-sync` (node-level, anchors as the
// unit — see that crate's module doc). This section only moves bytes: fetch
// both collections over `/api/l3/docs`, run the pure rules, PUT the planned
// writes through the destination's own normalization.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::store_client::{ClientError, L3PutError};

/// Every doc in a store's collection as (slug, content), or `None` when the
/// store does not serve L3 at all (route absent / no root configured).
fn fetch_collection(
    client: &StoreClient,
    label: &str,
) -> Result<Option<Vec<(String, String)>>, String> {
    let listing = match client.l3_list() {
        Ok(l) => l,
        Err(ClientError::Server { status: 404, .. }) => return Ok(None),
        Err(e) => return Err(format!("{label}: list docs: {e}")),
    };
    let mut docs = Vec::with_capacity(listing.len());
    for entry in listing {
        match client.l3_get(&entry.doc) {
            Ok(Some(content)) => docs.push((entry.doc, content)),
            Ok(None) => {}
            Err(e) => return Err(format!("{label}: get doc {}: {e}", entry.doc)),
        }
    }
    Ok(Some(docs))
}

/// Every agent context file as (name, content), or `None` when the store does
/// not serve the agent-file routes.
fn fetch_agent_files(client: &StoreClient) -> Result<Option<Vec<(String, String)>>, String> {
    let listing = match client.l3_agent_list() {
        Ok(l) => l,
        Err(ClientError::Server { status: 404, .. }) => return Ok(None),
        Err(e) => return Err(format!("list: {e}")),
    };
    let mut files = Vec::with_capacity(listing.len());
    for entry in listing {
        match client.l3_agent_get(&entry.name) {
            Ok(Some(content)) => files.push((entry.name, content)),
            Ok(None) => {}
            Err(e) => return Err(format!("get {}: {e}", entry.name)),
        }
    }
    Ok(Some(files))
}

/// Compare both stores' Research Collections node-by-node. `Ok(None)` when a
/// side does not serve L3.
pub fn l3_diff(
    a: &StoreClient,
    a_label: &str,
    b: &StoreClient,
    b_label: &str,
) -> Result<Option<braincrawl_l3_sync::CollectionDiff>, String> {
    let (Some(docs_a), Some(docs_b)) =
        (fetch_collection(a, a_label)?, fetch_collection(b, b_label)?)
    else {
        return Ok(None);
    };
    Ok(Some(braincrawl_l3_sync::diff_collections(&docs_a, &docs_b)))
}

pub fn print_l3_diff(diff: &Option<braincrawl_l3_sync::CollectionDiff>, a_label: &str, b_label: &str) {
    let Some(d) = diff else {
        eprintln!("research collection: not compared (a side does not serve L3)");
        return;
    };
    eprintln!(
        "research collection: {} nodes in sync; only in {}: {}; only in {}: {}; differing: {}; moved: {}",
        d.in_sync,
        a_label,
        d.only_in_a.len(),
        b_label,
        d.only_in_b.len(),
        d.differing.len(),
        d.moved.len(),
    );
    for (list, tag) in [(&d.only_in_a, format!("only in {a_label}")), (&d.only_in_b, format!("only in {b_label}")), (&d.differing, "differing".to_string())] {
        for n in list.iter().take(5) {
            eprintln!("  {tag}: {} ^{} ({})", n.doc, n.anchor, n.title);
        }
        if list.len() > 5 {
            eprintln!("  {tag}: … {} more", list.len() - 5);
        }
    }
    if d.unanchored_a + d.unanchored_b > 0 {
        eprintln!(
            "note: {} unanchored node(s) ({a_label}: {}, {b_label}: {}) are invisible to node sync — run `collection assign-ids`",
            d.unanchored_a + d.unanchored_b, d.unanchored_a, d.unanchored_b
        );
    }
}

/// What the L3 phase of a sync did.
#[derive(Debug, Default)]
pub struct L3SyncReport {
    /// Set when the phase did not run (a side does not serve L3).
    pub skipped: Option<String>,
    pub docs_written: usize,
    pub nodes_added: usize,
    pub nodes_updated: usize,
    pub nodes_line_union: usize,
    pub conflicts: Vec<braincrawl_l3_sync::NodeRef>,
    pub meta_updated: usize,
    pub meta_conflicts: Vec<String>,
    pub deleted_reported: usize,
    pub moved: usize,
    pub unanchored_source: usize,
    pub agent_added: usize,
    pub agent_updated: usize,
    pub agent_conflicts: Vec<String>,
    pub errors: Vec<String>,
}

impl L3SyncReport {
    pub fn is_partial(&self) -> bool {
        !self.conflicts.is_empty()
            || !self.meta_conflicts.is_empty()
            || !self.agent_conflicts.is_empty()
            || !self.errors.is_empty()
    }
}

pub fn print_l3_report(report: &L3SyncReport, dry_run: bool) {
    if let Some(reason) = &report.skipped {
        eprintln!("research collection: skipped ({reason})");
        return;
    }
    let label = if dry_run { "plan" } else { "synced" };
    eprintln!(
        "research collection ({label}): {} doc(s) written — {} node(s) added, {} updated, {} line-union, {} conflict(s)",
        report.docs_written,
        report.nodes_added,
        report.nodes_updated,
        report.nodes_line_union,
        report.conflicts.len(),
    );
    for c in &report.conflicts {
        eprintln!("  CONFLICT {} ^{} ({}) — changed on both sides; resolve by hand", c.doc, c.anchor, c.title);
    }
    if report.meta_updated > 0 {
        eprintln!("  {} doc frontmatter/prologue block(s) carried over", report.meta_updated);
    }
    for slug in &report.meta_conflicts {
        eprintln!("  CONFLICT {slug} (frontmatter/prologue changed on both sides)");
    }
    if report.agent_added + report.agent_updated > 0 {
        eprintln!(
            "  agent files: {} added, {} updated",
            report.agent_added, report.agent_updated
        );
    }
    for name in &report.agent_conflicts {
        eprintln!("  CONFLICT agent:{name} (changed on both sides)");
    }
    if report.deleted_reported > 0 {
        eprintln!("  note: {} node(s) deleted on one side were kept (deletions never propagate)", report.deleted_reported);
    }
    if report.moved > 0 {
        eprintln!("  note: {} node(s) live in different docs per side — not moved automatically", report.moved);
    }
    if report.unanchored_source > 0 {
        eprintln!("  note: {} unanchored source node(s) skipped — run `collection assign-ids`", report.unanchored_source);
    }
    for e in &report.errors {
        eprintln!("  l3 sync error: {e}");
    }
}

/// Where the per-store-pair node base hashes live. The base is
/// direction-independent, so the pair key sorts the two URLs.
fn pair_state_path(url_a: &str, url_b: &str) -> PathBuf {
    use sha2::{Digest, Sha256};
    let mut urls = [url_a.trim_end_matches('/'), url_b.trim_end_matches('/')];
    urls.sort();
    let digest = Sha256::digest(urls.join("|").as_bytes());
    let key: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".local/share/braincrawl/store-sync-state").join(format!("{key}.json"))
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct PairState {
    stores: Vec<String>,
    anchors: BTreeMap<String, String>,
}

fn load_pair_state(path: &PathBuf) -> PairState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Sync the Research Collection, source → dest, using the node-level rules in
/// `braincrawl-l3-sync`. Writes go through the destination's PUT normalization;
/// per-anchor base hashes for this store pair are kept under
/// `~/.local/share/braincrawl/store-sync-state/`.
pub fn l3_sync(
    src: &StoreClient,
    src_url: &str,
    dst: &StoreClient,
    dst_url: &str,
    dry_run: bool,
    doc_filter: Option<&str>,
    force: bool,
) -> Result<L3SyncReport, String> {
    let mut report = L3SyncReport::default();
    let Some(mut source_docs) = fetch_collection(src, "source")? else {
        report.skipped = Some("source does not serve L3".to_string());
        return Ok(report);
    };
    let Some(mut dest_docs) = fetch_collection(dst, "dest")? else {
        report.skipped = Some("dest does not serve L3".to_string());
        return Ok(report);
    };
    if let Some(slug) = doc_filter {
        source_docs.retain(|(s, _)| s == slug);
        dest_docs.retain(|(s, _)| s == slug);
        if source_docs.is_empty() {
            return Err(format!("--doc {slug}: the source has no such doc"));
        }
    }

    let state_path = pair_state_path(src_url, dst_url);
    let mut state = load_pair_state(&state_path);
    let plan = braincrawl_l3_sync::plan_merge(&source_docs, &dest_docs, &state.anchors);

    report.nodes_added = plan.nodes_added;
    report.nodes_updated = plan.nodes_updated;
    report.nodes_line_union = plan.nodes_line_union;
    report.conflicts = plan.conflicts.clone();
    report.meta_updated = plan.meta_updated;
    report.meta_conflicts = plan.meta_conflicts.clone();
    report.deleted_reported = plan.deleted_on_dest.len() + plan.deleted_on_source.len();
    report.moved = plan.moved.len();
    report.unanchored_source = plan.unanchored_source;

    // Agent context files: opaque, whole-file, same base logic. A single-doc
    // run leaves them alone.
    let agent_plan = if doc_filter.is_none() {
        match (fetch_agent_files(src), fetch_agent_files(dst)) {
            (Ok(Some(s_files)), Ok(Some(d_files))) => Some(braincrawl_l3_sync::plan_file_sync(
                &s_files,
                &d_files,
                &state.anchors,
                "agent:",
            )),
            (Err(e), _) | (_, Err(e)) => {
                report.errors.push(format!("agent files: {e}"));
                None
            }
            _ => None,
        }
    } else {
        None
    };
    if let Some(p) = &agent_plan {
        report.agent_added = p.added;
        report.agent_updated = p.updated;
        report.agent_conflicts = p.conflicts.clone();
        report.deleted_reported += p.deleted_reported;
    }

    if dry_run {
        report.docs_written = plan.writes.len();
        return Ok(report);
    }

    let mut state_after = plan.state_after.clone();
    for (slug, content) in &plan.writes {
        match dst.l3_put(slug, content, force) {
            Ok(normalized) => {
                report.docs_written += 1;
                // The destination may normalize on write; record the normalized
                // node hashes so the next run compares against what it stored.
                for node in braincrawl_l3_sync::split_doc(&normalized).nodes {
                    if let Some(anchor) = &node.anchor {
                        if state_after.contains_key(anchor) {
                            state_after.insert(anchor.clone(), braincrawl_l3_sync::node_hash(&node));
                        }
                    }
                }
            }
            Err(L3PutError::Warnings(w)) => {
                drop_doc_anchors(&mut state_after, content);
                report.errors.push(format!(
                    "{slug}: rejected with {} parse warning(s) — fix the doc or retry with `store sync <src> <dst> --doc {slug} --force`",
                    w.len()
                ));
            }
            Err(e) => {
                drop_doc_anchors(&mut state_after, content);
                report.errors.push(format!("{slug}: {e}"));
            }
        }
    }

    if let Some(p) = agent_plan {
        state_after.extend(p.state_after.clone());
        for (name, content) in &p.writes {
            if let Err(e) = dst.l3_agent_put(name, content) {
                state_after.remove(&format!("agent:{name}"));
                report.errors.push(format!("agent:{name}: {e}"));
            }
        }
    }

    state.stores = {
        let mut urls = vec![src_url.to_string(), dst_url.to_string()];
        urls.sort();
        urls
    };
    for (anchor, hash) in state_after {
        state.anchors.insert(anchor, hash);
    }
    if let Some(parent) = state_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&state).map_err(|e| e.to_string())?;
    std::fs::write(&state_path, json).map_err(|e| e.to_string())?;

    Ok(report)
}

/// A failed doc write must not advance the base for that doc's anchors.
fn drop_doc_anchors(state_after: &mut BTreeMap<String, String>, doc_content: &str) {
    for node in braincrawl_l3_sync::split_doc(doc_content).nodes {
        if let Some(anchor) = &node.anchor {
            state_after.remove(anchor);
        }
    }
}
