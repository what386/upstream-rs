# Testing

Tests are split by what they prove.

## Rust tests

Unit and Rust integration tests cover models, storage, providers, artifact
handling, packaging transactions, build profiles, and platform-gated code.
`tests/fixtures/` contains small deterministic release, archive, provider, and
repository fixtures. Prefer real fixture repositories and behavior-level tests
over mocked metadata when testing build discovery or artifact installation.

## Python integration tests

The Python suites exercise the installed CLI and filesystem-visible lifecycle:

- `tests/integration/test_*.py` covers fast hermetic CLI and state behavior.
- `tests/end2end/test_*.py` covers end-to-end installs using the local http server
- `tests/other/test_*.py` covers other miscellaneous tests.
- `tests/live/github/github_*.py` contacts GitHub with live network requests.
- `tests/framework/` contains fake-home, package and command helpers.

Keep live-provider tests independently runnable.

## Verification

The standard local checks are exposed through `Justfile`:

```text
just fmt
just lint
just test
just integration-tests-hermetic
just install-script-tests
```

`just run-tests-live` is opt-in because it uses external services.
`just verify-release` is the release-level aggregate and includes the live tier.
Use the narrowest relevant test first, then run broader checks before handoff.
Native Windows behavior requires Windows CI or a Windows toolchain; Linux
compilation alone is not runtime proof for Windows.

## Local artifact server

The Ruby server in `server/` provides a localhost endpoint for
direct-download tests without contacting an external provider:

```text
just server
```

Put files in `server/artifacts/` and reference them as
`http://127.0.0.1:8000/artifacts/<filename>`. Set `SERVER_HOST` or
`SERVER_PORT` when a different listener is needed.

## Fixtures

Keep fixtures small, deterministic, and named for the behavior they exercise.
When a generated fixture is unavoidable, document regeneration beside it.
Archive fixtures must preserve the security property under test, including path
traversal, symlink, hardlink, and executable-permission cases.
