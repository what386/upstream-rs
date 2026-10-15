# Contributing

Update behavior-level tests and the relevant documentation when a contract
changes.

## Local workflow

Run `just --list` for local commands. CI checks are defined in
`.github/workflows/ci.yml`.

## Change boundaries

- Update migrations and compatibility tests together with schema changes.
- Preserve canonical package identity separately from executable aliases.
- Keep destructive operations confirmed and fail closed on unsafe targets.
- Treat network-backed provider tests as live tests; use local fixtures for CI.
- Report host-only, cross-compiled, and native-runtime evidence separately.

## Release work

Release verification includes Rust checks, installer tests, and integration
tests. Native platform behavior requires testing in its respective CI
environment.
