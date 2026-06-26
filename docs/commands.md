# Command Reference

Use `upstream <command> --help` for the exact help output of your installed binary. This page summarizes the command surface and the options most users need.

## Global Option

```bash
-y, --yes
```

Accept confirmation prompts. This is useful for scripts and bootstrap flows.

## Install

```bash
upstream install [options] <repo-or-url> <name>
```

Installs a package from a release source and records it for future upgrades.
The canonical form is `<repo-or-url> <name>`. For git repositories, upstream can fall back to the repository name when `<name>` is omitted. Direct URLs and scraped download pages may still require `<name>`.

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
upstream install BurntSushi/ripgrep rg -k binary
upstream install BurntSushi/ripgrep
upstream install bootandy/dust dust -k archive
upstream install neovim/neovim nvim --tag v0.11.0
upstream install owner/repo app --desktop
upstream install https://example.com/downloads tool -p scraper
```

## Build

```bash
upstream build [options] <repo-or-url> <name>
```

Builds from source and installs the resulting artifact. See [Building from source](build.md).
The canonical form is `<repo-or-url> <name>`. For git repositories, upstream can fall back to the repository name when `<name>` is omitted.

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

At the confirmation prompt, enter `c` to view release notes from the installed version to the planned upgrade target before deciding.

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
upstream uninstall [packages...] [options]
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
upstream rollback <packages...> [--dry-run]
upstream rollback --prune [packages...] [--dry-run]
upstream rollback --list
```

Manages stored rollback artifacts. Provide package names to restore their latest rollback artifacts. Use `--prune` to delete all rollback data or `--prune <packages...>` to delete selected rollback data, and `--list` to inspect available artifacts.

Options:

| Option | Meaning |
| --- | --- |
| `--dry-run` | Preview restore or prune actions |
| `--list` | List stored rollback artifacts |
| `--prune [packages...]` | Delete rollback artifacts |

## Package Metadata

```bash
upstream package pin <name>
upstream package unpin <name>
upstream package rename <old-name> <new-name>
upstream package add-entry <name>
upstream package rm-entry <name>
```

Pinning prevents upgrades. Renaming changes the local alias without reinstalling. Entry actions manually create or remove launcher integration for an installed package.

## Information Commands

```bash
upstream list [filter] [--json]
upstream info <query> [--json]
upstream changelog <name> [--from <tag|current|latest>] [--to <tag|current|latest>]
upstream docs <name> [--offline] [keywords...]
upstream docs --fetch [names...]
upstream search [query...] [-p <provider>] [--base-url <url>] [--limit <n>] [filters]
upstream find <query...> [-p <provider>] [--limit <n>] [filters] [--name <name>] [install options]
upstream probe <repo-or-url> [name] [-p <provider>] [-k <kind>] [--channel <channel>] [--limit <n>] [--verbose] [--include-incompatible]
upstream doctor [names...] [--verbose] [--fix]
upstream doctor --migrate
```

- `list` shows installed packages. Provide `[filter]` to show only package names that contain that string.
- `info` shows detailed metadata for one installed package. The query can be an exact package name or a unique substring.
- `changelog` shows release notes for installed packages. `--from` and `--to` accept release tags plus `current` for the installed version and `latest` for the tracked latest release. If `glow` is installed, changelog Markdown is rendered with glow's terminal styling.
- `docs` fetches an installed package's upstream README, caches it under upstream's cache directory, parses Markdown sections, and opens ranked keyword matches in an interactive picker. If no keywords are provided, sections are shown in README order. If `glow` is installed, previews and selected sections use glow's terminal Markdown styling. If fetching fails and a cached README exists, upstream falls back to the cached copy. Use `--offline` to search only cached documentation. Use `--fetch [names...]` to refresh cached READMEs without opening the picker; omitting names refreshes all installed packages.
- `search` searches provider repositories for software discovery. Use filters like `--language Rust`, `--topic cli`, `--min-stars 100`, `--max-stars 50000`, `--pushed-after 2026-01-01`, `--include-forks`, and `--include-archived` to narrow results.
- `find` searches provider repositories with the same discovery filters as `search`, opens an interactive picker, prompts for the package name with an inferred default, and installs the selected result. Use `--name` to skip the prompt.
- `probe` shows releases and compatible assets, opens an interactive asset picker, prompts for the package name with an inferred default when `[name]` is omitted, and installs the selected asset. When `-k/--kind` is omitted, `probe` shows all current-platform installable file types; pass `-k` to narrow the picker to one kind. Use `--include-incompatible` to show all release assets, or `--dry-run` / `--json` to inspect without installing.
- `doctor` checks paths, symlinks, hooks, completion directories, desktop entries, config, and package metadata. `--fix` repairs supported package and hook issues and removes unused config keys. Missing config keys are left omitted and continue to use defaults. `--migrate` runs local data migrations after breaking layout or metadata changes when release notes or diagnostics recommend it.

## Configuration, Import, and Export

```bash
upstream config set key=value [key=value...]
upstream config get key [key...]
upstream config list
upstream config verify
upstream config edit
upstream config reset

upstream export config <path>
upstream export packages <path>
upstream export keys <path>
upstream export profile <path>
upstream import config <path>
upstream import packages <path> [--skip-failed] [--latest]
upstream import keys <path>
upstream import profile <path> [--skip-failed] [--latest]
```

See [Configuration](configuration.md) and [Backup, import, and export](backup.md).
