# Troubleshooting

[Documentation](../README.md#documentation) · [Usage guide](usage.md) · [Configuration](config.md)

## Start here

Start with:

```bash
upstream doctor --verbose
```

To check one package, use `upstream doctor NAME`. Add `--fix` to repair common
problems.

## Common problems

| Problem | What to try |
| --- | --- |
| An installed command is not found | Run `upstream hooks check`. If hooks are missing, run `upstream hooks init` and restart your shell. |
| Upstream picks the wrong release file | Run `upstream probe owner/repo --dry-run` to see the choices. Use `--match`, `--exclude`, or `--kind` to narrow them down. |
| A package is skipped during upgrades | Check `upstream info NAME`. If it is pinned, use `upstream package unpin NAME` when ready to update it. |
| `updates` shows old results | Run `upstream upgrade --check` to check for new releases. |
| Requests fail with a rate-limit or authentication error | Run `upstream doctor` to check your token. See [API token settings](config.md#api-tokens). |
| A checksum or signature check fails | Check the publisher's checksum or signature files and make sure you imported the right public key. See [download verification](usage.md#download-verification). |
| A source build fails | Check that Git and the project's required build tools are installed. Read the build error for missing dependencies. |
| Package documentation is outdated | Run `upstream docs --fetch NAME` to refresh its README. |
| Windows cannot replace an executable | Close the application and retry. Run `upstream doctor --fix` if Upstream reports leftover files. |
| A command says it is waiting for a lock | Wait for the other Upstream command to finish, or stop it if it is stuck. |
| Cached data is using too much space | Run `upstream cache list`, then preview cleanup with `upstream cache clean all --dry-run`. Remove `--dry-run` to apply it. |

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

## Report a bug

Use the [bug report template](../.github/ISSUE_TEMPLATE/bug_report.yml) and include:

- Your Upstream version (`upstream --version`), operating system, and CPU architecture.
- The exact command and steps needed to reproduce the problem.
- What you expected and what actually happened.
- The error output, relevant log lines, or crash report.

Remove tokens, credentials, and other private information before sharing files.
