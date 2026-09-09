CREATE TABLE IF NOT EXISTS fetch_jobs (
  id          TEXT PRIMARY KEY,
  kind        TEXT NOT NULL,
  target_id   TEXT NOT NULL,
  params      TEXT NOT NULL DEFAULT '{}',
  state       TEXT NOT NULL DEFAULT 'pending',
  attempts    INTEGER NOT NULL DEFAULT 0,
  run_after   TEXT NOT NULL,
  last_error  TEXT,
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS ux_fetch_jobs_active
  ON fetch_jobs(kind, target_id) WHERE state IN ('pending','running');
CREATE INDEX IF NOT EXISTS ix_fetch_jobs_claimable ON fetch_jobs(state, run_after);
