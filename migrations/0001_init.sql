-- payloads: one row per stored blob; the source of truth for blob keys.
-- Renamed to `artifacts` (kind→role) by 0004; kept here in original form because
-- this migration was already applied to live databases and must not be edited.
CREATE TABLE payloads (
  canonical_id TEXT NOT NULL,
  kind         TEXT NOT NULL,            -- 'abstract' | 'fulltext'
  version      INTEGER NOT NULL,
  r2_key       TEXT NOT NULL,            -- {canonical_id}/{kind}/v{version}
  content_hash TEXT NOT NULL,
  byte_size    INTEGER NOT NULL,
  mime         TEXT NOT NULL,
  source       TEXT,
  source_url   TEXT,
  fetched_at   TEXT NOT NULL,
  is_current   INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (canonical_id, kind, version)
);
CREATE INDEX payloads_current ON payloads (canonical_id, kind) WHERE is_current = 1;
