BEGIN;
ALTER TABLE packages ADD COLUMN version_tag_template TEXT;
PRAGMA user_version = 2;
COMMIT;
