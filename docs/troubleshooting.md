# Troubleshooting

Start with diagnostics:

```bash
upstream doctor
upstream doctor --verbose
upstream doctor --fix
```

`doctor` checks installed package paths, symlinks, shell hooks, desktop entries, icons, and metadata. Use `--verbose` when you need individual check lines.

## Shell Hooks

If installed commands are not found on `PATH`:

```bash
upstream hooks check
upstream hooks init
```

Restart the shell after initializing hooks. On Unix, Upstream writes a managed PATH file at:

```text
$HOME/.upstream/metadata/paths.sh
```

and sources it from supported shell profiles.

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

## Bad Asset Selection

Preview before installing:

```bash
upstream install app owner/repo --dry-run
upstream probe owner/repo --verbose
```

Guide selection with:

```bash
upstream install app owner/repo --kind archive
upstream install app owner/repo --match x86_64 --exclude debug
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
upstream build app owner/repo --build-profile rust
```

If a project needs custom build steps that do not fit the supported profiles, use a prebuilt release asset or add project install/upgrade scripts upstream can review and run.
