# Upstream

Upstream is a rootless package manager for installing software directly from
upstream release sources.

It installs release artifacts from GitHub, GitLab, Gitea, AppImageHub, direct URLs, and
download pages, or builds from source when a usable binary is unavailable.

Upstream handles finding compatible release assets, installing executables into
your user environment, and tracking their sources for future upgrades. It also
supports version pinning, checksum and signature verification, desktop launchers,
shell completions, and systemd user units on Linux.

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

## Find and install software

Install directly from a repository, or search and choose a result interactively.
Use `probe` to inspect releases and compatible assets before installing.

```bash
upstream install BurntSushi/ripgrep
upstream find ripgrep
upstream search editor -p appimagehub
upstream probe neovim/neovim
```

Upstream selects assets for your platform. You can also choose a release tag,
preview an installation with `--dry-run`, or install from a direct download URL.

### Build from source

Build and install from a forge repository when you prefer a source build or a
project does not publish a suitable binary. Build profiles support Rust, Go,
Zig, .NET, and CMake projects; builds can track a release or a branch.

```bash
upstream build BurntSushi/ripgrep
upstream build owner/repo --branch main
```

## Manage installed packages

Check for updates, upgrade individual packages or everything, and pin packages
you want to keep at their installed version. `updates` shows the results of the
last update check without making network requests.

```bash
upstream list
upstream upgrade --check
upstream updates
upstream upgrade
upstream package pin ripgrep
upstream remove ripgrep
```

Upstream also provides package details, changelogs, searchable documentation,
and operation history. Failed replacements restore the previous installation.
Run `upstream doctor` to check for installation and integration problems.

## Integrate with your environment

Packages are installed without root access. Upstream manages executable links
and discovers bundled shell completions. Add `--desktop` when installing a GUI
application to create a launcher entry.

On Linux, bundled systemd user units are installed and reported after the
package is installed. Enabling and starting services remains your choice.
Integration behavior is configurable; see [configuration](docs/configuration.md).

## Export and restore your setup

Export package references, configuration, and trusted keys individually or
together as a profile, then import them on another installation.

```bash
upstream export profile ./profile.json
upstream import profile ./profile.json --latest
```

Exports contain references and metadata, rather than installed binaries or
cache contents. Provider tokens are excluded.

## API tokens

Provider tokens are optional for public repositories and can help avoid rate
limits. Add one with `upstream auth set github.api_token=<token>`.
Tokens are stored separately from configuration and excluded from exports.

## Documentation

Use `upstream --help` or `upstream <command> --help` for commands and options.
See [configuration](docs/configuration.md), [building from source](docs/build.md),
and the [developer documentation](docs/index.md) for details.

## License

MIT OR Apache-2.0
