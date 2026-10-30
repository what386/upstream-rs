# Configuration and Storage

The CLI is the supported mutation interface. Keep deserialization strict:
unknown configuration keys must be rejected rather than silently ignored.

## Configuration

The primary config path is `$HOME/.upstream/config.toml`. Existing installations
using `$XDG_CONFIG_HOME/upstream/config.toml` remain supported. Configuration
sections and defaults are defined in `src/models/upstream/config/`.

| Section | Owns |
| --- | --- |
| `download` | Asset worker thresholds and parallelism |
| `concurrency` | Package check and install limits |
| `logging` | JSONL audit retention and severity |
| `integration` | Host integration behavior |

All integration settings default to `true`:

| Setting | Controls |
| --- | --- |
| `integration.systemd_units` | Discovering and installing bundled user units |
| `integration.shell_completions` | Discovering and installing bundled shell completions |
| `integration.systemd_daemon_reload` | Reloading systemd after managed unit files change |

Disabling discovery also skips supplementary release-asset downloads. Removal
still cleans up existing managed integrations. Desktop entries remain controlled
by `--desktop`.

`integration.systemd_daemon_reload` defaults to `true`. It controls best-effort
reloading of systemd's user manager after managed unit files are installed,
updated, or removed. Set it to `false` to reload manually:

```bash
upstream config set integration.systemd_daemon_reload=false
```

This setting does not enable, start, or restart services. Reload failures never
fail the package operation; Upstream prints a manual reload command instead.

Provider tokens are isolated in `metadata/auth.toml`; they must not be included
in config, package, or profile exports. Provider clients should receive tokens
through the authentication storage abstraction rather than reading this file
directly.

## Persistent data

| Path | Contract |
| --- | --- |
| `migration.json` | Root data-layout version and migration metadata |
| `metadata/packages.db` | Installed package state and relationships |
| `metadata/auth.toml` | Provider credentials |
| `metadata/trust.json` | Trusted minisign and cosign public keys |
| `state/symlinks/` | Active executable links |
| `cache/` | Reusable build, source, docs, and registry data |
| `temp/` | Per-operation staging data |

`packages.db` is the compatibility boundary for installed package state. Schema
migrations must be transactional and preserve canonical package IDs, managed
paths, executable aliases, and foreign keys. Do not change
identity fields without updating database mapping, filesystem paths, cache
paths, CLI resolution, exports, and integration tests together.

The schema is kept in
[`src/storage/database/schema.sql`](../src/storage/database/schema.sql). The
Rust storage layer owns SQL access; operations should not issue ad hoc queries
from command handlers.
