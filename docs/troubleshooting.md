# Troubleshooting

Start with diagnostics:

```bash
upstream doctor
upstream doctor --verbose
upstream doctor --fix
```

`doctor` checks installed package paths, symlinks, shell hooks, completion directories, desktop entries, icons, config, and metadata. Use `--verbose` when you need individual check lines. Use `--fix` to repair supported issues such as PATH hooks, missing symlinks, executable bits, executable metadata, and unused config keys. Versioned local-data migrations run automatically at startup.

## Migration

Startup migration creates missing current-layout directories, moves legacy
package artifacts into `$HOME/.upstream/packages/`, and rewrites affected
metadata paths. If a migration cannot complete, Upstream exits with the
underlying error before running the requested command.

## Shell Hooks

If installed commands are not found on `PATH`:

```bash
upstream hooks check
upstream hooks init
```

Restart the shell after initializing hooks. On Unix, Upstream writes managed PATH files at:

```text
$HOME/.upstream/generated/paths.sh
$HOME/.upstream/generated/paths.nu
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

Package completions are installed directly into shell-specific user completion directories when supported. If shell completion files are missing or stale, reinstall the package:

```bash
upstream reinstall <name>
```

If completion directories are missing, run `upstream hooks init`.

## Bad Asset Selection

Preview before installing:

```bash
upstream install owner/repo app --dry-run
upstream probe owner/repo --dry-run
```

Guide selection with:

```bash
upstream install owner/repo app --kind archive
upstream install owner/repo app --match-pattern x86_64 --exclude-pattern debug
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
upstream rollback --prune
```

## Build Failures

Build installs require the language toolchain and project dependencies to already work locally. If auto-detection is ambiguous:

```bash
upstream build owner/repo app --build-profile rust
```

If a project needs custom build steps that do not fit the supported profiles, use a prebuilt release asset or add project install/upgrade scripts upstream can review and run.

Git source builds use cached workspaces under `$HOME/.upstream/cache/build/`. If a cached build workspace appears corrupted, remove that package's build cache and rebuild or reinstall the package.
