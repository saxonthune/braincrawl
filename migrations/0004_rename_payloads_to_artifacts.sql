-- Glossary alignment: the blob table `payloads` becomes `artifacts`, and its
-- `kind` column becomes `role`. Forward-only so databases that already applied
-- 0001_init (with the `payloads` name) converge on the same schema fresh installs
-- get. RENAME COLUMN requires SQLite >= 3.25.
ALTER TABLE payloads RENAME TO artifacts;
ALTER TABLE artifacts RENAME COLUMN kind TO role;
DROP INDEX IF EXISTS payloads_current;
CREATE INDEX IF NOT EXISTS artifacts_current ON artifacts (canonical_id, role) WHERE is_current = 1;
