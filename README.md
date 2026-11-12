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

Windows requires the [Microsoft Visual C++ v14 Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)
for your architecture.

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

## Find and install software

Install from a repository, find software interactively, or inspect releases with `probe`.

```bash
upstream install BurntSushi/ripgrep
upstream find ripgrep
upstream search editor -p appimagehub
upstream probe neovim/neovim
```

Use `--dry-run` to preview an installation or `--desktop` to add a GUI launcher.

### Build from source

Build from a repository using the project's installed build tools:

```bash
upstream build BurntSushi/ripgrep
upstream build owner/repo --branch main
```

## Manage installed packages

```bash
upstream list
upstream upgrade --check
upstream updates
upstream upgrade
upstream package pin ripgrep
upstream remove ripgrep
```

Run `upstream doctor` to check for installation problems.

## Export and restore your setup

Move your package list and settings to another installation:

```bash
upstream export profile ./profile.json
upstream import profile ./profile.json --latest
```

Profiles exclude installed binaries and provider tokens.

## API tokens

To avoid GitHub rate limits, add a token with
`upstream auth set github.api_token=<token>`.

## Documentation

- [Usage guide](docs/usage.md)
- [Configuration reference](docs/config.md)
- [Native Nushell completions](docs/usage.md#nushell-completions)

Use `upstream <command> --help` for all options and examples.

## License

MIT OR Apache-2.0
