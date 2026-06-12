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
| `$HOME/.upstream/metadata/packages.json` | Installed package metadata |
| `$HOME/.upstream/metadata/metadata.json` | Sidecar metadata such as pin reasons |
| `$HOME/.upstream/metadata/paths.sh` | POSIX/fish PATH export managed by hooks |
| `$HOME/.upstream/metadata/paths.nu` | Nushell PATH export managed by hooks |
| `$HOME/.upstream/binaries/` | Installed binary artifacts |
| `$HOME/.upstream/appimages/` | Installed AppImage artifacts |
| `$HOME/.upstream/archives/` | Extracted archive installs |
| `$HOME/.upstream/symlinks/` | Runtime command links |
| `$HOME/.upstream/icons/` | Stored desktop icons |
| `$HOME/.upstream/rollback/` | Rollback artifacts |

Desktop entries are written to `$HOME/.local/share/applications` on Linux. Shell completions are installed into shell-specific user completion directories when supported.

## Updating Upstream Itself

The bootstrap scripts install Upstream through Upstream after the binary is available. Once tracked as a package, `upstream upgrade upstream` or plain `upstream upgrade` can update it like other managed packages.
