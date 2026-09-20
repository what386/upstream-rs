BEGIN;
CREATE TABLE path_entries_new (
    package_name TEXT PRIMARY KEY NOT NULL,
    path TEXT NOT NULL,
    position INTEGER NOT NULL,
    FOREIGN KEY (package_name) REFERENCES packages(name) ON DELETE CASCADE ON UPDATE CASCADE
);
INSERT INTO path_entries_new (package_name, path, position)
    SELECT path_entries.package_name, path_entries.path, path_entries.position
    FROM path_entries
    INNER JOIN packages ON packages.name = path_entries.package_name
    ORDER BY path_entries.position ASC;
DROP TABLE path_entries;
ALTER TABLE path_entries_new RENAME TO path_entries;
CREATE INDEX IF NOT EXISTS idx_path_entries_position
    ON path_entries(position);
PRAGMA user_version = 5;
COMMIT;
