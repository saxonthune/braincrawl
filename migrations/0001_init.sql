-- artifacts: one row per stored blob; the source of truth for blob keys.
CREATE TABLE artifacts (
  canonical_id TEXT NOT NULL,
  role         TEXT NOT NULL,            -- 'abstract' | 'fulltext'
  version      INTEGER NOT NULL,
  r2_key       TEXT NOT NULL,            -- {canonical_id}/{role}/v{version}
  content_hash TEXT NOT NULL,
  byte_size    INTEGER NOT NULL,
  mime         TEXT NOT NULL,
  source       TEXT,
  source_url   TEXT,
  fetched_at   TEXT NOT NULL,
  is_current   INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (canonical_id, role, version)
);
CREATE INDEX artifacts_current ON artifacts (canonical_id, role) WHERE is_current = 1;
