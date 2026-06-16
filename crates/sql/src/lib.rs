//! Shared SQLite-dialect query strings used by `store-d1` and `store-sqlite` (doc02.04).
//!
//! All statements use `?` positional parameters (SQLite / D1 compatible).
//! Where a statement needs a variable number of parameters (e.g. IN-lists, the
//! `present_aliases` "have" query), use [`in_list`] or [`alias_pair_list`] to
//! build the placeholder fragment at runtime; the backend splices it into the query.
//!
//! # Merge ordering
//!
//! The `MERGE_*` queries must be run inside a single transaction, in this order:
//!
//! 1. `node::TOMBSTONE`
//! 2. `alias::MERGE_DELETE_CONFLICTS`, `alias::MERGE_REPOINT`
//! 3. `node_assertion::MERGE_UPSERT`, `node_assertion::MERGE_DELETE_LOSER`
//! 4. `edge::MERGE_EA_SRC_UPSERT`, `edge::MERGE_EA_SRC_DELETE_LOSER`
//! 5. `edge::MERGE_EDGE_SRC_DELETE_CONFLICTS`, `edge::MERGE_EDGE_SRC_REPOINT`
//! 6. `edge::MERGE_EA_DST_UPSERT`, `edge::MERGE_EA_DST_DELETE_LOSER`
//! 7. `edge::MERGE_EDGE_DST_DELETE_CONFLICTS`, `edge::MERGE_EDGE_DST_REPOINT`
//! 8. `payload::MERGE_DEMOTE_LOSER_CURRENT`, `payload::MERGE_REPOINT`, `payload::MERGE_DELETE_LOSER`
//!
//! Note: `edge_assertion` has a FK on `edge`. If FK enforcement is on, run edge_assertion
//! upserts before the corresponding edge mutations to avoid transient FK violations, and
//! rely on deferred constraint checking (or disable FK enforcement for the transaction).

// ── Migrations ──────────────────────────────────────────────────────────────

pub const MIGRATION_0001: &str = include_str!("../../../migrations/0001_init.sql");
pub const MIGRATION_0002: &str = include_str!("../../../migrations/0002_graph.sql");

/// Ordered `(name, sql)` pairs for startup application by backends.
pub fn migrations() -> &'static [(&'static str, &'static str)] {
    &[
        ("0001_init", MIGRATION_0001),
        ("0002_graph", MIGRATION_0002),
    ]
}

// ── alias ─────────────────────────────────────────────────────────────────

pub mod alias {
    /// Insert an alias; silently ignores PK conflicts (get-or-create step 1).
    /// Params: (namespace, value, canonical_id)
    pub const INSERT_IGNORE: &str = "\
        INSERT INTO alias (namespace, value, canonical_id) \
        VALUES (?, ?, ?) \
        ON CONFLICT(namespace, value) DO NOTHING";

    /// Resolve alias → canonical_id (get-or-create step 2 / plain lookup).
    /// Params: (namespace, value)
    pub const GET: &str = "\
        SELECT canonical_id FROM alias WHERE namespace = ? AND value = ?";

    /// List all (namespace, value) aliases for a canonical_id.
    /// Params: (canonical_id)
    pub const LIST_BY_NODE: &str = "\
        SELECT namespace, value FROM alias WHERE canonical_id = ?";

    // ── merge (loser → survivor) ───────────────────────────────────────────

    /// Merge step 1: delete loser aliases that would violate UNIQUE(namespace, value)
    /// if repointed to the survivor (i.e. the survivor already owns that alias).
    /// Params: (loser_canonical_id, survivor_canonical_id)
    pub const MERGE_DELETE_CONFLICTS: &str = "\
        DELETE FROM alias \
        WHERE canonical_id = ? \
          AND EXISTS ( \
            SELECT 1 FROM alias AS s \
            WHERE s.namespace = alias.namespace \
              AND s.value = alias.value \
              AND s.canonical_id = ? \
          )";

    /// Merge step 2: repoint remaining loser aliases to the survivor.
    /// Run after [`MERGE_DELETE_CONFLICTS`].
    /// Params: (survivor_canonical_id, loser_canonical_id)
    pub const MERGE_REPOINT: &str = "\
        UPDATE alias SET canonical_id = ? WHERE canonical_id = ?";
}

// ── node ──────────────────────────────────────────────────────────────────

pub mod node {
    /// Mint a node; idempotent (ON CONFLICT DO NOTHING).
    /// Params: (canonical_id, kind, created_at)
    pub const INSERT_IGNORE: &str = "\
        INSERT INTO node (canonical_id, kind, created_at) \
        VALUES (?, ?, ?) \
        ON CONFLICT(canonical_id) DO NOTHING";

    /// Tombstone a node: set `merged_into` to the survivor.
    /// Params: (survivor_canonical_id, loser_canonical_id)
    pub const TOMBSTONE: &str = "\
        UPDATE node SET merged_into = ? WHERE canonical_id = ?";

    /// Single-hop `merged_into` lookup for iterative chain walking in `resolve_live`.
    /// Params: (canonical_id)
    pub const SELECT_MERGED_INTO: &str = "\
        SELECT merged_into FROM node WHERE canonical_id = ?";

    /// Fetch (kind, merged_into) for a node.
    /// Params: (canonical_id)
    pub const SELECT: &str = "\
        SELECT kind, merged_into FROM node WHERE canonical_id = ?";
}

// ── node_assertion ────────────────────────────────────────────────────────

pub mod node_assertion {
    /// Upsert a node assertion; newer `fetched_at` always wins on conflict.
    /// Params: (canonical_id, source, attrs, fetched_at)
    pub const UPSERT: &str = "\
        INSERT INTO node_assertion (canonical_id, source, attrs, fetched_at) \
        VALUES (?, ?, ?, ?) \
        ON CONFLICT(canonical_id, source) DO UPDATE \
          SET attrs = excluded.attrs, fetched_at = excluded.fetched_at";

    /// Fetch all (source, attrs, fetched_at) assertions for a node.
    /// Params: (canonical_id)
    pub const SELECT_BY_NODE: &str = "\
        SELECT source, attrs, fetched_at FROM node_assertion WHERE canonical_id = ?";

    // ── merge ──────────────────────────────────────────────────────────────

    /// Merge step 1: copy loser assertions into survivor space; keep whichever row
    /// has the newer `fetched_at` on conflict.
    /// Params: (survivor_canonical_id, loser_canonical_id)
    pub const MERGE_UPSERT: &str = "\
        INSERT INTO node_assertion (canonical_id, source, attrs, fetched_at) \
        SELECT ?, source, attrs, fetched_at FROM node_assertion WHERE canonical_id = ? \
        ON CONFLICT(canonical_id, source) DO UPDATE \
          SET attrs = excluded.attrs, fetched_at = excluded.fetched_at \
          WHERE excluded.fetched_at > node_assertion.fetched_at";

    /// Merge step 2: delete all loser assertions after repointing.
    /// Run after [`MERGE_UPSERT`].
    /// Params: (loser_canonical_id)
    pub const MERGE_DELETE_LOSER: &str = "\
        DELETE FROM node_assertion WHERE canonical_id = ?";
}

// ── edge & edge_assertion ─────────────────────────────────────────────────

pub mod edge {
    /// Insert an edge; silently ignores PK conflicts (dedup on (src_id, dst_id, relation)).
    /// Params: (src_id, dst_id, relation)
    pub const INSERT_IGNORE: &str = "\
        INSERT INTO edge (src_id, dst_id, relation) \
        VALUES (?, ?, ?) \
        ON CONFLICT(src_id, dst_id, relation) DO NOTHING";

    // ── paginated reads ────────────────────────────────────────────────────

    /// Forward edges (src_id = ?), first page, keyset-sorted by (dst_id, relation).
    /// Params: (src_id, limit)
    pub const SELECT_FORWARD_FIRST: &str = "\
        SELECT src_id, dst_id, relation FROM edge \
        WHERE src_id = ? \
        ORDER BY dst_id, relation \
        LIMIT ?";

    /// Forward edges, subsequent page. Cursor encodes (last_dst_id, last_relation).
    /// Params: (src_id, last_dst_id, last_dst_id, last_relation, limit)
    pub const SELECT_FORWARD_PAGE: &str = "\
        SELECT src_id, dst_id, relation FROM edge \
        WHERE src_id = ? \
          AND (dst_id > ? OR (dst_id = ? AND relation > ?)) \
        ORDER BY dst_id, relation \
        LIMIT ?";

    /// Backward edges (dst_id = ?), first page, keyset-sorted by (src_id, relation).
    /// Params: (dst_id, limit)
    pub const SELECT_BACKWARD_FIRST: &str = "\
        SELECT src_id, dst_id, relation FROM edge \
        WHERE dst_id = ? \
        ORDER BY src_id, relation \
        LIMIT ?";

    /// Backward edges, subsequent page. Cursor encodes (last_src_id, last_relation).
    /// Params: (dst_id, last_src_id, last_src_id, last_relation, limit)
    pub const SELECT_BACKWARD_PAGE: &str = "\
        SELECT src_id, dst_id, relation FROM edge \
        WHERE dst_id = ? \
          AND (src_id > ? OR (src_id = ? AND relation > ?)) \
        ORDER BY src_id, relation \
        LIMIT ?";

    // ── merge (loser → survivor) ───────────────────────────────────────────
    //
    // Run in order shown; all steps within a single transaction.
    // Steps 1–2 handle edge_assertions where src = loser.
    // Steps 3–4 repoint the edge rows themselves (src direction).
    // Steps 5–6 handle edge_assertions where dst = loser.
    // Steps 7–8 repoint the edge rows themselves (dst direction).
    // The FK on edge_assertion → edge is temporarily unsatisfied between steps 1 and 3;
    // run with deferred FK checking or FK enforcement disabled.

    /// Merge step 1: copy loser src-direction edge_assertions into survivor src space;
    /// keep newer `fetched_at` on conflict.
    /// Params: (survivor_id, loser_id)
    pub const MERGE_EA_SRC_UPSERT: &str = "\
        INSERT INTO edge_assertion (src_id, dst_id, relation, source, attrs, fetched_at) \
        SELECT ?, dst_id, relation, source, attrs, fetched_at \
        FROM edge_assertion WHERE src_id = ? \
        ON CONFLICT(src_id, dst_id, relation, source) DO UPDATE \
          SET attrs = excluded.attrs, fetched_at = excluded.fetched_at \
          WHERE excluded.fetched_at > edge_assertion.fetched_at";

    /// Merge step 2: delete all edge_assertions where src_id = loser.
    /// Run after [`MERGE_EA_SRC_UPSERT`].
    /// Params: (loser_id)
    pub const MERGE_EA_SRC_DELETE_LOSER: &str = "\
        DELETE FROM edge_assertion WHERE src_id = ?";

    /// Merge step 3: delete loser edges (src direction) that would collide with an
    /// existing (survivor, dst, relation) edge.
    /// Params: (loser_id, survivor_id)
    pub const MERGE_EDGE_SRC_DELETE_CONFLICTS: &str = "\
        DELETE FROM edge \
        WHERE src_id = ? \
          AND EXISTS ( \
            SELECT 1 FROM edge AS s \
            WHERE s.src_id = ? \
              AND s.dst_id = edge.dst_id \
              AND s.relation = edge.relation \
          )";

    /// Merge step 4: repoint remaining loser src edges to the survivor.
    /// Run after [`MERGE_EDGE_SRC_DELETE_CONFLICTS`].
    /// Params: (survivor_id, loser_id)
    pub const MERGE_EDGE_SRC_REPOINT: &str = "\
        UPDATE edge SET src_id = ? WHERE src_id = ?";

    /// Merge step 5: copy loser dst-direction edge_assertions into survivor dst space;
    /// keep newer `fetched_at` on conflict.
    /// Params: (survivor_id, loser_id)
    pub const MERGE_EA_DST_UPSERT: &str = "\
        INSERT INTO edge_assertion (src_id, dst_id, relation, source, attrs, fetched_at) \
        SELECT src_id, ?, relation, source, attrs, fetched_at \
        FROM edge_assertion WHERE dst_id = ? \
        ON CONFLICT(src_id, dst_id, relation, source) DO UPDATE \
          SET attrs = excluded.attrs, fetched_at = excluded.fetched_at \
          WHERE excluded.fetched_at > edge_assertion.fetched_at";

    /// Merge step 6: delete all edge_assertions where dst_id = loser.
    /// Run after [`MERGE_EA_DST_UPSERT`].
    /// Params: (loser_id)
    pub const MERGE_EA_DST_DELETE_LOSER: &str = "\
        DELETE FROM edge_assertion WHERE dst_id = ?";

    /// Merge step 7: delete loser edges (dst direction) that would collide with an
    /// existing (src, survivor, relation) edge.
    /// Params: (loser_id, survivor_id)
    pub const MERGE_EDGE_DST_DELETE_CONFLICTS: &str = "\
        DELETE FROM edge \
        WHERE dst_id = ? \
          AND EXISTS ( \
            SELECT 1 FROM edge AS s \
            WHERE s.dst_id = ? \
              AND s.src_id = edge.src_id \
              AND s.relation = edge.relation \
          )";

    /// Merge step 8: repoint remaining loser dst edges to the survivor.
    /// Run after [`MERGE_EDGE_DST_DELETE_CONFLICTS`].
    /// Params: (survivor_id, loser_id)
    pub const MERGE_EDGE_DST_REPOINT: &str = "\
        UPDATE edge SET dst_id = ? WHERE dst_id = ?";
}

// ── edge_assertion ────────────────────────────────────────────────────────

pub mod edge_assertion {
    /// Upsert an edge assertion; newer `fetched_at` wins on conflict.
    /// Params: (src_id, dst_id, relation, source, attrs, fetched_at)
    pub const UPSERT: &str = "\
        INSERT INTO edge_assertion (src_id, dst_id, relation, source, attrs, fetched_at) \
        VALUES (?, ?, ?, ?, ?, ?) \
        ON CONFLICT(src_id, dst_id, relation, source) DO UPDATE \
          SET attrs = excluded.attrs, fetched_at = excluded.fetched_at";

    /// Fetch all (source, attrs, fetched_at) assertions for a given edge.
    /// Params: (src_id, dst_id, relation)
    pub const SELECT_BY_EDGE: &str = "\
        SELECT source, attrs, fetched_at FROM edge_assertion \
        WHERE src_id = ? AND dst_id = ? AND relation = ?";
}

// ── payload ───────────────────────────────────────────────────────────────

pub mod payload {
    /// Fetch the current payload descriptor for (canonical_id, kind).
    /// Params: (canonical_id, kind)
    pub const SELECT_CURRENT: &str = "\
        SELECT canonical_id, kind, version, r2_key, content_hash, byte_size, \
               mime, rights, source, source_url, fetched_at, is_current \
        FROM payloads \
        WHERE canonical_id = ? AND kind = ? AND is_current = 1";

    /// Compute the next version number for (canonical_id, kind).
    /// Returns 1 when no rows exist yet.
    /// Params: (canonical_id, kind)
    pub const NEXT_VERSION: &str = "\
        SELECT COALESCE(MAX(version), 0) + 1 AS next_version \
        FROM payloads \
        WHERE canonical_id = ? AND kind = ?";

    /// Demote any prior current version before inserting a new one.
    /// Params: (canonical_id, kind)
    pub const FLIP_CURRENT_OFF: &str = "\
        UPDATE payloads SET is_current = 0 \
        WHERE canonical_id = ? AND kind = ? AND is_current = 1";

    /// Insert a new payload row.
    /// Params: (canonical_id, kind, version, r2_key, content_hash, byte_size,
    ///          mime, rights, source, source_url, fetched_at, is_current)
    pub const INSERT: &str = "\
        INSERT INTO payloads \
          (canonical_id, kind, version, r2_key, content_hash, byte_size, \
           mime, rights, source, source_url, fetched_at, is_current) \
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

    // ── merge ──────────────────────────────────────────────────────────────

    /// Merge step 1: demote loser's current payloads for any kind where the survivor
    /// already has a current version.
    /// Params: (loser_canonical_id, survivor_canonical_id)
    pub const MERGE_DEMOTE_LOSER_CURRENT: &str = "\
        UPDATE payloads SET is_current = 0 \
        WHERE canonical_id = ? \
          AND kind IN ( \
            SELECT kind FROM payloads WHERE canonical_id = ? AND is_current = 1 \
          )";

    /// Merge step 2: repoint all loser payload rows to the survivor.
    /// PK collisions (same kind + version) are silently ignored.
    /// Params: (survivor_canonical_id, loser_canonical_id)
    pub const MERGE_REPOINT: &str = "\
        INSERT INTO payloads \
          (canonical_id, kind, version, r2_key, content_hash, byte_size, \
           mime, rights, source, source_url, fetched_at, is_current) \
        SELECT ?, kind, version, r2_key, content_hash, byte_size, \
               mime, rights, source, source_url, fetched_at, is_current \
        FROM payloads WHERE canonical_id = ? \
        ON CONFLICT(canonical_id, kind, version) DO NOTHING";

    /// Merge step 3: delete all loser payload rows after repointing.
    /// Run after [`MERGE_REPOINT`].
    /// Params: (loser_canonical_id)
    pub const MERGE_DELETE_LOSER: &str = "\
        DELETE FROM payloads WHERE canonical_id = ?";
}

// ── variable-arity helpers ────────────────────────────────────────────────

/// Build a `(?, ?, …)` placeholder string with `n` slots.
///
/// Use for single-column IN-lists, e.g. a list of canonical_ids.
///
/// # Panics
///
/// Panics if `n == 0`.
pub fn in_list(n: usize) -> String {
    assert!(n > 0, "in_list: n must be >= 1");
    let mut s = String::with_capacity(n * 3 + 2);
    s.push('(');
    for i in 0..n {
        if i > 0 {
            s.push_str(", ");
        }
        s.push('?');
    }
    s.push(')');
    s
}

/// Build a `(?, ?), (?, ?), …` placeholder fragment with `n` (namespace, value) pairs.
///
/// Splice into the `present_aliases` ("have") query:
///
/// ```text
/// SELECT namespace, value FROM alias
/// WHERE (namespace, value) IN <alias_pair_list(n)>
/// ```
///
/// Bind params as `(ns0, val0, ns1, val1, …)`.
///
/// # Panics
///
/// Panics if `n == 0`.
pub fn alias_pair_list(n: usize) -> String {
    assert!(n > 0, "alias_pair_list: n must be >= 1");
    let mut s = String::with_capacity(n * 9 + 2);
    s.push('(');
    for i in 0..n {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str("(?, ?)");
    }
    s.push(')');
    s
}
