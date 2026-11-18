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

For scripts, use `upgrade --check --json` or
`upgrade --check --machine-readable` (one `name oldver newver` line per update).
`updates --json` returns saved results as JSON. `updates --notify` prints a brief
message when saved results include available updates.

## Package settings

These settings customize an installed package:

| Key                      | Purpose                                                                 |
| ------------------------ | ----------------------------------------------------------------------- |
| `match_pattern`          | Prefer asset names matching these comma-separated patterns.             |
| `exclude_pattern`        | Exclude asset names matching these comma-separated patterns.            |
| `trust_mode`             | Choose which checks to require when upgrading or reinstalling releases. |
| `desktop_launch_options` | Customize how an app opens from its desktop launcher on Linux.          |

```bash
upstream package get ripgrep
upstream package set ripgrep match_pattern=musl exclude_pattern=debug
upstream package set ripgrep trust_mode=checksum
upstream package unset ripgrep match_pattern trust_mode
```

Unsetting patterns clears them; unsetting `trust_mode` restores `best-effort`.
`--trust` overrides the saved setting for a single command.
Use `package rename OLD NEW` to change an installed executable alias.

### Desktop launch options

On Linux, use `desktop_launch_options` to add app flags or environment variables.
Include `{}` once, separated by spaces, where the app's command should go:

```bash
upstream package set obsidian 'desktop_launch_options=env FOO=bar {} --disable-gpu'
upstream package unset obsidian desktop_launch_options
```

Keep the outer quotes as shown. For values with spaces, use double quotes inside,
such as `'desktop_launch_options={} --profile "Work Profile"'`. Use `env KEY=value`
for environment variables; shell expressions such as `$HOME` and pipes are not
supported.

Changes apply to the desktop launcher immediately and are kept across upgrades.
If the app has no launcher, add one with `upstream package add-entry NAME`.
Use `unset` to restore the normal launch command. These options only affect
desktop launches.

## Download verification

Release installs, upgrades, and reinstalls support these `--trust` modes:

| Mode          | Behavior                                                                             |
| ------------- | ------------------------------------------------------------------------------------ |
| `best-effort` | Default. Check available checksums and signatures; allow missing verification files. |
| `checksum`    | Require a valid checksum.                                                            |
| `signature`   | Require a valid signature.                                                           |
| `all`         | Require both a valid checksum and signature.                                         |
| `none`        | Skip verification.                                                                   |

Failed checks stop installation, including in `best-effort` mode. Signatures
use minisign or cosign; import the publisher's public key when needed:

```bash
upstream import keys ./publisher.pub
upstream install owner/repo --trust signature
```

## Shell and desktop integration

Run `upstream hooks init`, then restart your shell so it can find installed
commands. Use `upstream hooks check` to check the setup or `upstream hooks clean`
to remove the shell hooks. Installed packages are kept.

Bundled shell completions are installed automatically. On Linux, bundled
systemd user units are also installed; enable and start them yourself with
`systemctl --user` when wanted. These features can be disabled in
[configuration](config.md).

Use `upstream package add-entry NAME` or `upstream package rm-entry NAME`
to add or remove a desktop launcher after installation. macOS does not support
desktop-entry creation.

## Documentation and troubleshooting

See [Troubleshooting](troubleshooting.md) for common problems and bug reports.

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

`doctor --fix` can fix problems with installed files, command links, and shell setup.
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
