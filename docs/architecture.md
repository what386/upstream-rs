# Code map

[Documentation](../README.md#documentation) · [Contributing](../CONTRIBUTING.md)

Find the files for a change below. See [Contributing](../CONTRIBUTING.md) for
build and test commands.

## Where to make a change

| Area                                                | Start here                                                      |
| --------------------------------------------------- | --------------------------------------------------------------- |
| Command options and help                            | [CLI arguments](../src/application/cli/arguments.rs)            |
| Running commands and taking locks                   | [CLI dispatch](../src/application/cli/dispatch.rs)              |
| Command output and interactive flows                | [Source](../src/)                                               |
| Code shared by commands                             | [Application](../src/application/)                              |
| Provider APIs and download pages                    | [Providers](../src/providers/)                                  |
| Interface used by all providers                     | [ReleaseProvider](../src/providers/sites/release_provider.rs)   |
| Release asset selection                             | [Asset selection](../src/providers/assets/)                     |
| Downloads                                           | [Download handler](../src/providers/shared/download_handler.rs) |
| Extracting files and finding executables            | [Artifact services](../src/services/artifact/)                  |
| Verification and trusted keys                       | [Trust services](../src/services/trust/)                        |
| Installation, replacement, and removal              | [Packaging services](../src/services/packaging/)                |
| Shell hooks, launchers, completions, and user units | [Integration services](../src/services/integration/)            |
| Source builds                                       | [Build routines](../src/routines/build/)                        |
| Diagnostics, README fetching, and migrations        | [Routines](../src/routines/)                                    |
| Package records and database tables                 | [Database storage](../src/storage/database/)                    |
| Config, tokens, trust storage, and locks            | [System storage](../src/storage/system/)                        |
| File and directory paths                            | [Path definitions](../src/utils/static_paths.rs)                |
| Logging and panic reports                           | [Logging](../src/logging.rs)                                    |
| Dynamic shell completions                           | [Completions](../src/completion/)                               |

## How commands run

1. [Early handling](../src/early.rs) handles help, version, shell completion, and
   saved update results before normal startup.
2. [Main](../src/main.rs) starts logging, runs migrations, and loads settings.
3. [Dispatch](../src/application/cli/dispatch.rs) takes a lock if needed, then
   runs the command.
4. Commands use operations and services to do the work. For installs, this means
   finding a release, downloading it, checking it, and preparing the files.
5. The activator moves the files into place, adds shell and desktop integration,
   and saves the package record.
6. Main logs the result and sets the exit code.

## Installation rules

- `--dry-run` must not install packages or change installed package records.
- Prepare files in an [install workspace](../src/services/packaging/staging.rs)
  before moving them into place. Dropping the workspace removes its temporary files.
- Use the [activator](../src/services/packaging/activation.rs) to replace packages.
  It keeps the old package until the new package record is saved, so a failed
  replacement can restore it.
- Add [cancellation checks](../src/application/cancellation.rs) to long operations
  so Ctrl-C can stop the work and clean up.
- Shell completion must not run migrations or start logging.

## Tests

| Suite       | What it checks                                                                                                                                  | Location                                                                                  |
| ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| Live        | Checks package operations against actual live websites, including search and release discovery.                                                 | [Live tests](../tests/live/)                                                              |
| End-to-end  | Similar to live, but uses the test server instead of live websites. Checks installs, upgrades, removal, and recovery when something goes wrong. | [End-to-end tests](../tests/end2end/)                                                     |
| Integration | Checks things that don't need a package website, such as shell setup, completions, the developer updater, and the test server itself.           | [CLI integration tests](../tests/integration/) and [server tests](../test-server/test.rb) |

Rust tests in [tests/](../tests/) also build sample projects and check logging
and saved update results. [Fixtures](../tests/fixtures/) hold sample files used
by the tests.

Enable `testing_donotuseinrelease` when running the CLI against test data to
keep changes away from your normal installation. Set `UPSTREAM_TEST_HOME` to
choose the test home directory. See [Contributing](../CONTRIBUTING.md) for test
commands and required tools.
