# Building From Source

Use `upstream build` when a project publishes source releases but does not provide a suitable prebuilt artifact for your system.

```bash
upstream build <repo-or-url>
```

Build accepts GitHub, GitLab, and Gitea repository slugs or URLs. Upstream derives
the canonical package ID from the resolved provider and repository slug. Build
profiles inspect their project metadata to select the declared executable target
and its actual output path; the repository name is used only to disambiguate
multiple targets.

## Supported Profiles

Upstream can auto-detect or explicitly use these build profiles:

| Profile | Detection | Default output expectation |
| --- | --- | --- |
| `rust` | Cargo metadata | Cargo-reported target directory and binary target |
| `dotnet` | MSBuild project metadata | Publish directory and evaluated `AssemblyName` |
| `go` | `go list` package metadata | Controlled output path for the selected `main` package |
| `zig` | Literal executable/install declarations in `build.zig` | `zig-out/bin/<declared executable name>` |
| `cmake` | CMake File API codemodel | Artifact path reported by the executable target |

Force a profile when detection is ambiguous:

```bash
upstream build BurntSushi/ripgrep --build-profile rust
```

## Tags, Branches, and Channels

Build a release tag:

```bash
upstream build owner/repo --tag v1.2.3
```

Build a branch head:

```bash
upstream build owner/repo --branch main
```

Without `--tag` or `--branch`, Upstream resolves the latest release for the selected channel.

## Build Cache

Git source builds use cached workspaces under:

```text
$HOME/.upstream/cache/build/
```

When upgrading or rebuilding a git source package, Upstream fetches the cached repository, checks out the requested branch or release tag, pulls changes when appropriate, and rebuilds in place. This lets project build systems reuse existing build artifacts when they support incremental rebuilds.

If a source is only available as an archive, Upstream falls back to a cached source-archive workspace under:

```text
$HOME/.upstream/cache/source/
```

Archive cache refreshes update source-controlled files while preserving unowned build output where possible.

## Build Scripts

After the profile build succeeds, Upstream looks for project-provided install/upgrade scripts in the repository root or `scripts/` directory.

Install builds look for:

```text
install.sh
install.bash
install.ps1
```

Upgrade/rebuild flows prefer:

```text
upgrade.sh
upgrade.bash
upgrade.ps1
```

If no upgrade script exists, upgrade flows fall back to install scripts. Scripts are shown for review and require confirmation before execution unless `--yes` is used.

Unix shell scripts must include a shebang. PowerShell scripts run through `pwsh` when selected.

## Installation After Build

The built artifact is staged and then installed through the same package installer used for downloaded artifacts. Build-installed packages are stored with source metadata so `upgrade` and `reinstall` can rebuild them later.

Use `--desktop` for GUI apps:

```bash
upstream build owner/repo --desktop
```

If desktop integration fails, Upstream rolls back the partial install.

## Limitations

Upstream does not manage language-specific dependencies for you. The relevant build toolchain and project dependencies must be available in the environment. If a project does not build cleanly with one of the supported profiles, install from a release asset instead.
