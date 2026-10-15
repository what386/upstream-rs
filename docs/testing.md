# Testing

## Rust tests

For build discovery and artifact installation, prefer behavior-level tests
using fixture repositories over mocked metadata.

## CLI tests

The Python suites exercise the installed CLI:

- `tests/integration/` covers local commands and state.
- `tests/end2end/` uses the local artifact server for package lifecycle tests.
- `tests/live/` probes real providers and requires network access.

Keep live-provider tests independently runnable.

## Verification

Run the standard checks with `just`:

```text
just lint
just test
just test-all
```

Run one local CLI test group when you are working on it:

```text
just run-tests integration
just run-tests end2end
just run-tests live
```

Native Windows behavior requires a Windows runner or machine.

## Local artifact server

The local server provides artifacts for direct-download tests:

```text
just start-artifact-server
```

Place artifacts in `test-server/artifacts/`. Fixture conventions are in
[`tests/fixtures/README.md`](../tests/fixtures/README.md).
