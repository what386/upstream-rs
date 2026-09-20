BEGIN;
ALTER TABLE packages RENAME COLUMN name TO id;
ALTER TABLE patterns RENAME COLUMN package_name TO package_id;
ALTER TABLE path_entries RENAME COLUMN package_name TO package_id;
ALTER TABLE package_settings RENAME COLUMN package_name TO package_id;
ALTER TABLE package_executables RENAME COLUMN package_name TO package_id;
PRAGMA user_version = 11;
COMMIT;
