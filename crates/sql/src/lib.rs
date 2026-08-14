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
//! 8. `artifact::MERGE_DEMOTE_LOSER_CURRENT`, `artifact::MERGE_REPOINT`, `artifact::MERGE_DELETE_LOSER`
//!
//! Note: `edge_assertion` has a FK on `edge`. If FK enforcement is on, run edge_assertion
//! upserts before the corresponding edge mutations to avoid transient FK violations, and
//! rely on deferred constraint checking (or disable FK enforcement for the transaction).

// ── Migrations ──────────────────────────────────────────────────────────────

pub const MIGRATION_0001: &str = include_str!("../../../migrations/0001_init.sql");
pub const MIGRATION_0002: &str = include_str!("../../../migrations/0002_graph.sql");
pub const MIGRATION_0003: &str = include_str!("../../../migrations/0003_jobs.sql");
pub const MIGRATION_0004: &str =
    include_str!("../../../migrations/0004_rename_payloads_to_artifacts.sql");
pub const MIGRATION_0005: &str =
    include_str!("../../../migrations/0005_drop_artifact_rights.sql");
pub const MIGRATION_0006: &str =
    include_str!("../../../migrations/0006_artifact_derived_from.sql");

/// Ordered `(name, sql)` pairs for startup application by backends.
pub fn migrations() -> &'static [(&'static str, &'static str)] {
    &[
        ("0001_init", MIGRATION_0001),
        ("0002_graph", MIGRATION_0002),
        ("0003_jobs", MIGRATION_0003),
        ("0004_rename_payloads_to_artifacts", MIGRATION_0004),
        ("0005_drop_artifact_rights", MIGRATION_0005),
        ("0006_artifact_derived_from", MIGRATION_0006),
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
    /// Create a node; idempotent (ON CONFLICT DO NOTHING).
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

// ── artifact ────────────────────────────────────────────────────────────────

pub mod artifact {
    /// Fetch the current artifact descriptor for (canonical_id, role).
    /// Params: (canonical_id, role)
    pub const SELECT_CURRENT: &str = "\
        SELECT canonical_id, role, version, r2_key, content_hash, byte_size, \
               mime, source, source_url, fetched_at, is_current, \
               derived_from_role, derived_from_version \
        FROM artifacts \
        WHERE canonical_id = ? AND role = ? AND is_current = 1";

    /// Compute the next version number for (canonical_id, role).
    /// Returns 1 when no rows exist yet.
    /// Params: (canonical_id, role)
    pub const NEXT_VERSION: &str = "\
        SELECT COALESCE(MAX(version), 0) + 1 AS next_version \
        FROM artifacts \
        WHERE canonical_id = ? AND role = ?";

    /// Demote any prior current version before inserting a new one.
    /// Params: (canonical_id, role)
    pub const FLIP_CURRENT_OFF: &str = "\
        UPDATE artifacts SET is_current = 0 \
        WHERE canonical_id = ? AND role = ? AND is_current = 1";

    /// Every current artifact for a canonical id, all roles.
    /// Params: (canonical_id)
    pub const LIST_CURRENT: &str = "\
        SELECT canonical_id, role, version, r2_key, content_hash, byte_size, \
               mime, source, source_url, fetched_at, is_current, \
               derived_from_role, derived_from_version \
        FROM artifacts \
        WHERE canonical_id = ? AND is_current = 1 \
        ORDER BY role, version DESC";

    /// Every artifact for a canonical id regardless of `is_current`.
    /// Params: (canonical_id)
    pub const LIST_ALL_VERSIONS: &str = "\
        SELECT canonical_id, role, version, r2_key, content_hash, byte_size, \
               mime, source, source_url, fetched_at, is_current, \
               derived_from_role, derived_from_version \
        FROM artifacts \
        WHERE canonical_id = ? \
        ORDER BY role, version DESC";

    /// As [`LIST_CURRENT`], restricted to one role.
    /// Params: (canonical_id, role)
    pub const LIST_CURRENT_BY_ROLE: &str = "\
        SELECT canonical_id, role, version, r2_key, content_hash, byte_size, \
               mime, source, source_url, fetched_at, is_current, \
               derived_from_role, derived_from_version \
        FROM artifacts \
        WHERE canonical_id = ? AND role = ? AND is_current = 1 \
        ORDER BY role, version DESC";

    /// As [`LIST_ALL_VERSIONS`], restricted to one role.
    /// Params: (canonical_id, role)
    pub const LIST_ALL_VERSIONS_BY_ROLE: &str = "\
        SELECT canonical_id, role, version, r2_key, content_hash, byte_size, \
               mime, source, source_url, fetched_at, is_current, \
               derived_from_role, derived_from_version \
        FROM artifacts \
        WHERE canonical_id = ? AND role = ? \
        ORDER BY role, version DESC";

    /// Insert a new artifact row.
    /// Params: (canonical_id, role, version, r2_key, content_hash, byte_size,
    ///          mime, source, source_url, fetched_at, is_current,
    ///          derived_from_role, derived_from_version)
    pub const INSERT: &str = "\
        INSERT INTO artifacts \
          (canonical_id, role, version, r2_key, content_hash, byte_size, \
           mime, source, source_url, fetched_at, is_current, \
           derived_from_role, derived_from_version) \
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

    // ── merge ──────────────────────────────────────────────────────────────

    /// Merge step 1: demote loser's current artifacts for any role where the survivor
    /// already has a current version.
    /// Params: (loser_canonical_id, survivor_canonical_id)
    pub const MERGE_DEMOTE_LOSER_CURRENT: &str = "\
        UPDATE artifacts SET is_current = 0 \
        WHERE canonical_id = ? \
          AND role IN ( \
            SELECT role FROM artifacts WHERE canonical_id = ? AND is_current = 1 \
          )";

    /// Merge step 2: repoint all loser artifact rows to the survivor.
    /// PK collisions (same role + version) are silently ignored.
    /// Params: (survivor_canonical_id, loser_canonical_id)
    pub const MERGE_REPOINT: &str = "\
        INSERT INTO artifacts \
          (canonical_id, role, version, r2_key, content_hash, byte_size, \
           mime, source, source_url, fetched_at, is_current, \
           derived_from_role, derived_from_version) \
        SELECT ?, role, version, r2_key, content_hash, byte_size, \
               mime, source, source_url, fetched_at, is_current, \
               derived_from_role, derived_from_version \
        FROM artifacts WHERE canonical_id = ? \
        ON CONFLICT(canonical_id, role, version) DO NOTHING";

    /// Merge step 3: delete all loser artifact rows after repointing.
    /// Run after [`MERGE_REPOINT`].
    /// Params: (loser_canonical_id)
    pub const MERGE_DELETE_LOSER: &str = "\
        DELETE FROM artifacts WHERE canonical_id = ?";
}

// ── export ────────────────────────────────────────────────────────────────

/// Store-wide enumeration queries backing the `/export/*` sync surface.
/// All pages are keyset-paginated on the table's primary key so a page
/// boundary never skips or repeats a row.
pub mod export {
    /// Live nodes, first page, ordered by canonical_id.
    /// Params: (limit)
    pub const NODES_FIRST: &str = "\
        SELECT canonical_id, kind FROM node WHERE merged_into IS NULL \
        ORDER BY canonical_id LIMIT ?";

    /// Live nodes, subsequent page. Cursor is the last canonical_id.
    /// Params: (last_canonical_id, limit)
    pub const NODES_PAGE: &str = "\
        SELECT canonical_id, kind FROM node \
        WHERE merged_into IS NULL AND canonical_id > ? \
        ORDER BY canonical_id LIMIT ?";

    /// Edge assertions, first page, ordered by the full PK.
    /// Params: (limit)
    pub const EDGE_ASSERTIONS_FIRST: &str = "\
        SELECT src_id, dst_id, relation, source, attrs, fetched_at \
        FROM edge_assertion \
        ORDER BY src_id, dst_id, relation, source LIMIT ?";

    /// Edge assertions, subsequent page. Cursor encodes the last row's PK.
    /// Params: (src, dst, relation, source, limit)
    pub const EDGE_ASSERTIONS_PAGE: &str = "\
        SELECT src_id, dst_id, relation, source, attrs, fetched_at \
        FROM edge_assertion \
        WHERE (src_id, dst_id, relation, source) > (?, ?, ?, ?) \
        ORDER BY src_id, dst_id, relation, source LIMIT ?";

    /// Current artifact descriptors, first page, ordered by (canonical_id, role).
    /// Same column list as the `artifact::LIST_*` queries.
    /// Params: (limit)
    pub const ARTIFACTS_FIRST: &str = "\
        SELECT canonical_id, role, version, r2_key, content_hash, byte_size, \
               mime, source, source_url, fetched_at, is_current, \
               derived_from_role, derived_from_version \
        FROM artifacts WHERE is_current = 1 \
        ORDER BY canonical_id, role LIMIT ?";

    /// Current artifact descriptors, subsequent page. Cursor encodes the last
    /// row's (canonical_id, role).
    /// Params: (last_canonical_id, last_role, limit)
    pub const ARTIFACTS_PAGE: &str = "\
        SELECT canonical_id, role, version, r2_key, content_hash, byte_size, \
               mime, source, source_url, fetched_at, is_current, \
               derived_from_role, derived_from_version \
        FROM artifacts \
        WHERE is_current = 1 AND (canonical_id, role) > (?, ?) \
        ORDER BY canonical_id, role LIMIT ?";

    /// Aliases for a page of nodes: `SELECT canonical_id, namespace, value FROM
    /// alias WHERE canonical_id IN <in_list(n)>`. Built at runtime because the
    /// id count varies; bind the page's canonical_ids in order.
    pub fn aliases_for_nodes(n: usize) -> String {
        format!(
            "SELECT canonical_id, namespace, value FROM alias WHERE canonical_id IN {}",
            crate::in_list(n)
        )
    }

    /// Assertions for a page of nodes, same shape as [`aliases_for_nodes`].
    pub fn assertions_for_nodes(n: usize) -> String {
        format!(
            "SELECT canonical_id, source, attrs, fetched_at FROM node_assertion \
             WHERE canonical_id IN {}",
            crate::in_list(n)
        )
    }
}

// ── stats ─────────────────────────────────────────────────────────────────

/// Aggregate-count queries backing the `MetadataStore::stats` summary.
/// All exclude tombstones (`merged_into IS NOT NULL`) except where noted.
/// None take parameters. Grouped queries are ordered count desc, key asc.
pub mod stats {
    /// Live work nodes. Returns column `count`. Params: none.
    pub const WORKS: &str = "\
        SELECT COUNT(*) AS count FROM node WHERE kind = 'work' AND merged_into IS NULL";

    /// Live works with at least one assertion. Returns column `count`. Params: none.
    pub const WORKS_DESCRIBED: &str = "\
        SELECT COUNT(DISTINCT na.canonical_id) AS count FROM node_assertion na \
        JOIN node n ON n.canonical_id = na.canonical_id \
        WHERE n.kind = 'work' AND n.merged_into IS NULL";

    /// All live nodes. Returns column `count`. Params: none.
    pub const NODES_TOTAL: &str = "\
        SELECT COUNT(*) AS count FROM node WHERE merged_into IS NULL";

    /// Tombstones (merged nodes). Returns column `count`. Params: none.
    pub const TOMBSTONES: &str = "\
        SELECT COUNT(*) AS count FROM node WHERE merged_into IS NOT NULL";

    /// Total deduped edges. Returns column `count`. Params: none.
    pub const EDGES_TOTAL: &str = "SELECT COUNT(*) AS count FROM edge";

    /// Live node counts by kind. Returns columns `(key, count)`. Params: none.
    pub const NODES_BY_KIND: &str = "\
        SELECT kind AS key, COUNT(*) AS count FROM node WHERE merged_into IS NULL \
        GROUP BY kind ORDER BY COUNT(*) DESC, kind ASC";

    /// Edge counts by relation. Returns columns `(key, count)`. Params: none.
    pub const EDGES_BY_RELATION: &str = "\
        SELECT relation AS key, COUNT(*) AS count FROM edge \
        GROUP BY relation ORDER BY COUNT(*) DESC, relation ASC";

    /// Node-assertion counts by source. Returns columns `(key, count)`. Params: none.
    pub const ASSERTIONS_BY_SOURCE: &str = "\
        SELECT source AS key, COUNT(*) AS count FROM node_assertion \
        GROUP BY source ORDER BY COUNT(*) DESC, source ASC";

    /// Total stored artifact bytes (the blob/R2 content footprint). Returns column
    /// `count`. Params: none.
    pub const LIBRARY_BYTES: &str =
        "SELECT COALESCE(SUM(byte_size), 0) AS count FROM artifacts";
}

// ── search ────────────────────────────────────────────────────────────────

/// Work search over assertion attrs, shared by the sqlite and D1 backends.
pub mod search {
    /// Build the work-search query for the given set of active filters.
    ///
    /// Bind params, in order: author (if `author`), title twice (if `title`),
    /// year (if `year`), then the row limit. Author and title are bound as the
    /// bare needle; the query wraps them in `%…%` itself. SQLite's default
    /// `LIKE` is already case-insensitive for ASCII.
    ///
    /// The author clause walks each assertion's JSON with `json_tree` and
    /// matches only author-name paths — `$.authorships[i].author.display_name`
    /// and `$.authorships[i].raw_author_name` (OpenAlex), `$.authors[i].name`
    /// (arXiv), `$.authors[i]` bare strings (manual) — so an institution or
    /// title string can never satisfy an author filter. `fullkey` is compared
    /// with its `"` stripped: SQLite ≥ 3.42 quotes object keys that are not
    /// simple identifiers (e.g. `."display_name"`), older versions do not.
    pub fn works(author: bool, title: bool, year: bool) -> String {
        let mut sql = String::from(
            "SELECT DISTINCT n.canonical_id FROM node n \
             WHERE n.kind = 'work' AND n.merged_into IS NULL",
        );
        if author {
            sql.push_str(
                " AND EXISTS ( \
                   SELECT 1 FROM node_assertion na, json_tree(na.attrs) jt \
                   WHERE na.canonical_id = n.canonical_id \
                     AND jt.type = 'text' \
                     AND (replace(jt.fullkey, '\"', '') LIKE '$.authorships[%].author.display_name' \
                       OR replace(jt.fullkey, '\"', '') LIKE '$.authorships[%].raw_author_name' \
                       OR replace(jt.fullkey, '\"', '') LIKE '$.authors[%') \
                     AND jt.value LIKE '%' || ? || '%' \
                 )",
            );
        }
        if title {
            sql.push_str(
                " AND EXISTS ( \
                   SELECT 1 FROM node_assertion na \
                   WHERE na.canonical_id = n.canonical_id \
                     AND (json_extract(na.attrs, '$.title') LIKE '%' || ? || '%' \
                       OR json_extract(na.attrs, '$.display_name') LIKE '%' || ? || '%') \
                 )",
            );
        }
        if year {
            sql.push_str(
                " AND EXISTS ( \
                   SELECT 1 FROM node_assertion na \
                   WHERE na.canonical_id = n.canonical_id \
                     AND json_extract(na.attrs, '$.publication_year') = ? \
                 )",
            );
        }
        sql.push_str(" ORDER BY n.canonical_id LIMIT ?");
        sql
    }
}

// ── variable-arity helpers ────────────────────────────────────────────────

// ── job ───────────────────────────────────────────────────────────────────────

/// Queries for the `fetch_jobs` background-queue table.
pub mod job {
    /// Insert a new job row; silently ignores conflicts with an active (pending/running)
    /// job for the same (kind, target_id) (get-or-create step 1).
    /// Params: (id, kind, target_id, params, run_after, created_at, updated_at)
    pub const ENQUEUE_INSERT: &str = "\
        INSERT INTO fetch_jobs (id, kind, target_id, params, state, run_after, created_at, updated_at) \
        VALUES (?, ?, ?, ?, 'pending', ?, ?, ?) \
        ON CONFLICT DO NOTHING";

    /// Resolve which job (id) is the active winner for (kind, target_id)
    /// (get-or-create step 2).
    /// Params: (kind, target_id)
    pub const ENQUEUE_SELECT: &str = "\
        SELECT id FROM fetch_jobs \
        WHERE kind = ? AND target_id = ? AND state IN ('pending', 'running') \
        ORDER BY created_at \
        LIMIT 1";

    /// Claim up to `limit` pending-and-ready jobs atomically; flips them to 'running'.
    /// Params: (updated_at, run_after_cutoff, limit)
    pub const CLAIM: &str = "\
        UPDATE fetch_jobs SET state='running', updated_at=? \
        WHERE id IN ( \
          SELECT id FROM fetch_jobs \
          WHERE state='pending' AND run_after <= ? \
          ORDER BY created_at LIMIT ? \
        ) \
        RETURNING id, kind, target_id, params, attempts";

    /// Mark a claimed job as done.
    /// Params: (updated_at, id)
    pub const COMPLETE: &str = "\
        UPDATE fetch_jobs SET state='done', updated_at=? WHERE id=?";

    /// Return a job to pending after a transient failure; increments attempts.
    /// Params: (run_after, last_error, updated_at, id)
    pub const RETRY: &str = "\
        UPDATE fetch_jobs \
        SET state='pending', attempts=attempts+1, run_after=?, last_error=?, updated_at=? \
        WHERE id=?";

    /// Mark a job as permanently failed.
    /// Params: (last_error, updated_at, id)
    pub const FAIL: &str = "\
        UPDATE fetch_jobs SET state='failed', last_error=?, updated_at=? WHERE id=?";
}

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
