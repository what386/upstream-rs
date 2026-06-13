# Building From Source

Use `upstream build` when a project publishes source releases but does not provide a suitable prebuilt artifact for your system.

```bash
upstream build <repo-or-url> <name>
```

## Supported Profiles

Upstream can auto-detect or explicitly use these build profiles:

| Profile | Detection | Default output expectation |
| --- | --- | --- |
| `rust` | `Cargo.toml` | `target/release/<name>` |
| `dotnet` | `.sln` or `.csproj` | `.upstream-build/publish/<name>` |
| `go` | `go.mod` | `.upstream-build/<name>` |
| `zig` | `build.zig` | `zig-out/bin/<name>` |
| `cmake` | `CMakeLists.txt` | `.upstream-build/cmake/<name>` |

Force a profile when detection is ambiguous:

```bash
upstream build BurntSushi/ripgrep rg --build-profile rust
```

## Tags, Branches, and Channels

Build a release tag:

```bash
upstream build owner/repo app --tag v1.2.3
```

Build a branch head:

```bash
upstream build owner/repo app --branch main
```

Without `--tag` or `--branch`, Upstream resolves the latest release for the selected channel.

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
upstream build owner/repo app --desktop
```

If desktop integration fails, Upstream rolls back the partial install.

## Limitations

Upstream does not manage language-specific dependencies for you. The relevant build toolchain and project dependencies must be available in the environment. If a project does not build cleanly with one of the supported profiles, install from a release asset instead.
