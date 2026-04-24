# Changelog — upstream-cli

*Generated on 2026-04-24*

## 1.6.1 — 2026-04-24

### Changes

- bug: fix pid implementation on windows+macos


## 1.6.0 — 2026-04-21

### Changes

- feat: `package remove` for forcing package deletions


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



