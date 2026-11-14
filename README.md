# Upstream

Install and update software directly from upstream releases, without root access.
Supports GitHub, GitLab, Gitea, AppImageHub, download pages, direct URLs, and source builds.

## Installation

### Linux

```bash
curl -fsSL https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.bash | bash
```

### macOS

```zsh
curl -fsSL https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.zsh | zsh
```

### Windows

```powershell
iwr -useb https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.ps1 | iex
```

The Windows installer downloads the [Microsoft Visual C++ v14 Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)
if it is missing and asks for administrator approval to install it. For Cargo or
manual installations, install the redistributable for your architecture if needed.

Linux is the primary supported platform. Windows and macOS support are
experimental; macOS does not support desktop-entry creation.

### Cargo

```bash
cargo install upstream-rs
```

Cargo installations must be updated through Cargo.

### Manual installation

Download the binary from the
[latest GitHub release](https://github.com/what386/upstream-rs/releases/latest).
On Unix, make it executable with `chmod +x upstream`.

## Usage

Install a tool from its GitHub repository:

```bash
upstream install BurntSushi/ripgrep
```

If you don't know the repository, `find` lets you search and install interactively.
Commands like `find` that use search are available for github (default) and appimagehub.

```bash
upstream find terminal emulator
upstream search editor --provider appimagehub
```

### Choose what to install

Preview an installation before downloading anything, or use `probe` to choose
among the available release files yourself:

```bash
upstream install neovim/neovim --dry-run
upstream probe neovim/neovim
```

When you need a particular version, install its release tag and pin it so
routine upgrades leave it in place:

```bash
upstream install sharkdp/bat --tag v0.25.0
upstream package pin bat
```

When you're ready to update it again, run `upstream package unpin bat`.

### Review and apply updates

Check for new versions, then read what's changed before updating a tool:

```bash
upstream upgrade --check
upstream changelog ripgrep
upstream upgrade ripgrep
```

To update everything at once:

```bash
upstream upgrade
```

Use `upstream updates` to revisit the last update check without checking online
again. Pinned packages are skipped during upgrades.

### Build when a release doesn't suit your system

If a project doesn't provide a suitable download, build it from source instead.
Install Git and the project's build tools first:

```bash
upstream build BurntSushi/ripgrep
```

Source-built packages are rebuilt as needed by `upstream upgrade` too.

### Look up usage and resolve problems

Search an installed tool's README from your terminal. Once fetched, it is also
available offline:

```bash
upstream docs ripgrep usage
upstream docs ripgrep --offline
```

If an installation or update fails, review recent failures and check for
installation problems:

```bash
upstream history --status failed --since 2d
upstream doctor
```

To remove a tool, use its repository name or command name, such as
`upstream remove ripgrep` or `upstream remove rg`.

## Export and restore your setup

Save your package list and settings, then restore them on another machine:

```bash
upstream export profile ./profile.json
upstream import profile ./profile.json
```

The import reinstalls your saved versions. Add `--latest` to install current
releases instead. Profiles exclude installed binaries and provider tokens.

## API tokens

To avoid GitHub rate limits, add a token with
`upstream auth set github.api_token=<token>`.

## Documentation

- [Usage guide](docs/usage.md)
- [Configuration reference](docs/config.md)

Use `upstream <command> --help` for all options and examples.

## License

MIT OR Apache-2.0
