# Configuration

Configuration is stored as TOML at:

```text
$XDG_CONFIG_HOME/upstream/config.toml
```

On many Linux systems this is:

```text
$HOME/.config/upstream/config.toml
```

## Commands

```bash
upstream config list
upstream config get github.api_token
upstream config set github.api_token=ghp_xxx
upstream config edit
upstream config reset
```

`config set` accepts multiple `key=value` pairs:

```bash
upstream config set github.api_token=... gitlab.api_token=...
```

## Provider Tokens

Supported provider token keys:

```text
github.api_token
gitlab.api_token
gitea.api_token
```

Tokens are used for API requests to the corresponding provider. They are useful for private repositories, self-hosted instances, or avoiding anonymous rate limits.

## Download Concurrency

Large downloads can use multiple HTTP range requests when the server supports `Accept-Ranges: bytes`.

Default download concurrency keys:

```text
download.low_threshold_mb = 16
download.high_threshold_mb = 64
download.low_threads = 2
download.high_threads = 4
```

With the defaults, downloads under 16 MiB use one stream, downloads from 16 MiB up to 64 MiB use two streams, and downloads at or above 64 MiB use four streams.

Examples:

```bash
upstream config set download.low_threshold_mb=32
upstream config set download.high_threshold_mb=128 download.high_threads=6
```

## Trust Keys

Trusted signature keys are stored under:

```text
trust.minisign_public_keys
trust.cosign_public_keys
```

Prefer importing key files with:

```bash
upstream import ./minisign.pub --as keys
upstream import ./cosign.pub --as keys
```

Manual edits are possible through `upstream config edit`, but imports handle parsing and deduplication.

## Package Metadata

Installed package metadata is separate from configuration:

```text
$HOME/.upstream/migration.json
$HOME/.upstream/metadata/packages.json
$HOME/.upstream/metadata/metadata.json
$HOME/.upstream/metadata/transactions.json
```

- `migration.json` records the root data layout version and migration metadata.
- `packages.json` tracks installed package source, version, file type, install paths, and provider metadata.
- `metadata.json` stores sidecar package data such as pin reasons.
- `transactions.json` records mutating package operations for context-aware rollback.

Do not hand-edit these files unless you are repairing a known issue. Use `package rename`, `package pin`, `package unpin`, `remove`, `reinstall`, and `rollback` where possible.

## Editing Safely

Use `upstream config edit` for manual config changes. Use `upstream doctor` after manual repairs to check paths and metadata consistency.
