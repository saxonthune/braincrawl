-- Glossary alignment: the alias's first component is a `scheme` (one fixed
-- external identification format — `doi`, `openalex`, …), not a `namespace` (a
-- set of names). Rename the `alias.namespace` column to `scheme`. Forward-only,
-- so databases that already applied 0002_graph (with the `namespace` name)
-- converge on the schema fresh installs get. RENAME COLUMN rewrites the
-- PRIMARY KEY (namespace, value) automatically. Requires SQLite >= 3.25.
ALTER TABLE alias RENAME COLUMN namespace TO scheme;
