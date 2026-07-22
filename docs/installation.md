# Installation and Paths

Upstream is a rootless package manager. It installs packages and metadata under the current user's home/config directories and does not require system package-manager privileges.

## Install Methods

Recommended bootstrap scripts:

```bash
# Linux
curl -fsSL https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.bash | bash

# macOS
curl -fsSL https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.zsh | zsh

# Windows PowerShell
iwr -useb https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.ps1 | iex
```

If `$HOME/.upstream` already exists, the bootstrap script asks whether to keep or replace it. Keeping existing data reruns `upstream hooks init` and installs the managed `upstream` package only if it is not already present. Replacing removes `$HOME/.upstream` before reinitializing hooks and installing `upstream`.

For unattended installs, set `UPSTREAM_EXISTING_DATA=keep` or `UPSTREAM_EXISTING_DATA=replace`.

Cargo install:

```bash
cargo install upstream-rs
```

Cargo installs do not participate in Upstream self-updates through `upstream upgrade`.

Manual install:

1. Download an `upstream` release artifact from the GitHub releases page.
2. Put the binary somewhere on `PATH`.
3. Run `upstream hooks init`.

## Shell Integration

Initialize shell integration after installing the binary:

```bash
upstream hooks init
```

This creates Upstream-managed directories and config files, then adds shell hooks so installed package symlinks are on `PATH`.

Useful hook commands:

```bash
upstream hooks check
upstream hooks clean
upstream --yes hooks purge
```

- `hooks check` verifies directories, metadata files, and shell profile integration.
- `hooks clean` removes shell profile hooks.
- `hooks purge` removes hooks and deletes local Upstream data.

## On-Disk Layout

Upstream stores data in user-owned locations:

| Path | Purpose |
| --- | --- |
| `$HOME/.upstream/config.toml` | Main configuration file (legacy XDG config remains supported) |
| `$HOME/.upstream/migration.json` | Root layout manifest used by init, doctor, and migration |
| `$HOME/.upstream/metadata/auth.toml` | Provider API tokens |
| `$HOME/.upstream/metadata/packages.db` | Installed package metadata |
| `$HOME/.upstream/metadata/trust.json` | Trusted minisign and cosign public keys |
| `$HOME/.upstream/generated/paths.sh` | POSIX/fish PATH export managed by hooks |
| `$HOME/.upstream/generated/paths.nu` | Nushell PATH export managed by hooks |
| `$HOME/.upstream/packages/binaries/` | Installed binary artifacts |
| `$HOME/.upstream/packages/appimages/` | Installed AppImage artifacts |
| `$HOME/.upstream/packages/archives/` | Extracted archive installs |
| `$HOME/.upstream/cache/` | Reusable package cache data |
| `$HOME/.upstream/cache/build/` | Cached git workspaces for source builds |
| `$HOME/.upstream/cache/source/` | Cached source archive workspaces |
| `$HOME/.upstream/cache/registry/` | Cached package registry index used by `upstream add` |
| `$HOME/.upstream/temp/` | Temporary staging for package operations |
| `$HOME/.upstream/state/` | Persistent app state |
| `$HOME/.upstream/state/symlinks/` | Runtime command links |
| `$HOME/.upstream/state/icons/` | Stored desktop icons |
| `$HOME/.upstream/state/rollback/` | Rollback artifacts |

Desktop entries are written to `$HOME/.local/share/applications` on Linux. Shell completions are installed directly into shell-specific user completion directories when supported.

## Migration

Versioned local-data migrations run automatically when Upstream starts. Run
`upstream doctor` after an upgrade to check the migrated layout and use
`upstream doctor --fix` for supported integration repairs.

## Updating Upstream Itself

The bootstrap scripts install Upstream through Upstream after the binary is available. Once tracked as a package, `upstream upgrade upstream` or plain `upstream upgrade` can update it like other managed packages.
