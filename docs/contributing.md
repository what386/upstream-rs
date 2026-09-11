# Contributing

Read the source module that owns a behavior before extending it. Keep changes
inside the narrowest appropriate boundary and update behavior-level tests and
developer documentation when a contract changes.

## Local workflow

Use `just` for the repository's common formatting, linting, testing, fixture,
and release commands. The CI workflow in `.github/workflows/ci.yml` is the
authoritative baseline for checks that must pass in pull requests.

Rust formatting includes `cargo spaced`; a normal rustfmt pass is not enough
for this repository. Keep generated completions synchronized when changing CLI
arguments or completion behavior.

## Change boundaries

- Update migrations and compatibility tests together with schema changes.
- Preserve canonical package identity separately from executable aliases.
- Keep destructive operations confirmed and fail closed on unsafe targets.
- Treat network-backed provider tests as live tests; use local fixtures for CI.
- Report host-only, cross-compiled, and native-runtime evidence separately.
- Preserve unrelated worktree changes.

## Release work

Release scripts under `scripts/release/` prepare, promote, publish, and resync
artifacts. Release verification includes Rust checks, installer tests, and
integration tests. Native platform artifacts and runtime behavior still require
their respective CI environments.
