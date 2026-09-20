BEGIN;
CREATE TABLE IF NOT EXISTS path_entries (
    path TEXT PRIMARY KEY NOT NULL,
    position INTEGER NOT NULL CHECK (position >= 0)
);
PRAGMA user_version = 3;
COMMIT;
