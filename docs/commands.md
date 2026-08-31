# Command Reference

Use `upstream <command> --help` for the exact help output of your installed binary. This page summarizes the command surface and the options most users need.

## Global Options

```bash
-y, --yes
--no-pager
```

`--yes` accepts confirmation prompts. `--no-pager` prevents long output from opening a pager. Neither option supplies required interactive text input.

## Install

```bash
upstream install [options] <repo-or-url>
```

Installs a package from a release source and records it for future upgrades.
Upstream derives the canonical package ID from the provider and repository slug,
then discovers executable aliases from the installed artifact.

Common options:

| Option | Meaning |
| --- | --- |
| `-t, --tag <tag>` | Install a specific release tag |
| `-v, --semver <version>` | Resolve a semantic version to its repository release tag |
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
upstream install BurntSushi/ripgrep -k binary
upstream install BurntSushi/ripgrep
upstream install bootandy/dust -k archive
upstream install neovim/neovim --tag v0.11.0
upstream install sharkdp/bat --semver 0.25.0
upstream install owner/repo --desktop
upstream install https://example.com/downloads -p scraper
```

## Build

```bash
upstream build [options] <repo-or-url>
```

Builds from source and installs the resulting artifact. See [Building from source](build.md).
Build accepts GitHub, GitLab, and Gitea repository slugs or URLs. The package ID is
derived from the resolved provider and repository slug.

Common options:

| Option | Meaning |
| --- | --- |
| `-t, --tag <tag>` | Build a release tag |
| `-v, --semver <version>` | Resolve and build a semantic release version |
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
| `--json` | With `--check`, print structured update results |
| `--force` | Reinstall/upgrade even when current metadata says up to date |
| `--trust <mode>` | Verification mode for downloaded release assets |
| `--dry-run` | Preview upgrade resolution without writing |

Examples:

```bash
upstream upgrade
upstream upgrade nvim ripgrep
upstream upgrade --check
upstream upgrade --check --machine-readable
upstream upgrade ripgrep --force
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
upstream package set <name> <key=value>...
upstream package get <name> [keys...] [--json]
upstream package unset <name> [match_pattern|exclude_pattern|trust_mode]...
```

Package operations use friendly repository/site names and require the full provider reference when a name is ambiguous. Pinning prevents upgrades. Renaming changes only the local executable alias without reinstalling. Entry actions manually create or remove launcher integration for an installed package.

Package settings support `match_pattern`, `exclude_pattern`, and `trust_mode`. Patterns are comma-separated. An explicit `upgrade` or `reinstall --trust` value overrides the stored trust mode; otherwise the stored value applies, falling back to `best-effort`.

## Cache

```bash
upstream cache list [--json]
upstream cache clean [build|source|docs|registry|all]... [--dry-run]
```

`cache list` reports known cache sizes and locations. `cache clean` removes selected categories after confirmation; with no category it selects all known caches. Global `--yes` skips confirmation and `--dry-run` previews cleanup. Installed packages and rollback artifacts are never cache-cleaning targets.

## Information Commands

```bash
upstream list [filter] [--json]
upstream info <query> [--json]
upstream history [--package <name>] [--action <command>] [--status <status>] [--limit <n>] [--since <duration>|--today] [--json]
upstream changelog <name> [--for <tag>]
upstream changelog <name> [--from <tag|current|latest>] [--to <tag|current|latest>]
upstream docs <name> [--offline] [keywords...]
upstream docs --fetch [names...]
upstream search [query...] [-p <provider>] [--base-url <url>] [--limit <n>] [filters]
upstream find <query...> [-p <provider>] [--limit <n>] [filters] [install options]
upstream probe <repo-or-url> [-p <provider>] [-k <kind>] [--channel <channel>] [--limit <n>] [--include-incompatible]
upstream doctor [names...] [--verbose] [--fix] [--json]
```

- `list` shows installed packages. Provide `[filter]` to rank exact and substring matches first, followed by close fuzzy matches.
- `info` shows detailed metadata for one installed package. It requires an exact package name and suggests close or substring matches when lookup fails.
- `history` shows the latest 20 grouped operations from the JSONL history. Successful read-only commands are hidden by default. Filter with `--package`, `--action`, `--status`, or `--since 2d`; use `--today` for the local calendar day, `--limit` to change the number of operations, and `--json` for nested operation records.
- `changelog` shows release notes for installed packages. Use `--for <tag>` to show one release. `--from` and `--to` accept release tags plus `current` for the installed version and `latest` for the tracked latest release. If `glow` is installed, changelog Markdown is rendered with glow's terminal styling.
- `docs` fetches an installed package's upstream README, caches it under upstream's cache directory, parses Markdown sections, and opens ranked keyword matches in an interactive picker. If no keywords are provided, sections are shown in README order. If `glow` is installed, previews and selected sections use glow's terminal Markdown styling. If fetching fails and a cached README exists, upstream falls back to the cached copy. Use `--offline` to search only cached documentation. Use `--fetch [names...]` to refresh cached READMEs without opening the picker; omitting names refreshes all installed packages.
- `search` searches provider repositories for software discovery. Use filters like `--language Rust`, `--topic cli`, `--min-stars 100`, `--max-stars 50000`, `--pushed-after 2026-01-01`, `--include-forks`, and `--include-archived` to narrow results.
- `find` searches provider repositories with the same discovery filters as `search`, opens an interactive picker, and installs the selected result.
- `probe` shows releases and compatible assets, opens an interactive asset picker, and installs the selected asset. When `-k/--kind` is omitted, `probe` shows all current-platform installable file types; pass `-k` to narrow the picker to one kind. Use `--include-incompatible` to show all release assets, or `--dry-run` to follow the same selection and preview flow but stop before confirmation, download, installation, or metadata changes. Use `--json` for structured output.
- `doctor` checks the local layout, paths, symlinks, hooks, completion directories, desktop entries, configuration, provider tokens, and package metadata. `--verbose` prints individual findings, `--json` emits a machine-readable report, and `--fix` repairs supported package and integration issues. Invalid configuration is reported for manual correction; missing config keys continue to use defaults. Versioned local-data migrations run automatically at startup.

## Configuration, Import, and Export

```bash
upstream config set key=value [key=value...]
upstream config get key [key...]
upstream config list
upstream config edit
upstream config reset

upstream export config <path>
upstream export packages <path>
upstream export keys <path>
upstream export profile <path>
upstream import config <path>
upstream import packages <path> [--latest]
upstream import keys <path>
upstream import profile <path> [--latest]
```

See [Configuration](configuration.md) and [Backup, import, and export](backup.md).
