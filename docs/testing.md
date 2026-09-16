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

- `tests/integration/test_*.py` uses local fixtures and is hermetic.
- `tests/live/test_*.py` contacts real providers and is intentionally
  separate from normal CI.
- `tests/install/test_*.py` covers installer lifecycle behavior.
- `tests/framework/` contains fake-home, command, package, and local-server
  helpers.

Keep live-provider tests independently runnable. Do not make hermetic tests
depend on network access or a developer's real home directory.

## Verification

The standard local checks are exposed through `Justfile`:

```text
just fmt
just lint
just test
just integration-tests-hermetic
just install-script-tests
```

`just integration-tests-live` is opt-in because it uses external services.
`just verify-release` is the release-level aggregate and includes the live tier.
Use the narrowest relevant test first, then run broader checks before handoff.
Native Windows behavior requires Windows CI or a Windows toolchain; Linux
compilation alone is not runtime proof for Windows.

## Fixtures

Keep fixtures small, deterministic, and named for the behavior they exercise.
When a generated fixture is unavoidable, document regeneration beside it.
Archive fixtures must preserve the security property under test, including path
traversal, symlink, hardlink, and executable-permission cases.
