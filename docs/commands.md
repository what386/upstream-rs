# Command Reference

Use `upstream <command> --help` for the exact help output of your installed binary. This page summarizes the command surface and the options most users need.

## Global Option

```bash
-y, --yes
```

Accept confirmation prompts. This is useful for scripts and bootstrap flows.

## Install

```bash
upstream install [options] <name> <repo-or-url>
```

Installs a package from a release source and records it for future upgrades.

Common options:

| Option | Meaning |
| --- | --- |
| `-t, --tag <tag>` | Install a specific release tag |
| `-k, --kind <kind>` | Select asset type: `auto`, `binary`, `archive`, `compressed`, `app-image`, `mac-app`, `mac-dmg`, `win-exe`, `checksum` |
| `-p, --provider <provider>` | Use `github`, `gitlab`, `gitea`, `direct`, or `scraper` |
| `--base-url <url>` | Custom provider root for self-hosted GitLab/Gitea/etc. |
| `-c, --channel <channel>` | Track `stable`, `preview`, or `nightly` |
| `-m, --match-pattern <text>` | Prefer assets containing text |
| `-e, --exclude-pattern <text>` | Reject assets containing text |
| `-d, --desktop` | Create a desktop launcher entry |
| `--trust <mode>` | Verification mode: `none`, `best-effort`, `checksum`, `signature`, `all` |
| `--dry-run` | Resolve only; do not download or install |

Examples:

```bash
upstream install rg BurntSushi/ripgrep -k binary
upstream install dust bootandy/dust -k archive
upstream install nvim neovim/neovim --tag v0.11.0
upstream install app owner/repo --desktop
upstream install tool https://example.com/downloads -p scraper
```

## Build

```bash
upstream build [options] <name> <repo-or-url>
```

Builds from source and installs the resulting artifact. See [Building from source](build.md).

Common options:

| Option | Meaning |
| --- | --- |
| `-t, --tag <tag>` | Build a release tag |
| `--branch <branch>` | Build the current head of a branch |
| `-p, --provider <provider>` | Use a forge provider |
| `--base-url <url>` | Custom provider root |
| `-c, --channel <channel>` | Channel used for release resolution |
| `-d, --desktop` | Create a desktop launcher entry |
| `--build-profile <profile>` | Force `rust`, `dotnet`, `go`, `zig`, or `cmake` |
| `--dry-run` | Resolve only; do not compile or install |

## Upgrade

```bash
upstream upgrade [packages...] [options]
```

Upgrades all packages when no names are provided, or only the named packages otherwise.

Options:

| Option | Meaning |
| --- | --- |
| `--check` | Check for updates without applying them |
| `--machine-readable` | With `--check`, print `name oldver newver` lines |
| `--force` | Reinstall/upgrade even when current metadata says up to date |
| `--trust <mode>` | Verification mode for downloaded release assets |
| `--dry-run` | Preview upgrade resolution without writing |

Examples:

```bash
upstream upgrade
upstream upgrade nvim rg
upstream upgrade --check
upstream upgrade --check --machine-readable
upstream upgrade rg --force
```

## Remove

```bash
upstream remove [packages...] [options]
```

Options:

| Option | Meaning |
| --- | --- |
| `--purge` | Remove app-owned config/cache/data candidates too |
| `--force` | Ignore uninstall errors and remove metadata anyway |
| `--dry-run` | Preview removal |

## Reinstall

```bash
upstream reinstall [packages...] [options]
```

Reinstalls using stored package metadata. Release installs attempt the currently recorded version tag. Build installs rebuild from the recorded source.

Options:

| Option | Meaning |
| --- | --- |
| `--trust <mode>` | Verification mode for release-asset reinstalls |
| `--force` | Ignore uninstall errors before reinstalling |
| `--dry-run` | Preview reinstall resolution |

## Rollback

```bash
upstream rollback [packages...] [options]
```

Restores stored rollback artifacts. Use `--prune` to delete rollback data instead of restoring it.

Options:

| Option | Meaning |
| --- | --- |
| `--prune` | Prune rollback artifacts |
| `--dry-run` | Preview restore/prune actions |

## Package Metadata

```bash
upstream package pin <name> [reason]
upstream package unpin <name>
upstream package rename <old-name> <new-name>
```

Pinning prevents upgrades. Renaming changes the local alias without reinstalling.

## Information Commands

```bash
upstream list [name] [--json]
upstream changelog <name> [--from <tag>] [--to <tag>]
upstream search <query...> [-p <provider>] [--base-url <url>] [--limit <n>]
upstream probe <repo-or-url> [-p <provider>] [--channel <channel>] [--limit <n>] [--verbose]
upstream doctor [names...] [--verbose] [--fix]
```

- `list` shows installed package metadata.
- `changelog` shows release notes for installed packages.
- `search` searches provider repositories.
- `probe` shows releases and candidate assets without installing.
- `doctor` checks paths, symlinks, hooks, desktop entries, and package metadata.

## Configuration, Import, and Export

```bash
upstream config set key=value [key=value...]
upstream config get key [key...]
upstream config list
upstream config edit
upstream config reset

upstream export <path> [--full]
upstream import <path> [--skip-failed] [--as keys|manifest|snapshot]
```

See [Configuration](configuration.md) and [Backup, import, and export](backup.md).
