# Contributing

Check existing issues and pull requests before starting. For a large change,
open an issue first to discuss it.

## Getting started

Install stable Rust, `rustfmt`, and `clippy`. Clone your fork and create a branch.

Install and remove commands affect your installed packages. Use `--dry-run` to
preview changes. For an isolated test installation, build with
`--features testing_donotuseinrelease` and set `UPSTREAM_TEST_HOME` to a test
directory.

See the [code map](docs/architecture.md) to find the files and tests for a change.

## Making changes

Keep changes focused and follow the existing style. Update tests, CLI help, and
[user docs](docs/usage.md) when behavior changes. Linux is the main platform;
check whether your change also affects Windows or macOS.

Before opening a pull request, run:

```bash
cargo fmt -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

On Linux, also run the Python integration tests if your change affects them:

```bash
python3 -m unittest discover -s tests/integration -p 'test_*.py'
```

For Windows installer changes, run the **Windows installer tests** workflow
from GitHub Actions or the CLI:

```bash
gh workflow run windows-installer.yml --ref YOUR_BRANCH
```

Python integration tests need Python; end-to-end tests also need Ruby for the
test server. Source-build tests need Rust/Cargo, Go, Zig, the .NET SDK, and CMake
with a C compiler.

## Submitting your work

In your pull request, explain the problem, what changed, and how you tested it.
Include your test platform and any checks you could not run.

For bugs, follow the [troubleshooting guide](docs/troubleshooting.md#report-a-bug).
