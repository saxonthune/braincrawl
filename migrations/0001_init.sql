-- payloads: one row per stored blob; the source of truth for blob keys.
-- This migration was already applied to live databases and must not be edited:
-- it is kept in its ORIGINAL form (table `payloads`, `kind` column, `rights`
-- column). Later forward migrations evolve it — 0004 renames payloads→artifacts
-- and kind→role; 0005 drops the now-unused rights column.
CREATE TABLE payloads (
  canonical_id TEXT NOT NULL,
  kind         TEXT NOT NULL,            -- 'abstract' | 'fulltext'
  version      INTEGER NOT NULL,
  r2_key       TEXT NOT NULL,            -- {canonical_id}/{kind}/v{version}
  content_hash TEXT NOT NULL,
  byte_size    INTEGER NOT NULL,
  mime         TEXT NOT NULL,
  rights       TEXT NOT NULL,            -- removed as a concept; dropped by 0005
  source       TEXT,
  source_url   TEXT,
  fetched_at   TEXT NOT NULL,
  is_current   INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (canonical_id, kind, version)
);
CREATE INDEX payloads_current ON payloads (canonical_id, kind) WHERE is_current = 1;
