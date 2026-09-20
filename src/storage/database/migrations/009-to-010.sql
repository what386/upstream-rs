BEGIN;
CREATE TABLE IF NOT EXISTS package_executables (
    package_name TEXT NOT NULL,
    path TEXT NOT NULL,
    name TEXT NOT NULL UNIQUE,
    PRIMARY KEY (package_name, path, name),
    FOREIGN KEY (package_name) REFERENCES packages(name) ON DELETE CASCADE ON UPDATE CASCADE
);
INSERT INTO package_executables (package_name, path, name)
    SELECT name, exec_path, name
    FROM packages
    WHERE exec_path IS NOT NULL;
UPDATE packages
SET name = lower(provider) || ':' || trim(repo_slug, '/');
COMMIT;
PRAGMA foreign_keys = OFF;
BEGIN;
CREATE TABLE packages_new (
    name TEXT PRIMARY KEY NOT NULL,
    repo_slug TEXT NOT NULL,
    filetype TEXT NOT NULL CHECK (filetype IN ('AppImage', 'Archive', 'Compressed', 'Binary', 'WinExe', 'Checksum', 'Auto')),
    version_major INTEGER NOT NULL CHECK (version_major >= 0),
    version_minor INTEGER NOT NULL CHECK (version_minor >= 0),
    version_patch INTEGER NOT NULL CHECK (version_patch >= 0),
    version_is_prerelease INTEGER NOT NULL CHECK (version_is_prerelease IN (0, 1)),
    version_kind TEXT NOT NULL DEFAULT 'Semver' CHECK (version_kind IN ('Unknown', 'Semver', 'Datetime')),
    version_value TEXT,
    release_tag TEXT,
    release_published_at TEXT,
    version_tag_template TEXT,
    channel TEXT NOT NULL CHECK (channel IN ('Stable', 'Preview', 'Nightly')),
    provider TEXT NOT NULL CHECK (provider IN ('Github', 'Gitlab', 'Gitea', 'WebScraper', 'Direct')),
    base_url TEXT,
    install_type TEXT NOT NULL CHECK (install_type IN ('Release', 'Build')),
    build_branch TEXT,
    build_commit TEXT,
    is_pinned INTEGER NOT NULL CHECK (is_pinned IN (0, 1)),
    icon_path TEXT,
    install_path TEXT,
    last_upgraded TEXT NOT NULL
);
INSERT INTO packages_new (
    name, repo_slug, filetype, version_major, version_minor, version_patch,
    version_is_prerelease, version_kind, version_value, release_tag,
    release_published_at, version_tag_template, channel, provider, base_url,
    install_type, build_branch, build_commit, is_pinned, icon_path,
    install_path, last_upgraded
) SELECT
    name, repo_slug, filetype, version_major, version_minor, version_patch,
    version_is_prerelease, version_kind, version_value, release_tag,
    release_published_at, version_tag_template, channel, provider, base_url,
    install_type, build_branch, build_commit, is_pinned, icon_path,
    install_path, last_upgraded
FROM packages;
DROP TABLE packages;
ALTER TABLE packages_new RENAME TO packages;
CREATE INDEX IF NOT EXISTS idx_package_executables_package
    ON package_executables(package_name, name);
PRAGMA user_version = 10;
COMMIT;
PRAGMA foreign_keys = ON;
