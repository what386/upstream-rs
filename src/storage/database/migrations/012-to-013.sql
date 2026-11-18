BEGIN TRANSACTION;
ALTER TABLE package_settings ADD COLUMN desktop_launch_options TEXT;
PRAGMA user_version = 13;
COMMIT;
