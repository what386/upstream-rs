# Configuration

Configuration is stored as TOML at:

```text
$HOME/.upstream/config.toml
```

Existing installations that already use the legacy XDG path continue to load:

```text
$XDG_CONFIG_HOME/upstream/config.toml
```

Provider tokens are stored separately at `$HOME/.upstream/metadata/auth.toml`.

## Commands

```bash
upstream config list
upstream config get download.low_threads
upstream config set download.low_threads=2
upstream config edit
upstream config reset
```

`config set` accepts multiple `key=value` pairs:

```bash
upstream config set download.low_threads=2 concurrency.check_concurrency=4
```

Values are parsed as TOML literals when possible. Use normal strings for simple values, or quote strings explicitly when needed:

```bash
upstream config set rollback.compression_level=high rollback.stored_artifacts=2
```

Unknown keys are rejected when `config.toml` is loaded.

## Config Keys

| Key | Type | Default | Purpose |
| --- | --- | --- | --- |
| `download.low_threshold_mb` | integer | `16` | Minimum asset size for low parallel download worker count |
| `download.high_threshold_mb` | integer | `64` | Minimum asset size for high parallel download worker count |
| `download.low_threads` | integer | `2` | Parallel workers used at or above the low threshold |
| `download.high_threads` | integer | `4` | Parallel workers used at or above the high threshold |
| `concurrency.check_concurrency` | integer | `8` | Packages checked in parallel during `upgrade --check` and upgrade previews |
| `concurrency.install_concurrency` | integer | `4` | Packages upgraded or imported in parallel |
| `rollback.compression_level` | `none`, `low`, `high` | `high` | Compression level for rollback artifacts |
| `rollback.stored_artifacts` | integer | `1` | Number of rollback artifacts to keep per package |
| `logging.enabled` | boolean | `true` | Enable JSONL audit logging |
| `logging.level` | `error`, `warn`, `info`, `debug` | `info` | Minimum severity to write |
| `logging.vacuum` | integer | `10000` | Maximum number of log records retained during vacuuming |
| `logging.max_size_mb` | integer | `10` | Maximum JSONL log size; `0` disables the size limit |
| `registry.index_url` | string | Upstream's raw GitHub `index.min.json` URL | Registry index downloaded by `upstream add` |

Override the registry index source when using a mirror or private registry:

```bash
upstream config set registry.index_url=https://example.com/upstream/index.min.json
```

## Provider Tokens

Supported provider token keys:

```text
github.api_token
gitlab.api_token
gitea.api_token
```

Tokens are used for API requests to the corresponding provider. They are useful for private repositories, self-hosted instances, or avoiding anonymous rate limits.

Set tokens with `auth set`:

```bash
upstream auth set github.api_token=github_pat_xxx
upstream auth set gitlab.api_token=glpat_xxx
upstream auth set gitea.api_token=token_xxx
```

After configuring tokens, run:

```bash
upstream doctor
```

`doctor` validates configured provider tokens and reports invalid, rate-limited, or unreachable token checks.

### GitHub token setup

In the GitHub web UI, open your profile menu and go to:

```text
Settings > Developer settings > Personal access tokens
```

For public GitHub releases, use the smallest token that works:

- Fine-grained personal access token: grant access to the repositories Upstream needs to read.
- Personal access token (classic): an unscoped token can access public data; use the `repo` scope only when private repository access is required.

Store the copied token with:

```bash
upstream auth set github.api_token=github_pat_xxx
```

## Rollback

Rollback behavior is controlled by:

```text
rollback.compression_level = high
rollback.stored_artifacts = 1
```

`rollback.compression_level` accepts `none`, `low`, or `high`. `rollback.stored_artifacts` controls how many rollback artifacts are retained for each package.

Examples:

```bash
upstream config set rollback.compression_level=low
upstream config set rollback.stored_artifacts=3
```

## Trust Keys

Trusted signature keys are stored outside `config.toml` at:

```text
$HOME/.upstream/metadata/trust.json
```

Prefer importing key files with:

```bash
upstream import keys ./minisign.pub
upstream import keys ./cosign.pub
```

Manual edits are possible, but imports handle parsing and deduplication.

## Package Metadata

Installed package metadata is separate from configuration:

```text
$HOME/.upstream/migration.json
$HOME/.upstream/metadata/packages.db
$HOME/.upstream/metadata/auth.toml
$HOME/.upstream/metadata/trust.json
$HOME/.upstream/state/rollback/
```

- `migration.json` records the root data layout version and migration metadata.
- `packages.db` tracks installed package source, version, file type, install paths, provider metadata, and extensible per-package settings.
- `auth.toml` stores provider API tokens with restricted file permissions.
- `trust.json` stores trusted minisign and cosign public keys.
- `state/rollback/` contains rollback artifact metadata and payloads.

Do not hand-edit these files unless you are repairing a known issue. Use `package rename`, `package pin`, `package unpin`, `remove`, `reinstall`, and `rollback` where possible.
