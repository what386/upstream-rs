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
repo = "https://github.com/what386/upstream-rs"
provider = "github"

desktop = false
trust = "checksum"
```

### `name`

The package name used by the registry.

```toml
name = "upstream"
```

The name should be lowercase and should match the TOML filename.

For example:

```text
packages/example-cli.toml
```

```toml
name = "example-cli"
```

### `repo`

The canonical public repository URL.

```toml
repo = "https://github.com/what386/upstream-rs"
```

Use an HTTPS URL rather than an SSH clone URL.

### `provider`

The service hosting the repository and its releases.

```toml
provider = "github"
```

The provider must be supported by Upstream.

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
repo = "https://github.com/what386/upstream-rs"
provider = "github"

desktop = false
trust = "checksum"

match = [
    "upstream-",
]

exclude = [
    "completions",
]
```

In this example, `match` and `exclude` are included only because the package requires additional guidance during asset selection.

## Contribution checklist

Before opening a pull request, confirm that:

* The filename matches `name`.
* The package name is lowercase.
* `repo` points to the canonical public repository.
* `provider` matches the repository host.
* The repository publishes prebuilt release artifacts.
* The selected trust method is supported.
* Automatic asset selection has been tested.
* `match` and `exclude` are omitted unless they are necessary.
* Any overrides are minimal and address a demonstrated problem.
* The TOML file is valid.

Run the local validator before opening a pull request:

```bash
just registry-validate
```

The committed [`index.json`](index.json) is generated as a direct mapping from package name to package metadata. Rebuild it after changing package definitions:

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
