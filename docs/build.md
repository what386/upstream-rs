# Build Profiles

`src/services/packaging/` owns package installation; build-specific behavior is
implemented by the profile logic used by `application/operations/build_op.rs`.
The important boundary is that a profile returns a staged executable, while the
normal installer owns activation, metadata, recovery, and integrations.

Builds persist the resolved provider/repository plus the selected branch or
release tag. Package identity is the canonical provider and repository slug;
executable names are aliases discovered from the output.

## Supported profiles

| Profile | Detection source | Artifact contract |
| --- | --- | --- |
| `rust` | Cargo metadata | Cargo target directory and binary target |
| `dotnet` | MSBuild project metadata | Publish directory and evaluated `AssemblyName` |
| `go` | `go list` package metadata | Controlled output path for the selected `main` package |
| `zig` | Literal executable/install declarations in `build.zig` | `zig-out/bin/<declared executable name>` |
| `cmake` | CMake File API codemodel | Path reported by the executable target |

When detection finds multiple viable targets, fail with an ambiguity error and
require an explicit profile or target choice. Do not infer an executable from
the repository name when project metadata can provide one.

## Source cache

Git builds reuse a cached checkout under `$HOME/.upstream/cache/build/`. The
operation must reset it to the requested ref, update it only when the ref is a
moving branch, and preserve the workspace between builds so project build
systems can reuse incremental output.

Archive source workspaces live under `$HOME/.upstream/cache/source/`. Refreshes
must update source-controlled files while preserving unowned build output where
possible.

## Project scripts

After a profile build succeeds, Upstream may run a project-provided script from
the repository root or `scripts/`. Script execution is a reviewed, confirmed
operation, not part of profile detection. Install scripts are named
`install.sh`, `install.bash`, or `install.ps1`; upgrade scripts use the
corresponding `upgrade` prefix and fall back to install scripts when absent.

The staged artifact is handed to the same installer used for release assets.
That keeps replacement atomic and ensures build installs participate in the
same recovery and integration cleanup paths.

## Constraints

Upstream does not provision language toolchains or project dependencies. Keep
toolchain-specific behavior inside its profile boundary; do not add provider or
package-lifecycle workarounds to profile code.
