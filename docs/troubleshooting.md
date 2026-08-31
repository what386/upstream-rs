# Troubleshooting

## Start with diagnostics

Run the read-only checks first:

```bash
upstream doctor
upstream doctor --verbose
upstream doctor --json
```

`doctor` checks the local layout, package paths, symlinks, shell hooks, completion directories, desktop and icon files, configuration, provider tokens, and package metadata. The default output is a summary with actionable hints; `--verbose` prints every check and `--json` emits a machine-readable report. A nonzero exit status means that at least one check failed.

After reviewing the report, run the supported repairs:

```bash
upstream doctor --fix
```

`--fix` repairs supported package and integration issues, including the generated
PATH integration, missing package links, executable bits, executable metadata,
and version-tag templates. It does not repair invalid configuration, replace a
damaged Upstream installation, or modify provider credentials. Fix invalid keys
directly in `config.toml`, then rerun `upstream doctor`.

## Repair a damaged Upstream installation

Use the platform repair script when the `upstream` executable itself is missing,
cannot read its package state, or cannot repair itself. It first tries an
in-place reinstall. If that fails, it downloads the latest release, verifies its
published SHA-256 checksum, forcefully removes the broken managed `upstream`
installation, and reinstalls it before running `hooks init` and `doctor --fix`.
The forced removal targets the managed Upstream package; it keeps your existing
configuration, installed package records, package artifacts, caches, and rollback
data.

On Linux or macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/debug/repair.sh | bash
```

On Windows PowerShell:

```powershell
iwr -useb https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/debug/repair.ps1 | iex
```

The repair scripts preserve the existing `.upstream` data directory. They do not
repair arbitrary packages; use `upstream reinstall <name>` for those. Restart
shells that were launched before the repair so they receive the updated PATH.

On Windows, `upstream doctor --fix` updates the user PATH registry value. A
running shell still has its old environment, so start a new PowerShell session
after the repair.

## Microsoft Visual C++ Redistributable on Windows

The Windows release requires the latest supported Microsoft Visual C++ v14 Redistributable. Install the package matching your system architecture from [Microsoft's latest supported Visual C++ Redistributable downloads](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist), then rerun the installer or repair command. The bootstrap installer checks for this runtime before downloading the Upstream binary.

## Startup migration and legacy data

Migrations run automatically before the requested command. They create missing
current-layout directories, move legacy package artifacts into
`$HOME/.upstream/packages/`, rewrite affected metadata paths, and import older
package records when possible. If migration fails, Upstream stops before running
the command and prints the underlying error.

Do not delete legacy data or edit the package database while diagnosing a
migration failure. Preserve `$HOME/.upstream` and use the repair script only for
a damaged Upstream installation; it is not a substitute for recovering corrupt
user data.

## Shell hooks and PATH

If installed commands are not found on `PATH`:

```bash
upstream hooks check
upstream hooks init
upstream doctor --fix
```

On Unix, Upstream writes managed PATH files at:

```text
$HOME/.upstream/generated/paths.sh
$HOME/.upstream/generated/paths.nu
```

and sources the appropriate file from supported shell profiles.

## Stale or Missing Symlinks

Run:

```bash
upstream doctor --fix
```

If the package artifact was manually deleted, remove metadata with:

```bash
upstream remove <name> --force
```

Then reinstall.

## Stale or Missing Shell Completions

Package completions are installed directly into shell-specific user completion directories when supported. If shell completion files are missing or stale, reinstall the package:

```bash
upstream reinstall <name>
```

If completion directories are missing, run `upstream hooks init`.

## Bad Asset Selection

Preview before installing:

```bash
upstream install owner/repo --dry-run
upstream probe owner/repo --dry-run
```

Guide selection with:

```bash
upstream install owner/repo --kind archive
upstream install owner/repo --match-pattern x86_64 --exclude-pattern debug
```

## Upgrade Problems

Check what would upgrade:

```bash
upstream upgrade --check
upstream upgrade --dry-run
```

Force a reinstall/upgrade when metadata says the package is current:

```bash
upstream upgrade <name> --force
```

Pinned packages are skipped until unpinned:

```bash
upstream package unpin <name>
```

Upgrades and reinstalls use a temporary replacement and retain the previous
install until the new files, integrations, and metadata are committed. If a
replacement fails, Upstream removes the partial install and reports that the
previous version was restored. Check the result before retrying:

```bash
upstream doctor <name> --verbose
upstream history --package <name> --status failed
```

An interrupted operation may leave a temporary `.old` recovery copy while
cleanup finishes. Leave that file in place and rerun the command or `doctor`; it
is transaction recovery data, not the persistent artifact managed by
`upstream rollback`.

Press Ctrl-C once to request cancellation and allow cleanup or rollback to
finish. Press it a second time only when immediate termination is necessary;
that exits with status 130 and can prevent cleanup from completing.

## Rollback

If an upgrade or removal captured rollback data, restore it with:

```bash
upstream rollback <name>
```

Preview first:

```bash
upstream rollback <name> --dry-run
```

List or remove persistent rollback artifacts:

```bash
upstream rollback --list
upstream rollback --prune
```

`rollback` is separate from temporary failed-operation recovery. It restores the
latest stored artifact for a named package; `--prune` removes stored rollback
artifacts and does not repair an active installation.

## Build Failures

Build installs require the language toolchain and project dependencies to already work locally. If auto-detection is ambiguous:

```bash
upstream build owner/repo --build-profile rust
```

If a project needs custom build steps that do not fit the supported profiles, use a prebuilt release asset or add project install/upgrade scripts upstream can review and run.

Git source builds use cached workspaces under `$HOME/.upstream/cache/build/`. If a cached build workspace appears corrupted, remove that package's build cache and rebuild or reinstall the package.

## Logs and operation history

For a failed or interrupted command, inspect grouped operation history before
retrying:

```bash
upstream history --status failed
upstream history --today
upstream history --json
```

Detailed JSONL logging is stored under the Upstream data directory as
`$HOME/.upstream/log.jsonl`. Include the relevant history output and error chain
when reporting a problem, but redact tokens and private source URLs.
