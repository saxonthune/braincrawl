-- Identity, metadata, and citation-graph schema.
-- The canonical id is a braincrawl-minted GUID (doc02.01.02) and is the join key
-- across every table, including payloads (0001_init).
--
-- There are only two node-shaped tables and two edge-shaped tables: works,
-- authors, venues, concepts, and topics are all `node`s, differing only in
-- `kind`, which alias namespaces apply, and which literal fields their
-- assertions carry. Metadata uses Option A: each provider's record is stored as
-- an opaque JSON assertion and merged at read time, so per-field provenance is
-- preserved rather than baked in (doc02.01.01).

-- node: the canonical resource. A node with no assertions is a stub — a known
-- identifier with no metadata yet (first-class, per the open-world model).
-- A node with merged_into set is a tombstone: it was found to be the same
-- resource as another node and repointed to the survivor (doc02.01.02).
-- Tombstones are never deleted — consumers may still hold the old GUID — and
-- resolution follows the merged_into chain (path-compressed) to the live node.
CREATE TABLE node (
  canonical_id TEXT PRIMARY KEY,         -- braincrawl GUID
  kind         TEXT NOT NULL,            -- 'work' | 'author' | 'venue' | 'concept' | 'topic'
  created_at   TEXT NOT NULL,
  merged_into  TEXT REFERENCES node (canonical_id)   -- NULL for live nodes; survivor GUID for tombstones
);

-- alias: the namespaced multimap backing id resolution. Every external id is an
-- alias of exactly one node. UNIQUE(namespace, value) is the convergence
-- guarantee — an external id resolves to one canonical_id and can never fork.
CREATE TABLE alias (
  namespace    TEXT NOT NULL,            -- 'doi' | 'isbn' | 'oclc' | 'pmid' | 'openalex' | 'orcid' | 'issn' | 's2_corpus' | ...
  value        TEXT NOT NULL,
  canonical_id TEXT NOT NULL REFERENCES node (canonical_id),
  PRIMARY KEY (namespace, value)
);
CREATE INDEX alias_by_node ON alias (canonical_id);

-- node_assertion: one row per provider that described a node. `attrs` holds that
-- provider's literal fields verbatim; the merged view and per-field provenance
-- are computed at read time.
CREATE TABLE node_assertion (
  canonical_id TEXT NOT NULL REFERENCES node (canonical_id),
  source       TEXT NOT NULL,            -- 'openalex' | 'crossref' | 's2' | 'pubmed' | ...
  attrs        TEXT NOT NULL,            -- JSON: { title, year, type, language, ... }
  fetched_at   TEXT NOT NULL,
  PRIMARY KEY (canonical_id, source)
);

-- edge: the deduped directed relation between two nodes. Both endpoints are
-- canonical_ids; a dst that is only ever cited resolves to a stub node.
CREATE TABLE edge (
  src_id   TEXT NOT NULL REFERENCES node (canonical_id),   -- citing / subject
  dst_id   TEXT NOT NULL REFERENCES node (canonical_id),   -- cited / object (may be a stub)
  relation TEXT NOT NULL,                -- 'cites' | 'authored_by' | 'published_in' | 'has_concept' | ...
  PRIMARY KEY (src_id, dst_id, relation)
);
CREATE INDEX edge_forward  ON edge (src_id, relation);     -- references (out)
CREATE INDEX edge_backward ON edge (dst_id, relation);     -- cited_by (in)

-- edge_assertion: one row per provider claiming an edge. Same provenance split
-- as node_assertion; sparse provider-specific fields (e.g. Semantic Scholar
-- intent/context, reference position) live in `attrs`.
CREATE TABLE edge_assertion (
  src_id     TEXT NOT NULL,
  dst_id     TEXT NOT NULL,
  relation   TEXT NOT NULL,
  source     TEXT NOT NULL,              -- 'openalex' | 'crossref' | 's2' | 'opencitations' | 'pubmed'
  attrs      TEXT,                        -- JSON, nullable: { intent, context, is_influential, ref_index, ... }
  fetched_at TEXT NOT NULL,
  PRIMARY KEY (src_id, dst_id, relation, source),
  FOREIGN KEY (src_id, dst_id, relation) REFERENCES edge (src_id, dst_id, relation)
);
