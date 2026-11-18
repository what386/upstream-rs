# Advanced usage

[Documentation](../README.md#documentation) · [Usage guide](usage.md) · [Configuration reference](config.md)

## Package settings

These settings customize an installed package:

| Key                      | Purpose                                                                 |
| ------------------------ | ----------------------------------------------------------------------- |
| `match_pattern`          | Prefer asset names matching these comma-separated patterns.             |
| `exclude_pattern`        | Exclude asset names matching these comma-separated patterns.            |
| `trust_mode`             | Choose which checks to require when upgrading or reinstalling releases. |
| `desktop_launch_options` | Customize how an app opens from its desktop launcher.                   |

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

Use `upstream package add-entry NAME` or `upstream package rm-entry NAME`
to add or remove a desktop launcher after installation. macOS does not support
desktop-entry creation.

On Linux and Windows, use `desktop_launch_options` to change how a desktop app opens.
Include `{}` once, separated by spaces, where the app's command should go:

```bash
upstream package set obsidian 'desktop_launch_options={} --disable-gpu'
upstream package unset obsidian desktop_launch_options
```

Keep the outer quotes as shown. For values with spaces, use double quotes inside,
such as `'desktop_launch_options={} --profile "Work Profile"'`.

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

## Package documentation and caches

```bash
upstream docs ripgrep usage
upstream docs --fetch ripgrep
upstream docs ripgrep --offline
upstream changelog ripgrep
upstream cache list
upstream cache clean docs --dry-run
```

`docs` searches a package's README and saves it for offline use. Use
`docs --fetch` without names to refresh all installed packages' READMEs.
`changelog` shows newer release notes; `--for TAG` selects a single release.
If `glow` is installed, it styles Markdown output.

`cache clean` accepts `updates`, `build`, `source`, `docs`, or `all`; omitting
categories cleans all known caches. Installed packages stay in place.

## Scripting

For scripts, use `upgrade --check --json` or
`upgrade --check --machine-readable` (one `name oldver newver` line per update).
`updates --json` returns saved results as JSON. `updates --notify` prints a brief
message when saved results include available updates.

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
