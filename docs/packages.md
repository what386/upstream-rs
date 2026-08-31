# Package Lifecycle

Packages are tracked by a canonical provider-and-slug ID, source metadata, selected
file type, provider, channel, installed paths, and discovered executable aliases.

## Install

```bash
upstream install <repo-or-url>
```

Upstream derives the package ID from the resolved provider and source, then scans
the archive root and `bin/` directory for executable files to register as aliases.

The install flow:

1. Resolve the source and provider.
2. Fetch the target release.
3. Select the best matching asset for OS, architecture, file type, and match/exclude hints.
4. Download and optionally verify the asset.
5. Install the artifact into a managed directory.
6. Cache completion files and copy them into shell completion directories when available.
7. Optionally create desktop integration with `--desktop`.
8. Save package metadata.

Use `--dry-run` to inspect the selected release and asset before download.

## Asset Selection

The default file type is `auto`. Upstream scores release assets using filename, OS, architecture, and file-type hints. Use these options when automatic selection needs steering:

```bash
upstream install owner/repo --kind archive
upstream install owner/repo --match-pattern linux --exclude-pattern debug
```

`--match-pattern` increases preference for matching assets. `--exclude-pattern` filters out matching assets.

## Desktop Entries

Use `--desktop` for GUI applications:

```bash
upstream install owner/repo --desktop
upstream build owner/repo --desktop
```

On Linux, Upstream creates a `.desktop` file under the user applications directory and copies a discovered icon when possible. If desktop integration fails during install or upgrade, Upstream rolls back the partial package install so metadata and files remain consistent.

## Upgrade

```bash
upstream upgrade
upstream upgrade rg fd
```

Upgrade checks installed package metadata, resolves newer releases or branch heads, previews the transaction, and applies the upgrade after confirmation. Use:

```bash
upstream upgrade --check
upstream upgrade --check --machine-readable
upstream upgrade --dry-run
```

Pinned packages are skipped. Build-installed packages are rebuilt from source when upgraded.
Git source builds reuse cached workspaces under `$HOME/.upstream/cache/build/` when possible.

## Remove

```bash
upstream remove <name>
upstream remove <name> --purge
upstream uninstall <name>
```

Plain remove deletes the installed artifact and managed integrations, then removes package metadata. It preserves rollback data and app-owned config/cache/data. `--purge` also removes candidate user config/cache/data paths for the package name.

Use `--force` when files have already been manually removed and you still want metadata cleaned.

## Reinstall

```bash
upstream reinstall <name>
```

Reinstall removes the current package, then reinstalls from stored metadata. Release packages attempt the recorded version tag. Build packages rebuild from source.

## Rollback

Removal, reinstall, and upgrade flows can capture rollback artifacts. Rollback is package-name-specific and restores the latest stored artifact for each requested package.

Restore a specific package with:

```bash
upstream rollback <name>
```

Preview or prune rollback data:

```bash
upstream rollback <name> --dry-run
upstream rollback --list
upstream rollback --prune
upstream rollback --prune <name>
```

## Pinning and Aliases

```bash
upstream package pin <name>
upstream package unpin <name>
upstream package rename <old-name> <new-name>
upstream package add-entry <name>
upstream package rm-entry <name>
upstream package set <name> match_pattern=linux,x86_64 exclude_pattern=debug
upstream package set <name> trust_mode=checksum
upstream package get <name>
upstream package unset <name> trust_mode
```

Package operations accept the friendly repository name (for example, `cooltool` for `github:devguy/cooltool`). HTTP sources use the site name (for example, `ziglang` for `ziglang.org/whatever`). If that name is ambiguous, use the full provider reference. Pinned packages do not upgrade until unpinned. `package rename` changes only an executable alias and does not change package identity or reinstall the package.

Entry actions manually create or remove launcher integration for an installed package.

`package set`, `get`, and `unset` manage persistent user-controlled selection patterns and trust mode. Operation-level `--trust` flags override a stored trust mode; otherwise the stored mode is used for upgrade and reinstall.
