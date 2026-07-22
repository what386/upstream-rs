# Contributing Packages

Thank you for contributing to the Upstream package registry.

Each package is defined by a TOML file in the [`packages/`](packages/) directory.

## Adding a package

1. Fork this repository.

2. Create a file named:

   ```text
   packages/<package-name>.toml
   ```

3. Add the package metadata.

4. Verify that Upstream can detect the correct release assets.

5. Open a pull request.

## Basic package format

Most packages only need the following fields:

```toml
name = "upstream"
revision = 1
desktop = false
trust = "checksum"

[install]
type = "release"
repo = "https://github.com/what386/upstream-rs"
provider = "github"
```

### `name`

The package name used by the registry.

```toml
name = "upstream"
```

The name must match the TOML filename. Lowercase, hyphen-separated names are preferred for
consistent lookup (for example, `visual-studio-code`), but uppercase letters and spaces are
supported when the package's established name requires them.

For example:

```text
packages/example-cli.toml
```

```toml
name = "example-cli"
```

### `binary`

Use the optional `binary` field when the primary installed command differs from the registry name:

```toml
name = "ripgrep"
binary = "rg"
```

With this entry, `upstream add ripgrep` installs and manages the package as `rg`. Commands such as `upstream upgrade rg` and `upstream remove rg` use the binary name. Omit `binary` when it is the same as `name`.

The value must be a command basename without a directory or platform-specific extension such as
`.exe`. It may contain uppercase letters and spaces, matching the installed executable exactly.
Effective installed names must be unique across the registry.

### `revision`

Registry revisions start at `1` and must increase by exactly one whenever a package definition changes:

```toml
revision = 1
```

Do not increment the revision when the package metadata is unchanged. CI compares changed entries with the pull request base and enforces the revision.

## Install recipes

Every entry has an `[install]` table whose `type` selects how Upstream obtains the package.

### Release installs

Use `release` for prebuilt assets published by a supported forge:


```toml
[install]
type = "release"
repo = "https://github.com/what386/upstream-rs"
provider = "github"
```

`repo` is the canonical public HTTPS repository URL. `provider` must be `github`, `gitlab`, or
`gitea`. Release installs support the package-level `match` and `exclude` asset-selection hints.

### Build installs

Use `build` when the package should be compiled from source:

```toml
[install]
type = "build"
repo = "https://github.com/owner/project"
provider = "github"
profile = "rust"
```

The optional `profile` may be `rust`, `dotnet`, `go`, `zig`, or `cmake`. Omit it to let Upstream
detect the build system. An optional `branch` selects a branch instead of the latest stable release:

```toml
branch = "main"
```

Build installs do not support `match` or `exclude`.

### Direct HTTP installs

Use `http` for an artifact available at a direct HTTPS URL:

```toml
[install]
type = "http"
url = "https://downloads.example.com/example-tool-linux-amd64.tar.gz"
filetype = "archive"
```

`filetype` defaults to `auto`. Supported explicit values are `appimage`, `mac-app`, `mac-dmg`,
`archive`, `compressed`, `binary`, and `win-exe`. The URL may redirect, but it must not contain
credentials or a fragment. Direct URLs should remain stable so upgrades can retrieve newer content.

### `desktop`

Whether the package is a graphical desktop application.

```toml
desktop = false
```

Use `true` for desktop applications and `false` for command-line tools or other non-desktop packages.
This will generate a .desktop (linux) or start menu entry (windows) for the application when set to `true`.

### `trust`

The method used to verify downloaded artifacts.

```toml
trust = "checksum"
```

Only select a trust method that is supported by the package’s releases.

## Asset selection overrides

Upstream automatically identifies the appropriate release assets for most packages.

Do not add `match` or `exclude` unless automatic selection chooses the wrong files or cannot identify a valid artifact.
You should likely also open an issue unless upstream has a good reason to fail autodetection.

### `match`

Provides an additional hint for selecting release assets.

```toml
match = [
    "upstream-",
]
```

Use this only when valid release files share a distinctive string that Upstream does not detect automatically.

### `exclude`

Prevents known incorrect assets from being selected.

```toml
exclude = [
    "completions",
]
```

This can be useful for files such as:

* Shell completion archives
* Documentation bundles
* Debug builds
* Auxiliary files
* Other assets that resemble installable releases

Keep overrides as narrow as possible.

## Complete example with overrides

```toml
name = "upstream"
revision = 1
desktop = false
trust = "checksum"

match = [
    "upstream-",
]

exclude = [
    "completions",
]

[install]
type = "release"
repo = "https://github.com/what386/upstream-rs"
provider = "github"
```

In this example, `match` and `exclude` are included only because the package requires additional guidance during asset selection.

## Contribution checklist

Before opening a pull request, confirm that:

* The filename matches `name`.
* The package name uses lowercase, hyphen-separated words unless its established name requires otherwise.
* `binary` is omitted unless the installed command differs from `name`.
* New packages use `revision = 1`; modified packages increment their revision by one.
* `[install]` contains exactly the fields supported by its `type`.
* Release and build repositories use canonical public HTTPS URLs and matching providers.
* Direct HTTP URLs are stable HTTPS download locations.
* The selected trust method is supported.
* Automatic asset selection has been tested.
* `match` and `exclude` are omitted unless they are necessary.
* Any overrides are minimal and address a demonstrated problem.
* The TOML file is valid.

Run the local validator before opening a pull request:

```bash
just registry-validate
```

The committed [`index.json`](index.json) and [`index.min.json`](index.min.json) are generated from the package definitions. The minified index contains the same data without formatting whitespace and is intended for clients. Rebuild both after changing package definitions:

```bash
just registry-index
```

## Pull requests

Keep each pull request focused on a single package when practical.

Include:

* The package name
* A link to the repository
* A link to an example release
* A brief explanation of any `match` or `exclude` overrides
