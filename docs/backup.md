# Backup, Import, and Export

Upstream supports three portable export types:

- A config export that records upstream's TOML configuration.
- A packages export that records installed package sources and release version tags.
- A keys export that records trusted minisign and cosign public keys.
- A profile export that bundles config, packages, and keys.

## Export Config

```bash
upstream export config ./config.toml
```

Import config on another machine:

```bash
upstream import config ./config.toml
```

Config imports replace the current upstream config file after validating the exported TOML.

## Export Packages

```bash
upstream export packages ./packages.json
```

The packages export is intended for migration or replication. It does not contain installed binaries, executable paths, icons, rollback data, or local cache contents.

Import it on another machine:

```bash
upstream import packages ./packages.json
```

Package imports install release packages at the version tags recorded in the export and rebuild build packages from their recorded release tag or branch. Use `--latest` to ignore stored release version tags and install each release package's latest release:

```bash
upstream import packages ./packages.json --latest
```

Package installs run concurrently and use `concurrency.install_concurrency` (default `4`) as their limit. Profile imports use the same limit after applying the profile's config.

Build-installed packages are rebuilt from their recorded release tag or branch during import. Build artifacts themselves are not exported.

Package exports also preserve match/exclude selection patterns and an optional stored trust mode. During import, the stored trust mode verifies the package before it is persisted.

## Export Trusted Keys

```bash
upstream export keys ./keys.json
```

Import keys on another machine:

```bash
upstream import keys ./keys.json
```

Key imports merge into `$HOME/.upstream/metadata/trust.json` and deduplicate existing keys.

## Export a Profile

```bash
upstream export profile ./profile.json
```

Import a profile on another machine:

```bash
upstream import profile ./profile.json
```

Profile imports apply config first, merge trusted keys second, and install release and build packages last. Use `--latest` to ignore stored package version tags:

```bash
upstream import profile ./profile.json --latest
```

Profiles are portable restore bundles. They do not include installed artifacts, rollback data, or cache contents.

## Partial Failures

For package and profile imports, `--skip-failed` continues processing remaining packages if an individual package install fails:

```bash
upstream import packages ./packages.json --skip-failed
upstream import profile ./profile.json --skip-failed
```
