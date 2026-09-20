PRAGMA foreign_keys = OFF;
BEGIN;
DELETE FROM packages WHERE filetype IN ('MacApp', 'MacDmg');
UPDATE packages SET version_kind = 'Semver' WHERE version_kind IS NULL;
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
    exec_path TEXT,
    last_upgraded TEXT NOT NULL
);
INSERT INTO packages_new (
    name, repo_slug, filetype, version_major, version_minor, version_patch,
    version_is_prerelease, version_kind, version_value, release_tag,
    release_published_at, version_tag_template, channel, provider, base_url,
    install_type, build_branch, build_commit, is_pinned, icon_path,
    install_path, exec_path, last_upgraded
) SELECT
    name, repo_slug, filetype, version_major, version_minor, version_patch,
    version_is_prerelease, COALESCE(version_kind, 'Semver'), version_value,
    release_tag, release_published_at, version_tag_template, channel, provider,
    base_url, install_type, build_branch, build_commit, is_pinned, icon_path,
    install_path, exec_path, last_upgraded
FROM packages;
DROP TABLE packages;
ALTER TABLE packages_new RENAME TO packages;
PRAGMA user_version = 9;
COMMIT;
PRAGMA foreign_keys = ON;
