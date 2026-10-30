PRAGMA foreign_keys = OFF;
BEGIN;
CREATE TABLE packages_new (
    id TEXT PRIMARY KEY NOT NULL,
    repo_slug TEXT NOT NULL,
    filetype TEXT NOT NULL CHECK (
        filetype IN (
            'AppImage',
            'Archive',
            'Compressed',
            'Binary',
            'WinExe',
            'Checksum',
            'Auto'
        )
    ),
    version_major INTEGER NOT NULL CHECK (version_major >= 0),
    version_minor INTEGER NOT NULL CHECK (version_minor >= 0),
    version_patch INTEGER NOT NULL CHECK (version_patch >= 0),
    version_is_prerelease INTEGER NOT NULL CHECK (version_is_prerelease IN (0, 1)),
    version_kind TEXT NOT NULL DEFAULT 'Semver' CHECK (
        version_kind IN ('Unknown', 'Semver', 'Datetime')
    ),
    version_value TEXT,
    release_tag TEXT,
    release_published_at TEXT,
    version_tag_template TEXT,
    channel TEXT NOT NULL CHECK (channel IN ('Stable', 'Preview', 'Nightly')),
    provider TEXT NOT NULL CHECK (
        provider IN ('Github', 'Gitlab', 'Gitea', 'WebScraper', 'Direct', 'AppImageHub')
    ),
    base_url TEXT,
    install_type TEXT NOT NULL CHECK (install_type IN ('Release', 'Build')),
    build_branch TEXT,
    build_commit TEXT,
    is_pinned INTEGER NOT NULL CHECK (is_pinned IN (0, 1)),
    icon_path TEXT,
    install_path TEXT,
    last_upgraded TEXT NOT NULL
);
INSERT INTO packages_new SELECT * FROM packages;
DROP TABLE packages;
ALTER TABLE packages_new RENAME TO packages;
PRAGMA user_version = 12;
COMMIT;
PRAGMA foreign_keys = ON;
