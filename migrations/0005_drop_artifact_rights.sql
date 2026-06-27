-- `rights` was removed as a concept. The column was created by 0001_init and
-- carried through 0004's rename, so it lingers on already-initialized databases
-- even though the code no longer writes it (a push INSERT omits it, which a
-- NOT NULL column rejects). Drop it. Runs after 0004, so the table is `artifacts`
-- by now on both fresh and existing databases. DROP COLUMN requires SQLite >= 3.35.
ALTER TABLE artifacts DROP COLUMN rights;
