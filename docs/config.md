# Configuration

[Usage guide](usage.md) · [README](../README.md)

Defaults work without any configuration. To view or change settings:

```bash
upstream config list
upstream config get download.high_threads
upstream config set download.high_threads=2 integration.systemd_units=false
upstream config edit
upstream config reset
```

`edit` uses `EDITOR`, then `VISUAL`, then `nano` (or Notepad on Windows).
`reset` restores all defaults after confirmation; installed packages stay in place.

## Available keys

Use the full key with `config get` or `config set`. Booleans accept `true` or
`false`; counts and sizes use whole numbers.

| Key | Default | What it controls |
| --- | --- | --- |
| `download.low_threshold_mb` | `16` | Download size at which to use `low_threads`. |
| `download.high_threshold_mb` | `64` | Download size at which to use `high_threads`. |
| `download.low_threads` | `2` | Parallel download connections between the two size thresholds. |
| `download.high_threads` | `4` | Parallel download connections at or above the higher threshold. |
| `concurrency.check_concurrency` | `8` | Packages checked for updates at once. |
| `concurrency.install_concurrency` | `4` | Package installations or upgrades processed at once. |
| `logging.enabled` | `true` | Record operations for `upstream history`. |
| `logging.level` | `info` | Logging detail: `error`, `warn`, `info`, or `debug`. |
| `logging.vacuum` | `10000` | Maximum log records retained during cleanup. |
| `logging.max_size_mb` | `10` | Log size limit; older records are removed first. `0` disables this size limit. |
| `integration.systemd_units` | `true` | Install bundled systemd user units on Linux. |
| `integration.systemd_daemon_reload` | `true` | Refresh systemd after managed unit files change. |
| `integration.shell_completions` | `true` | Install shell completions bundled with packages. |

Sizes use units of 1,048,576 bytes. Smaller downloads use one connection;
parallel downloads also require server support. Connection and concurrency
counts below `1` are treated as `1`.

Integration settings apply when packages are installed or replaced. Installing
systemd units does not enable or start services.

## Configuration file

New installations use `~/.upstream/config.toml`. If that file is absent,
Upstream also recognizes an existing `upstream/config.toml` in your system's
config directory, such as `~/.config/upstream` on Linux. Use `config edit` to
open the active file.

The file uses TOML sections. Include only the settings you want to change;
omitted keys use their defaults. Unknown keys are rejected.

The full default configuration is:

```toml
[download]
low_threshold_mb = 16
high_threshold_mb = 64
low_threads = 2
high_threads = 4

[concurrency]
check_concurrency = 8
install_concurrency = 4

[logging]
enabled = true
level = "info"
vacuum = 10000
max_size_mb = 10

[integration]
systemd_units = true
systemd_daemon_reload = true
shell_completions = true
```

## API tokens

Tokens are managed separately from configuration. Supported keys are
`github.api_token`, `gitlab.api_token`, and `gitea.api_token`.

```bash
upstream auth set github.api_token=YOUR_TOKEN
upstream auth edit
upstream auth reset
```

Tokens are optional for public repositories and can help with API rate limits.
They are stored in `~/.upstream/metadata/auth.toml` and excluded from exports.
`auth reset` clears all tokens after confirmation.

For settings that affect only one installed package, see
[package settings](usage.md#package-settings).
