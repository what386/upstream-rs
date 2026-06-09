# Changelog — upstream-cli

*Generated on 2026-06-09*

## 1.18.1 — 2026-06-09

### Changes

- support 7zip format


## 1.18.0 — 2026-06-09

### Changes

- add autorun of build/install scripts as fallback
- Implement pager for long command outputs


## 1.17.1 — 2026-05-29

### Changes

- refactor output formatter
- make list show package count in top element


## 1.17.0 — 2026-05-29

### Changes

- make rollback a continous callback instead of multiple printlines
- fix prompting for nonexistant packages in rollback
- force flag for uninstall
- remove set-key, get-key, metadata from package subcommand
- near-complete CLI update to modernize output


## 1.16.3 — 2026-05-28

### Changes

- bug: fix test failures on windows


## 1.16.2 — 2026-05-28

### Changes

- feat: fix asset detection for weird nested archives (e.g. broot)


## 1.16.1 — 2026-05-27

### Changes

- feat: disk space free/used estimates
- feat: confirmations for destructive actions


## 1.16.0 — 2026-05-27

### Changes

- changelog subcommand
- feat: install detected autocompletion scripts to shells
- remove 'rate limit' key from cfg


## 1.15.2 — 2026-05-14

### Changes

- bug: sometimes harmless relative paths would be rejected in decompression 'e.g: ../README.md'


## 1.15.1 — 2026-05-11

### Changes

- behavior: make icon matching more consistent


## 1.15.0 — 2026-05-09

### Changes

- feat: `search` subcommand that can search providers for keywords


## 1.14.2 — 2026-05-09

### Changes

- bug: fix checksum signatures applying to binaries
- bug: fix cosign not actually applying checksums to blobs (??)
- feat: update fish paths.sh line


## 1.14.1 — 2026-05-06

### Changes

- feat: package rollbacks for upgrade/reinstall/remove
- bug: fix rollback 'stealing' deletion from remove and throwing an error


## 1.14.0 — 2026-05-06

### Changes

- feat: package rollbacks for upgrade/reinstall/remove


## 1.13.1 — 2026-05-06

### Changes

- behavior/bug: Archives can now contain symlinks/hardlinks


## 1.13.0 — 2026-05-04

### Changes

- feat: dry run for install ops
- feat: dry run for upgrade/reinstall ops
- feat: dry run for remove op


## 1.12.0 — 2026-05-01

### High Priority

- feat: doctor --fix repairs symlink/PATH/executable metadata `feature`, `doctor`, `ux`
- behavior: versioned packages.json with legacy-array compatibility `behavior`, `storage`, `compat`
- feat: package pin --reason with sidecar metadata file `feature`, `package`, `metadata`

### Changes

- feat: arbitrarily nested executable detection, e.g: 'root/projectname/x86_64/program' ([`169ba1d`])
- behavior: remove interactive confirmation from import command ([`193cae2`])
- feat: list --json outputs package metadata (single/all) `feature`, `list`, `cli`
- behavior: start versioning package file


## 1.11.1 — 2026-05-01

### Changes

- feat: include commit hash in long --version
- feat: atomic writes for packages.json
- feat: atomic writes for config.toml
- feat: atomic writes for shell integration


## 1.11.0 — 2026-04-30

### Changes

- feat: replace --ignore-checksums with --trust policy modes and wire signature checks `feature`, `security`, `cli`
- feat: add trusted minisign key helpers in config model/storage `feature`, `security`
- feat: extend import with keys/manifest/snapshot autodetection plus --as and --yes `feature`, `cli`
- feat: switch manifest import to metadata-only with conflict skip/warn behavior `feature`, `cli`


## 1.10.0 — 2026-04-29

### Changes

- feat: track branch build updates by commit hash in upgrade/check `feature`, `cli`
- feat: make build --tag and --branch mutually exclusive `feature`, `cli`
- feat: add --branch option to build command `feature`, `cli`


## 1.9.0 — 2026-04-29

### Changes

- feat: add more profiles for `build` feature


## 1.8.0 — 2026-04-28

### Changes

- feat: add `reinstall` command
- feat: add `build` command that builds packages from source code


## 1.7.0 — 2026-04-28

### Changes

- bug: fix potential orphaned entries in PATHS file
- feat: rename `init` to `hooks`. Add subcommands to hooks: init, clean, purge, check
- feat: improve wording in help text


## 1.6.2 — 2026-04-24

### Changes

- bug: fix builds/warnings on windows


## 1.6.1 — 2026-04-24

### Changes

- bug: fix pid implementation on windows+macos
- feat: lockfiles now only care about pid (not time)


## 1.6.0 — 2026-04-21

### Changes

- feat: `package remove` for forcing package deletions
- feat: `doctor` checks for orphaned installed directories/files ([`408163a`])


## 1.5.5 — 2026-04-20

### Changes

- feat: Readd Zstandard support ([`972f514`])


## 1.5.4 — 2026-04-17

### Changes

- bug: fix build error on macos


## 1.5.3 — 2026-04-17

### Changes

- behavior: config reads no longer autocreates config file


## 1.5.2 — 2026-04-10

### High Priority

- Fix snapshot import to avoid destructive pre-delete and guarantee rollback `bug`, `data-loss`
- Harden archive extraction against path traversal/zip-slip writes `bug`, `security`, `data-loss`

### Changes

- Fix lock stale-recovery policy so active long-running operations are never stolen `bug`, `reliability`
- Replace runtime unwrap panic paths with actionable errors `bug`, `reliability`
- Use per-package provider base_url in upgrade/import paths `bug`, `reliability`

### Minor Changes

- Make filename marker parsing Unicode-safe (no byte/char index mismatch) `bug`, `reliability`


## 1.5.0 — 2026-04-05

### Changes

- let read-only operations ignore lockfile
- Instead of failing, lockfiles block until lock is aquired


## 1.4.6 — 2026-04-01

### Changes

- fix issue where 'matrix' formats for checksums would fail to parse


## 1.4.5 — 2026-02-26

### High Priority

- Improve doctor to detect dangling symlinks and report broken symlink targets explicitly `bug`, `cli`, `ux`
- Fix release automation to commit changelog before tagging and harden notes extraction `bug`, `release`, `ci`

### Changes

- Revert publish script tag normalization and use provided version argument directly `bug`, `release`


## 1.4.4 — 2026-02-25

### High Priority

- Fix symlink recreation during upgrade rollback when previous link is dangling `bug`, `cli`


## 1.4.3 — 2026-02-24

### Changes

- Improve cross-platform handling for Windows installers and macOS app bundles `feature`, `platform`, `investigating`
- Improve cross-platform handling for Windows installers and macOS app bundles `feature`, `platform`, `investigating`


## 1.4.1 — 2026-02-23

### High Priority

- Add --ignore-checksums flag for install and upgrade to skip checksum verification `feature`, `cli`, `security`


## 1.4.0 — 2026-02-21

### High Priority

- Add lockfile to prevent concurrent mutating operations `bug`, `reliability`, `ops`
- Add package rename command `feature`, `cli`
- Add import --skip-failed mode `feature`, `reliability`

### Changes

- Refactor lock storage to acquire from Commands at dispatch start `refactor`, `reliability`
- Move CLI command label Display impls into application/cli/labels.rs `refactor`, `cli`
- Add init --check mode `feature`, `cli`
- Add non-intrusive unit tests for CLI flags, metadata rename, init checks, import detection, and package storage `test`, `reliability`


## 1.3.2 — 2026-02-21

### High Priority

- Fix cross-device (EXDEV) install moves from /tmp to ~/.upstream `bug`, `platform`


## 1.2.1 — 2026-02-14

### High Priority

- Add verify and doctor commands for install integrity and system diagnostics `feature`, `ops`

### Changes

- Add conditional HTTP checks with ETag/Last-Modified to speed update scans `feature`, `http`, `performance`
- Generate and ship shell completions with synchronized CLI docs `feature`, `docs`


## 1.0.4 — 2026-02-07

### High Priority

- Fix GitHub latest-release JSON parsing when fields are null `bug`


## 1.0.3 — 2026-02-06

### Changes

- fallback for missing icon? maybe include a default icon when lookup fails `feature`


## 1.0.2 — 2026-02-06

### High Priority

- fix archives not respecting the name argument


## 1.0.0 — 2026-02-03

### High Priority

- extract appimages for icons `feature`, `bugfix` ([`d5a89fc`])

### Minor Changes

- fix resolved filetype callback to use proper display function `bugfix` ([`eb8e608`])
- add appimage embedded .desktop file extraction `feature` ([`0cc8ded`])


## 0.9.0 — 2026-02-03

### Changes

- add export/import for packages `feature` ([`51c4cb0`])

### Minor Changes

- consider removing dead functions `cleanup` ([`7669a1a`])


