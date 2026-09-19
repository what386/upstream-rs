# Testing

Tests are split by what they prove.

## Rust tests

Unit and Rust integration tests cover models, storage, providers, artifact
handling, packaging transactions, build profiles, and platform-gated code.
`tests/fixtures/` contains small deterministic release, archive, provider, and
repository fixtures. Prefer real fixture repositories and behavior-level tests
over mocked metadata when testing build discovery or artifact installation.

## CLI tests

The Python suites exercise the installed CLI and filesystem-visible lifecycle:

- `tests/integration/test_*.py` is fast local CLI and state coverage.
- `tests/end2end/test_*.py` installs from the local test server and checks the
  full package lifecycle.
- `tests/live/test_*.py` probes real providers. Run these when you intend to
  use the network.
- `tests/framework/` contains the shared fake-home and command helpers.

Keep live-provider tests independently runnable.

## Verification

The standard local checks are exposed through `Justfile`:

```text
just lint       # formatting, Clippy, and Windows cross-checks
just test       # Rust tests
just test-all   # local CLI, end-to-end, and live-provider tests
```

Run one local CLI test group when you are working on it:

```text
just run-tests integration
just run-tests end2end
just run-tests live
```

The live group needs network access. Native Windows behavior still needs a
Windows runner or machine; building on Linux is not the same thing.

## Local artifact server

The Ruby server in `test-server/` provides a localhost endpoint for
direct-download tests without contacting an external provider:

```text
just start-artifact-server
```

Put files in `test-server/artifacts/` and reference them as
`http://127.0.0.1:8000/artifacts/<filename>`. Set `SERVER_HOST` or
`SERVER_PORT` when a different listener is needed.

## Fixtures

Keep fixtures small, deterministic, and named for the behavior they exercise.
When a generated fixture is unavoidable, document regeneration beside it.
Archive fixtures must preserve the security property under test, including path
traversal, symlink, hardlink, and executable-permission cases.
