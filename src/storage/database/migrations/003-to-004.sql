BEGIN;
CREATE TABLE patterns_new (
    package_name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('match', 'exclude')),
    position INTEGER NOT NULL CHECK (position >= 0),
    pattern TEXT NOT NULL,
    PRIMARY KEY (package_name, kind, position),
    FOREIGN KEY (package_name) REFERENCES packages(name) ON DELETE CASCADE ON UPDATE CASCADE
);
INSERT INTO patterns_new (package_name, kind, position, pattern)
    SELECT patterns.package_name, patterns.kind, patterns.position, patterns.pattern
    FROM patterns
    INNER JOIN packages ON packages.name = patterns.package_name;
DROP TABLE patterns;
ALTER TABLE patterns_new RENAME TO patterns;
CREATE INDEX IF NOT EXISTS idx_patterns_package_kind_position
    ON patterns(package_name, kind, position);

DROP TABLE IF EXISTS path_entries;
CREATE TABLE path_entries (
    package_name TEXT PRIMARY KEY NOT NULL,
    path TEXT NOT NULL,
    position INTEGER NOT NULL CHECK (position >= 0),
    FOREIGN KEY (package_name) REFERENCES packages(name) ON DELETE CASCADE ON UPDATE CASCADE
);
PRAGMA user_version = 4;
COMMIT;
