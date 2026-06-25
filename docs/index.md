# Upstream Documentation

This directory contains the detailed reference material for Upstream. The README is intentionally short; use these pages when you need exact command behavior, storage locations, or workflow details.

## Pages

- [Installation and paths](installation.md): install methods, shell hooks, and on-disk layout.
- [Command reference](commands.md): full command overview with common options.
- [Package lifecycle](packages.md): install, upgrade, remove, reinstall, rollback, and pinning.
- [Building from source](build.md): source-build behavior and supported build profiles.
- [Configuration](configuration.md): config file layout, provider tokens, and package metadata.
- [Trust and verification](trust.md): checksum/signature modes and trusted key imports.
- [Backup, import, and export](backup.md): config, package, key, and profile export/import workflows.
- [Troubleshooting](troubleshooting.md): diagnostics, stale links, hooks, and common failure modes.

## Global CLI Behavior

Most commands accept the global `-y` / `--yes` flag to accept confirmation prompts. Commands that can change installed packages usually also provide `--dry-run` to resolve and preview the operation without writing files.

Use command help as the source of truth for the installed binary:

```bash
upstream --help
upstream install --help
upstream upgrade --help
```
