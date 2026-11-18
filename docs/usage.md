# Usage guide

[Documentation](../README.md#documentation) · [Configuration reference](config.md)

Use `upstream <command> --help` for the full option list. Add `--no-pager` to
print long output directly, or `--yes` to accept confirmation prompts automatically.

## Find and install

`search` lists results; `find` lets you search and choose what to install.
`probe` lets you choose a release file and install it. Add `--dry-run` to
`probe` to preview the choice without installing.

```bash
upstream search ripgrep
upstream find terminal emulator
upstream probe neovim/neovim --dry-run
upstream install BurntSushi/ripgrep
```

Repository names default to GitHub. Choose a provider with `-p`:
`github`, `gitlab`, `gitea`, or `appimagehub`. For self-hosted GitLab or Gitea,
use a full repository URL or supply `--base-url https://your-host`.

Direct download links use `-p direct`; download pages use `-p scraper`:

```bash
upstream install https://example.com/tool.tar.gz -p direct
upstream probe https://ziglang.org/download/ -p scraper
```

Useful install options:

| Option                 | Purpose                                                     |
| ---------------------- | ----------------------------------------------------------- |
| `--tag v1.2.3`         | Install a particular release tag.                           |
| `--semver 1.2.3`       | Install a specific version. Use instead of `--tag`.         |
| `--channel preview`    | Track `stable` (default), `preview`, or `nightly` releases. |
| `--kind archive`       | Choose an asset type when automatic selection needs help.   |
| `--match musl`         | Prefer assets matching a pattern.                           |
| `--exclude debug,rocm` | Filter out unwanted assets.                                 |
| `--desktop`            | Create a desktop launcher for a GUI application.            |
| `--dry-run`            | Preview the installation without downloading or installing. |

If Upstream cannot choose an executable automatically, select one from the
suggested commands or press Esc to cancel. `--yes` accepts a single candidate
but does not choose between multiple candidates.

## Build from source

To build from source, install Git and the project's build tools, then run
`upstream build owner/repo`. Use `--branch main` to follow a branch or `--tag` to
choose a release. Upstream detects how to build the project; use
`--build-profile` to choose `rust`, `go`, `zig`, `dotnet`, or `cmake` yourself.
`upstream upgrade` rebuilds these packages when their source changes.

## Updates and removal

```bash
upstream list
upstream info ripgrep
upstream upgrade --check
upstream updates
upstream upgrade ripgrep
upstream upgrade
upstream package pin ripgrep
upstream package unpin ripgrep
upstream reinstall ripgrep
upstream remove ripgrep
```

You can also use a source URL or installed command name, such as `upstream remove rg`.

`upgrade --check` checks for new releases; `updates` shows the last saved results
without checking again. Pinned packages are skipped during upgrades.
At the upgrade confirmation prompt, enter `c` to read release notes.

Removal keeps cached files. Add `--purge` to remove that package's cache
data too. Install, build, probe, upgrade, reinstall, and remove support `--dry-run`.

## Shell and desktop integration

Run `upstream hooks init`, then restart your shell so it can find installed
commands. Use `upstream hooks check` to check the setup or `upstream hooks clean`
to remove the shell hooks. Installed packages are kept.

Bundled shell completions are installed automatically. On Linux, bundled
systemd user units are also installed; enable and start them yourself with
`systemctl --user` when wanted. These features can be disabled in
[configuration](config.md).

## More options

See [Advanced usage](advanced.md) for package settings, desktop launch options,
download verification, package documentation, scripting, and backups.

For common problems, see [Troubleshooting](troubleshooting.md). Run
`upstream doctor` to check your installation or `upstream doctor --fix` to repair
common problems.
