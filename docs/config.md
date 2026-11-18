# Configuration

[Documentation](../README.md#documentation) · [Usage guide](usage.md)

Upstream works with the default settings. To view or change them:

```bash
upstream config list
upstream config get download.high_threads
upstream config set download.high_threads=2 integration.systemd_units=false
upstream config edit
upstream config reset
```

`edit` uses `EDITOR`, then `VISUAL`, then `nano` (or Notepad on Windows).
`reset` asks for confirmation, then restores the defaults. It keeps installed packages.

## Available keys

Use the full setting name with `config get` or `config set`. Use `true` or
`false` for switches and whole numbers for counts and sizes.

| Key                                 | Default | What it controls                                                               |
| ----------------------------------- | ------- | ------------------------------------------------------------------------------ |
| `download.low_threshold_mb`         | `16`    | Download size at which to use `low_threads`.                                   |
| `download.high_threshold_mb`        | `64`    | Download size at which to use `high_threads`.                                  |
| `download.low_threads`              | `2`     | Parallel download connections between the two size thresholds.                 |
| `download.high_threads`             | `4`     | Parallel download connections at or above the higher threshold.                |
| `concurrency.check_concurrency`     | `8`     | Packages checked for updates at once.                                          |
| `concurrency.install_concurrency`   | `4`     | Package installations or upgrades processed at once.                           |
| `logging.enabled`                   | `true`  | Save diagnostic logs to `~/.upstream/upstream.log`.                            |
| `logging.level`                     | `info`  | Logging detail: `error`, `warn`, `info`, `debug`, or `trace`.                  |
| `logging.vacuum`                    | `10000` | Number of recent log lines to keep during cleanup.                             |
| `logging.max_size_mb`               | `10`    | Log size limit; older records are removed first. `0` disables this size limit. |
| `integration.systemd_units`         | `true`  | Install bundled systemd user units on Linux.                                   |
| `integration.systemd_daemon_reload` | `true`  | Refresh systemd after managed unit files change.                               |
| `integration.shell_completions`     | `true`  | Install shell completions bundled with packages.                               |

Smaller downloads use one connection; multiple connections require server
support. Lower the connection and concurrency settings to reduce resource use.
Counts below `1` use `1`.

Shell completion and systemd settings take effect when you install or replace a
package. You must enable or start systemd services yourself.

## Configuration file

Settings are saved in `~/.upstream/config.toml`. If it does not exist, Upstream
also checks your system's config directory, such as
`~/.config/upstream/config.toml` on Linux. Use `config edit` to open the file.

The file uses TOML. Include only settings you want to change; the rest use their
defaults. Unknown setting names cause an error.

For example, to reduce download connections and disable bundled systemd units:

```toml
[download]
low_threads = 1
high_threads = 2

[integration]
systemd_units = false
```

## Logging

Choose how much detail to save with `logging.level`:

| Level            | What is logged                                 |
| ---------------- | ---------------------------------------------- |
| `error`          | Failures only.                                 |
| `warn`           | Failures, warnings, and interruptions.         |
| `info` (default) | Command results, downloads, and installations. |
| `debug`          | Detailed steps for troubleshooting.            |
| `trace`          | All available diagnostic detail.               |

Each level includes the levels above it. Logs go to `~/.upstream/upstream.log`.

```bash
upstream config set logging.level=debug
upstream config set logging.enabled=false
```

Changes apply to the next command. Use `logging.vacuum` and `logging.max_size_mb`
to limit how much is kept. Set `max_size_mb=0` for no size limit. Turning off
logging still allows `crash-*.txt` reports when Upstream panics.

## API tokens

Use `auth` to manage provider tokens. The available settings are
`github.api_token`, `gitlab.api_token`, and `gitea.api_token`.

```bash
upstream auth set github.api_token=YOUR_TOKEN
upstream auth edit
upstream auth reset
```

Public repositories usually work without tokens. Adding one can help avoid API
rate limits. Tokens are saved in `~/.upstream/metadata/auth.toml` and left out of
exports.
`auth reset` clears all tokens after confirmation.

For settings that affect only one installed package, see
[package settings](advanced.md#package-settings).
