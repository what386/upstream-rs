BEGIN;
ALTER TABLE packages ADD COLUMN version_kind TEXT NOT NULL DEFAULT 'Semver'
    CHECK (version_kind IN ('Unknown', 'Semver', 'Datetime'));
ALTER TABLE packages ADD COLUMN version_value TEXT;
PRAGMA user_version = 7;
COMMIT;
