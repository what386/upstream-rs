# Upstream

Upstream is a rootless package manager for installing software directly from
upstream release sources.

It installs release artifacts from GitHub, GitLab, Gitea, direct URLs, and
download pages, or builds from source when a usable binary is unavailable.

## Installation

### Binary installers

#### Linux

```bash
curl -fsSL https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.bash | bash
```

#### macOS

```zsh
curl -fsSL https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.zsh | zsh
```

#### Windows

```powershell
iwr -useb https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.ps1 | iex
```

Windows also requires the latest supported Microsoft Visual C++ v14
Redistributable. Install the package matching your architecture from
[Microsoft](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)
before running the installer.

Linux is the primary supported platform. Windows and macOS support are
experimental; macOS does not support desktop-entry creation.

### Cargo

```bash
cargo install upstream-rs
```

Cargo installs work normally, but they cannot update themselves with `upstream upgrade`.

### Manual installation

Download the binary from the
[latest GitHub release](https://github.com/what386/upstream-rs/releases/latest).
On Unix, make it executable with `chmod +x upstream`.

## Examples

```bash
upstream install BurntSushi/ripgrep
upstream install BurntSushi/ripgrep --dry-run
upstream find ripgrep
upstream upgrade
upstream upgrade --check
upstream list
upstream remove ripgrep
```

## Common workflows

### Install a release

```bash
upstream install neovim/neovim --tag v0.11.0
upstream install owner/repo --desktop
upstream install owner/repo --match-pattern linux --exclude-pattern debug
```

### Build from source

```bash
upstream build BurntSushi/ripgrep
upstream build owner/repo --branch main
upstream build owner/repo --build-profile dotnet
```

Supported build profiles are `rust`, `dotnet`, `go`, `zig`, and `cmake`.

### Manage packages

```bash
upstream upgrade nvim ripgrep
upstream reinstall ripgrep
upstream package pin nvim
upstream package rename nvim neovim
upstream package set nvim match_pattern=linux,x86_64 trust_mode=checksum
upstream cache list
upstream cache clean docs
```

### Import and export

```bash
upstream export config ./config.toml
upstream import config ./config.toml
upstream export packages ./packages.json
upstream import packages ./packages.json --latest
upstream export profile ./profile.json
upstream import profile ./profile.json --latest
upstream export keys ./keys.json
upstream import keys ./keys.json
```

Exports contain reinstallable references and metadata. They do not contain
installed files or cache contents.

## API tokens

Provider tokens are optional. They help avoid anonymous rate limits,
but are required for private repositories.

```bash
upstream auth set github.api_token=github_pat_xxx
upstream auth list
upstream doctor
```

For GitHub, create a token under **Settings > Developer settings > Personal
access tokens**. Tokens are stored separately in `auth.toml` and are not
included in configuration or profile exports.

## Documentation

See the [developer documentation](docs/index.md). For command options, use
`upstream --help` or `upstream <command> --help`.

## License

MIT OR Apache-2.0
