# Troubleshooting

Start with diagnostics:

```bash
upstream doctor
upstream doctor --verbose
upstream doctor --fix
upstream doctor --migrate
```

`doctor` checks installed package paths, symlinks, shell hooks, cached completions, desktop entries, icons, and metadata. Use `--verbose` when you need individual check lines. Use `--fix` to repair supported issues such as PATH hooks, missing symlinks, executable bits, executable metadata, and cached completion drift. Use `--migrate` when local data needs a versioned layout or metadata migration.

## Migration

If `doctor` reports that local data looks like an older layout, run:

```bash
upstream doctor --migrate
```

Migration creates missing current-layout directories, moves legacy package artifacts into `$HOME/.upstream/packages/`, and rewrites affected metadata paths.

## Shell Hooks

If installed commands are not found on `PATH`:

```bash
upstream hooks check
upstream hooks init
```

Restart the shell after initializing hooks. On Unix, Upstream writes managed PATH files at:

```text
$HOME/.upstream/metadata/paths.sh
$HOME/.upstream/metadata/paths.nu
```

and sources the appropriate file from supported shell profiles.

## Stale or Missing Symlinks

Run:

```bash
upstream doctor --fix
```

If the package artifact was manually deleted, remove metadata with:

```bash
upstream remove <name> --force
```

Then reinstall.

## Stale or Missing Shell Completions

Package completions are cached under:

```text
$HOME/.upstream/cache/completions/<package>/
```

If shell completion files are missing or differ from the cached copies, run:

```bash
upstream doctor --fix
```

`doctor --fix` copies cached completions back into the supported shell completion directories.

## Bad Asset Selection

Preview before installing:

```bash
upstream install owner/repo app --dry-run
upstream probe owner/repo --verbose
```

Guide selection with:

```bash
upstream install owner/repo app --kind archive
upstream install owner/repo app --match x86_64 --exclude debug
```

## Upgrade Problems

Check what would upgrade:

```bash
upstream upgrade --check
upstream upgrade --dry-run
```

Force a reinstall/upgrade when metadata says the package is current:

```bash
upstream upgrade <name> --force
```

Pinned packages are skipped until unpinned:

```bash
upstream package unpin <name>
```

## Rollback

If an upgrade or removal captured rollback data, restore it with:

```bash
upstream rollback <name>
```

Preview first:

```bash
upstream rollback <name> --dry-run
```

Remove old rollback artifacts:

```bash
upstream rollback --prune all
```

## Build Failures

Build installs require the language toolchain and project dependencies to already work locally. If auto-detection is ambiguous:

```bash
upstream build owner/repo app --build-profile rust
```

If a project needs custom build steps that do not fit the supported profiles, use a prebuilt release asset or add project install/upgrade scripts upstream can review and run.

Git source builds use cached workspaces under `$HOME/.upstream/cache/build/`. If a cached build workspace appears corrupted, remove that package's build cache and rebuild or reinstall the package.
