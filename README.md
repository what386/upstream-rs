# Upstream

**Upstream** is a rootless package manager for installing software directly from upstream release sources.

It installs binaries, archives, AppImages, and other release artifacts from sources like GitHub, GitLab, Gitea, direct URLs, and scraped download pages. It can also build from source when prebuilt artifacts are unavailable.

## Features

* Install packages without root
* Automatically select assets for your OS and architecture
* Upgrade, remove, reinstall, and roll back packages
* Build from source using Rust, .NET, Go, Zig, or CMake
* Track `stable`, `preview`, or `nightly` channels
* Pin packages to prevent upgrades
* Create desktop entries for GUI apps
* Import/export package manifests and full snapshots
* Optional checksum and signature verification
* Shell integration hooks and diagnostics

## Installation

### Recommended

#### Linux
```bash
curl -fsSL https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.bash | bash
```

#### MacOS
```zsh
curl -fsSL https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.zsh | zsh
```

#### Windows
```ps1
iwr -useb https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.ps1 | iex
```

### Cargo

```bash
cargo install upstream-rs
```

Cargo installs do not support `upstream upgrade` self-updates.

### Manual

Download a release from:

```text
https://github.com/what386/upstream-rs/releases/latest
```

Then make it executable:

```bash
chmod +x upstream
```

## Quick Start

Initialize shell integration:

```bash
upstream hooks init
```

Install a package:

```bash
upstream install BurntSushi/ripgrep rg
```

Install a specific asset kind:

```bash
upstream install BurntSushi/ripgrep rg -k binary
```

Preview an install without changing anything:

```bash
upstream install BurntSushi/ripgrep rg --dry-run
```

Search for repositories:

```bash
upstream search ripgrep --language Rust
```

Search interactively and install a selected result:

```bash
upstream find ripgrep
```

`find` prompts for the package name after selection and defaults to the selected repository name.

Probe releases, choose an asset, and install it:

```bash
upstream probe BurntSushi/ripgrep
```

Inspect parsed releases without installing:

```bash
upstream probe BurntSushi/ripgrep --dry-run
upstream probe BurntSushi/ripgrep --json
```

Upgrade installed packages:

```bash
upstream upgrade
```

Check for available updates:

```bash
upstream upgrade --check
```

List installed packages:

```bash
upstream list
```

Remove a package:

```bash
upstream remove rg
```

Run diagnostics:

```bash
upstream doctor
```

## API Tokens

Provider API tokens are optional, but they help avoid anonymous rate limits and are required for private repositories.

Set a GitHub token with:

```bash
upstream config set github.api_token=github_pat_xxx
```

For GitHub, open your profile menu, then go to **Settings > Developer settings > Personal access tokens**.

Both of these will work:
A fine-grained personal access token with public repository access.
A classic personal access token with `read:project` permissions.

Run `upstream doctor` after configuring tokens to verify that they work.

## Common Workflows

### Install from a release source

```bash
upstream install <repo-or-url> <name>
```

The canonical form is `<repo-or-url> <name>`. For git repositories, upstream can fall back to the repository name when `<name>` is omitted. Direct URLs and scraped download pages may still require `<name>`.

Examples:

```bash
upstream install sharkdp/fd fd
upstream install BurntSushi/ripgrep
upstream install neovim/neovim nvim --tag v0.11.0
upstream install owner/repo app --desktop
```

Use `--match` and `--exclude` to guide asset selection:

```bash
upstream install owner/repo app --match linux --exclude debug
upstream install owner/repo app --match linux,x86_64 --exclude debug,symbols
```

### Build from source

```bash
upstream build <repo-or-url> <name>
```

The canonical form is `<repo-or-url> <name>`. For git repositories, upstream can fall back to the repository name when `<name>` is omitted.

Examples:

```bash
upstream build BurntSushi/ripgrep rg
upstream build BurntSushi/ripgrep
upstream build BurntSushi/ripgrep rg --branch main
upstream build owner/repo app --build-profile dotnet
```

Git source builds use cached workspaces under `.upstream/cache/build/` so rebuilds and upgrades can reuse build output when the project build system supports it.

Supported build profiles:

```text
rust
dotnet
go
zig
cmake
```

### Upgrade packages

```bash
upstream upgrade
upstream upgrade nvim rg
upstream upgrade --check
upstream upgrade --check --machine-readable
```

### Manage packages

```bash
upstream remove rg
upstream reinstall rg
upstream rollback rg
upstream rollback --list
upstream rollback --prune
upstream package pin nvim
upstream package unpin nvim
upstream package rename nvim neovim
upstream package add-entry nvim
upstream package rm-entry nvim
```

Rollback is package-name-specific. After upgrading across breaking local data changes, run `upstream doctor --migrate` when release notes or `doctor` recommend it.

### Import and export

```bash
upstream export ./packages.json
upstream import ./packages.json

upstream export ./backup.tar.gz --full
upstream import ./backup.tar.gz
```

## Command Overview

| Command     | Purpose                              |
| ----------- | ------------------------------------ |
| `install`   | Install from a release source        |
| `build`     | Build and install from source        |
| `upgrade`   | Upgrade packages                     |
| `remove`    | Remove packages                      |
| `reinstall` | Reinstall using stored metadata      |
| `rollback`  | Manage rollback artifacts            |
| `list`      | Show installed packages              |
| `changelog` | Show upstream release notes          |
| `docs`      | Search package documentation         |
| `search`    | Search provider repositories         |
| `find`      | Pick and install a search result     |
| `probe`     | Pick and install a release asset     |
| `config`    | Manage configuration                 |
| `package`   | Pin, unpin, or rename packages       |
| `hooks`     | Manage shell integration             |
| `import`    | Import keys, manifests, or snapshots |
| `export`    | Export manifests or snapshots        |
| `doctor`    | Check installation health            |

Use `-y` or `--yes` to accept confirmation prompts automatically.

## Documentation

Detailed documentation is available in [`docs/`](docs/):

* [Documentation index](docs/index.md)
* [Installation and paths](docs/installation.md)
* [Command reference](docs/commands.md)
* [Package lifecycle](docs/packages.md)
* [Building from source](docs/build.md)
* [Configuration](docs/configuration.md)
* [Trust and verification](docs/trust.md)
* [Backup, import, and export](docs/backup.md)
* [Troubleshooting](docs/troubleshooting.md)

## Notes

Upstream installs packages in user space and does not manage dependencies.

## License

MIT OR Apache-2.0
