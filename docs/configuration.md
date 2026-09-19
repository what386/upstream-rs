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
