BEGIN;
CREATE TABLE IF NOT EXISTS package_settings (
    package_name TEXT PRIMARY KEY NOT NULL,
    trust_mode TEXT CHECK (
        trust_mode IS NULL OR trust_mode IN (
            'None', 'BestEffort', 'Checksum', 'Signature', 'All'
        )
    ),
    FOREIGN KEY (package_name) REFERENCES packages(name) ON DELETE CASCADE ON UPDATE CASCADE
);
PRAGMA user_version = 6;
COMMIT;
