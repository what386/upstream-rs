CREATE TABLE IF NOT EXISTS packages (
    name TEXT PRIMARY KEY NOT NULL,
    repo_slug TEXT NOT NULL,
    filetype TEXT NOT NULL CHECK (
        filetype IN (
            'AppImage',
            'MacApp',
            'MacDmg',
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
    version_tag_template TEXT,
    channel TEXT NOT NULL CHECK (channel IN ('Stable', 'Preview', 'Nightly')),
    provider TEXT NOT NULL CHECK (
        provider IN ('Github', 'Gitlab', 'Gitea', 'WebScraper', 'Direct')
    ),
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

CREATE TABLE IF NOT EXISTS patterns (
    package_name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('match', 'exclude')),
    position INTEGER NOT NULL CHECK (position >= 0),
    pattern TEXT NOT NULL,
    PRIMARY KEY (package_name, kind, position),
    FOREIGN KEY (package_name) REFERENCES packages(name) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_patterns_package_kind_position
    ON patterns(package_name, kind, position);
