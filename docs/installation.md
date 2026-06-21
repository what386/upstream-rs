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
| `$XDG_CONFIG_HOME/upstream/config.toml` | Main configuration file |
| `$HOME/.upstream/migration.json` | Root layout manifest used by init, doctor, and migration |
| `$HOME/.upstream/metadata/packages.json` | Installed package metadata |
| `$HOME/.upstream/metadata/trust.json` | Trusted minisign and cosign public keys |
| `$HOME/.upstream/metadata/rollback.json` | Rollback artifact metadata |
| `$HOME/.upstream/metadata/paths.sh` | POSIX/fish PATH export managed by hooks |
| `$HOME/.upstream/metadata/paths.nu` | Nushell PATH export managed by hooks |
| `$HOME/.upstream/packages/binaries/` | Installed binary artifacts |
| `$HOME/.upstream/packages/appimages/` | Installed AppImage artifacts |
| `$HOME/.upstream/packages/archives/` | Extracted archive installs |
| `$HOME/.upstream/cache/` | Reusable package cache data |
| `$HOME/.upstream/cache/build/` | Cached git workspaces for source builds |
| `$HOME/.upstream/cache/source-archives/` | Cached source archive workspaces |
| `$HOME/.upstream/cache/completions/` | Cached package completion scripts |
| `$HOME/.upstream/tmp/` | Temporary staging for package operations |
| `$HOME/.upstream/symlinks/` | Runtime command links |
| `$HOME/.upstream/icons/` | Stored desktop icons |
| `$HOME/.upstream/rollback/` | Rollback artifacts |

Desktop entries are written to `$HOME/.local/share/applications` on Linux. Shell completions are cached under `$HOME/.upstream/cache/completions/<package>/` and copied into shell-specific user completion directories when supported.

## Migration

After upgrading across breaking layout changes, run:

```bash
upstream migrate
```

`doctor` detects common legacy layouts and recommends `migrate` when local data appears to use an older format.

## Updating Upstream Itself

The bootstrap scripts install Upstream through Upstream after the binary is available. Once tracked as a package, `upstream upgrade upstream` or plain `upstream upgrade` can update it like other managed packages.
