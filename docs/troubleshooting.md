# Troubleshooting

[Documentation](../README.md#documentation) · [Usage guide](usage.md) · [Configuration](config.md)

## Check the installation

```bash
upstream doctor --verbose
```

To check one package, use `upstream doctor NAME`. Add `--fix` to repair common
problems.

## Common problems

### An installed command is not found

Run `upstream hooks check`. If hooks are missing, run `upstream hooks init`
and restart your shell.

### Upstream picks the wrong release file

Run `upstream probe owner/repo` to see the choices and pick the right file.
Probe automatically creates matching patterns and saves them when you install,
so future upgrades can use them.

To choose manually with `upstream install`, use `--match`, `--exclude`, or `--kind`.

### Updates shows old results

`upstream updates` only shows saved results. Run `upstream upgrade --check`
to check for new releases and refresh them, or enable the systemd timer.

## Logs and crash reports

Check `~/.upstream/upstream.log` for recent activity. For more detail, turn on
debug logging and repeat the command that failed:

```bash
upstream config set logging.level=debug
```

Then return to normal logging:

```bash
upstream config set logging.level=info
```

If Upstream panics, look for `~/.upstream/crash-<timestamp>.txt`. Upstream prints
the path when it saves a report. Include the report with your bug report.

## Upgrading an older installation

If Upstream reports an unsupported installation layout or package database,
run Upstream v2.24.1 with your existing setup first, then
install the current release. Keep `~/.upstream` in place during both upgrades.

## Report a bug

Use the [bug report template](../.github/ISSUE_TEMPLATE/bug_report.yml) and include:

- Your Upstream version (`upstream --version`), operating system, and CPU architecture.
- The exact command and steps needed to reproduce the problem.
- What you expected and what actually happened.
- The error output, relevant log lines, or crash report.

Remove tokens, credentials, and other private information before sharing files.
