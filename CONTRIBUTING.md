# Contributing

Bug fixes, features, documentation improvements, and bug reports are welcome.
Check existing issues and pull requests before starting. For larger changes,
open an issue first to discuss the approach.

## Getting started

Install stable Rust with `rustfmt` and `clippy`, then clone your fork and create
a branch for your change.

Running commands that install or remove packages affects your local Upstream
setup. Use `--dry-run` when previewing those operations.

Test binaries use a different path helper file, so they do not change
your local install. Enable the "testing_donotuseinrelease" feature,
or run tests with cargo / justfile, since they set it automatically.

## Making changes

Keep changes focused and follow the surrounding code style. Add or update tests
when behavior changes, and update CLI help and [user docs](docs/usage.md) when
needed. Linux is the primary platform, but consider Windows and macOS behavior.

Before submitting, run the checks used by CI:

```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

On Linux, also run the Python integration tests for relevant changes:

```bash
python3 -m unittest discover -s tests/integration -p 'test_*.py'
```

Windows installer tests run separately from regular CI. When changing the
installer, run the **Windows installer tests** workflow from GitHub's Actions
tab or the CLI:

```bash
gh workflow run windows-installer.yml --ref YOUR_BRANCH
```

Integration and end-to-end tests use Python and Ruby. 

The source-build tests require all the toolchains they exercise: 
Rust/Cargo, Go, Zig, the .NET SDK, and CMake with a C compiler.
Install these before running the full test suite, or skip them
if your changes do not affect the source-build system.

## Submitting your work

Open a pull request describing the problem, what changed, and how you checked
it. Follow the PR template and note any checks you could not run, including the
platform you tested on.

For bug reports, include your Upstream version, operating system, the command
you ran, and the expected and actual results. Remove API tokens and other
private information from logs before sharing them.
