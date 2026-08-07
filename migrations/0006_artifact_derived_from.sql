-- Artifacts gain an optional parent pointer: `derived_from_role` +
-- `derived_from_version` name the (role, version) an artifact was produced
-- from, e.g. a `chunks` artifact derived from `fulltext` v1. Both NULL means
-- a root artifact (nothing derived it — a user-supplied upload, an original
-- fetch). No FOREIGN KEY: the parent may later be superseded or removed, and
-- a dangling pointer is a display concern, not a write-time error. Existing
-- rows predate this concept and get NULL on both columns, so they read as
-- roots — no backfill guess is made about their lineage.
ALTER TABLE artifacts ADD COLUMN derived_from_role TEXT;
ALTER TABLE artifacts ADD COLUMN derived_from_version INTEGER;
