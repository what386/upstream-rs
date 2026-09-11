# Upstream

Upstream is a rootless package manager for installing software directly from
upstream release sources.

It fetches binaries, archives, AppImages, and other release artifacts from
GitHub, GitLab, Gitea, direct URLs, and download pages. When nobody published a
usable binary, it can build the project from source.

## What it does

* Installs software without root
* Finds the right release asset for your OS and architecture
* Installs from GitHub, GitLab, Gitea, URLs, and scraped download pages
* Upgrades, reinstalls, removes, and rolls back packages
* Builds Rust, .NET, Go, Zig, and CMake projects from source
* Tracks stable, preview, and nightly release channels
* Creates desktop entries for graphical applications
* Verifies downloads with checksums and signatures when available
* Imports and exports package lists, profiles, configuration, and trusted keys
* Keeps shell hooks, cached documentation, and diagnostics in one place

## Usage

    $ upstream
    Fetch package updates directly from the source

    Usage: upstream [OPTIONS] <COMMAND>

    Commands:
      install     Install a package from a release source
      build       Build and install a package from source
      upgrade     Upgrade installed packages
      remove      Remove installed packages
      reinstall   Reinstall packages using stored metadata
      rollback    Restore or prune rollback artifacts
      list        List installed packages
      info        Show package metadata
      search      Search provider repositories
      find        Search and interactively install a repository
      probe       Choose and install a release asset
      changelog   Show upstream release notes
      docs        Search installed package documentation
      package     Manage package settings and aliases
      cache       Inspect or clean reusable cache data
      config      Manage configuration
      auth        Manage provider API tokens
      hooks       Manage shell integration
      import      Import configuration or package data
      export      Export configuration or package data
      doctor      Check installation health

    Options:
      -y, --yes       Accept confirmation prompts
          --no-pager  Do not open long output in a pager
      -h, --help      Print help
      -V, --version   Print version

Use `upstream <command> --help` for the exact options for a command.
Most commands that change package state support `--dry-run`.

## Installation

There are a couple of ways to install Upstream.

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

Linux is the primary supported platform. Windows and macOS support are experimental.
Windows is currently more supported and tested than macOS (which is to say not at all)
MacOS is also missing some features, including desktop-entry creation.

### Cargo

```bash
cargo install upstream-rs
```

Cargo installs work normally, but they cannot update themselves with `upstream upgrade`.

### Manual installation

Download a release from the
[latest GitHub release](https://github.com/what386/upstream-rs/releases/latest),
then make the binary executable (on Unix systems):

```bash
chmod +x upstream
```

## Getting started

Initialize shell integration:

```bash
upstream hooks init
```

Install something:

```bash
upstream install BurntSushi/ripgrep
```

Preview the install first:

```bash
upstream install BurntSushi/ripgrep --dry-run
```

Find software interactively:

```bash
upstream find ripgrep
```

Choose a release asset interactively:

```bash
upstream probe BurntSushi/ripgrep
```

Upgrade everything, or check without changing anything:

```bash
upstream upgrade
upstream upgrade --check
```

List, inspect, remove, and diagnose:

```bash
upstream list
upstream info ripgrep
upstream remove ripgrep
upstream doctor
```

## Common workflows

### Install a release

```bash
upstream install sharkdp/fd
upstream install neovim/neovim --tag v0.11.0
upstream install owner/repo --desktop
upstream install owner/repo --match-pattern linux --exclude-pattern debug
```

Packages are identified by their provider and repository slug. Upstream
discovers executable aliases from the installed artifact, so a project does
not need to use the same name for its repository and its binary.

### Build from source

```bash
upstream build BurntSushi/ripgrep
upstream build owner/repo --branch main
upstream build owner/repo --build-profile dotnet
```

Supported build profiles are `rust`, `dotnet`, `go`, `zig`, and `cmake`.
Build workspaces are cached under `.upstream/cache/build/` when the project
build system allows rebuilds to reuse its output.

### Manage packages

```bash
upstream upgrade nvim ripgrep
upstream reinstall ripgrep
upstream rollback ripgrep
upstream rollback --list
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
installed files, rollback artifacts, or cache contents.

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

Developer documentation is in [`docs/`](docs/):

* [Documentation index](docs/index.md)
* [Architecture](docs/architecture.md)
* [Build profiles](docs/build.md)
* [Configuration and storage](docs/configuration.md)
* [Testing](docs/testing.md)
* [Contributing](docs/contributing.md)

## FAQ

### Why not just use my distribution's package manager?

You should use it when it has the package and version you want. Upstream is for
the gaps: small projects, portable binaries, newer versions, private
repositories, or projects that are expected to be built from source.

### Why Rust?

Because it's 🚀🚀🚀 BLAZINGLY FAST 🚀🚀🚀 and 💾💾💾 MEMORY SAFE 💾💾💾 and 🔒🔒🔒 ZERO-COST ABSTRACTIONS 🔒🔒🔒 and ⚡⚡⚡ FEARLESSLY CONCURRENT ⚡⚡⚡ an

## License

MIT OR Apache-2.0
