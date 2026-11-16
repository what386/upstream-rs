# Usage guide

[Configuration reference](config.md) · [README](../README.md)

Use `upstream <command> --help` for the full option list. Add `--no-pager` to
print long output directly, or `--yes` to accept confirmation prompts automatically.

## Find and install

`search` lists results; `find` opens an interactive search and install flow.
`probe` lets you choose a release asset and install it. Add `--dry-run` to
`probe` to preview the choice without installing.

```bash
upstream search ripgrep
upstream find terminal emulator
upstream probe neovim/neovim --dry-run
upstream install BurntSushi/ripgrep
```

Repository names default to GitHub. Select another provider with `-p`:
`github`, `gitlab`, `gitea`, or `appimagehub`. For self-hosted GitLab or Gitea,
use a full repository URL or supply `--base-url https://your-host`.

Direct download links use `-p direct`; download pages use `-p scraper`:

```bash
upstream install https://example.com/tool.tar.gz -p direct
upstream probe https://ziglang.org/download/ -p scraper
```

Useful install options:

| Option | Purpose |
| --- | --- |
| `--tag v1.2.3` | Install a particular release tag. |
| `--semver 1.2.3` | Install a specific version. Use instead of `--tag`. |
| `--channel preview` | Track `stable` (default), `preview`, or `nightly` releases. |
| `--kind archive` | Choose an asset type when automatic selection needs help. |
| `--match musl` | Prefer assets matching a pattern. |
| `--exclude debug,rocm` | Filter out unwanted assets. |
| `--desktop` | Create a desktop launcher for a GUI application. |
| `--dry-run` | Preview the installation without downloading or installing. |

If Upstream cannot choose an executable automatically, select one from the
suggested commands or press Esc to cancel. `--yes` accepts a single candidate
but does not choose between multiple candidates.

To build from source, use `upstream build owner/repo`. Add `--branch main` to
track a branch, or `--tag` to select a release. Profiles are detected automatically;
`--build-profile` can select `rust`, `go`, `zig`, `dotnet`, or `cmake`.
Install Git and the project's build tools first. Build packages are rebuilt by
`upstream upgrade` when their tracked source changes.

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

Removal keeps reusable caches. Add `--purge` to remove that package's cache
data too. Install, build, probe, upgrade, reinstall, and remove support `--dry-run`.

For scripts, use `upgrade --check --json` or
`upgrade --check --machine-readable` (one `name oldver newver` line per update).
`updates --json` returns saved results as JSON. `updates --notify` prints a brief
message when saved results include available updates.

## Package settings

These settings affect future operations on an installed package:

| Key | Purpose |
| --- | --- |
| `match_pattern` | Prefer asset names matching these comma-separated patterns. |
| `exclude_pattern` | Exclude asset names matching these comma-separated patterns. |
| `trust_mode` | Set the verification policy for future release upgrades and reinstalls. |

```bash
upstream package get ripgrep
upstream package set ripgrep match_pattern=musl exclude_pattern=debug
upstream package set ripgrep trust_mode=checksum
upstream package unset ripgrep match_pattern trust_mode
```

Unsetting patterns clears them; unsetting `trust_mode` restores `best-effort`.
A command's `--trust` option overrides the saved policy for that operation.
Use `package rename OLD NEW` to change an installed executable alias.

## Download verification

Release installs, upgrades, and reinstalls support these `--trust` modes:

| Mode | Behavior |
| --- | --- |
| `best-effort` | Default. Check available checksums and signatures; allow missing verification files. |
| `checksum` | Require a valid checksum. |
| `signature` | Require a valid signature. |
| `all` | Require both a valid checksum and signature. |
| `none` | Skip verification. |

Failed checks stop installation, including in `best-effort` mode. Signatures
use minisign or cosign; import the publisher's public key when needed:

```bash
upstream import keys ./publisher.pub
upstream install owner/repo --trust signature
```

## Shell and desktop integration

Run `upstream hooks init` to add installed commands to your shell's PATH, then
restart your shell. Use `upstream hooks check` to check the setup, or
`upstream hooks clean` to remove the hooks while keeping installed packages.

Bundled shell completions are installed automatically. On Linux, bundled
systemd user units are also installed; enable and start them yourself with
`systemctl --user` when wanted. These features can be disabled in
[configuration](config.md).

Use `upstream package add-entry NAME` or `upstream package rm-entry NAME`
to add or remove a desktop launcher after installation. macOS does not support
desktop-entry creation.

## Documentation and troubleshooting

```bash
upstream docs ripgrep usage
upstream docs --fetch ripgrep
upstream docs ripgrep --offline
upstream changelog ripgrep
upstream doctor
upstream doctor --fix
upstream cache list
upstream cache clean docs --dry-run
```

`docs` searches a package's README and saves it for offline use. Use
`docs --fetch` without names to refresh all installed packages' READMEs.
`changelog` shows newer release notes; `--for TAG` selects a single release.
If `glow` is installed, it styles Markdown output.

Check `~/.upstream/upstream.log` when troubleshooting. To save more detail for
subsequent commands:

```bash
upstream config set logging.level=debug
```

See [logging settings](config.md#logging) to adjust detail or disable logging.
If Upstream panics, it prints the path to a `crash-*.txt` report when one is saved.

`doctor --fix` repairs supported installation and integration issues.
`cache clean` accepts `updates`, `build`, `source`, `docs`, or `all`; omitting
categories cleans all known caches. Installed packages stay in place.

## Back up or move your setup

```bash
upstream export profile ./profile.json
upstream import profile ./profile.json
```

Export a profile to back up your configuration, trusted public keys, and package
list. Importing it replaces your configuration, adds trusted keys, and reinstalls
the saved releases or branches. Add `--latest` to install current releases;
branch builds keep tracking their saved branches.

Use `export config`, `export keys`, or `export packages` to save just one part,
and the matching `import` command to restore it. Exports exclude binaries,
caches, and API tokens; set tokens separately on the new installation.
