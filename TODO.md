# TODO — upstream-cli v1.5.4

@created: 2026-01-31
@modified: 2026-04-20

## Tasks

- [ ] Add signature verification support (GPG/minisign/cosign) and trust policies per source (high) #feature #security
      @created 2026-02-14 00:58

- [ ] Add per-package checksum pinning for reproducible installs (high) #feature #security
      @created 2026-02-14 00:58

- [ ] community package registry? #feature #website
      @created 2026-02-14 01:09


## Completed

- [x] fix issue where 'matrix' formats for checksums would fail to parse
      @created 2026-04-01 00:19
      @completed 2026-04-01 00:19
      @completed_version 1.4.6

- [x] let read-only operations ignore lockfile
      @created 2026-04-05 02:35
      @completed 2026-04-05 02:35
      @completed_version 1.5.0

- [x] Instead of failing, lockfiles block until lock is aquired
      @created 2026-04-05 02:34
      @completed 2026-04-05 02:35
      @completed_version 1.5.0

- [x] Fix snapshot import to avoid destructive pre-delete and guarantee rollback (high) #bug #data-loss
      @created 2026-04-10 02:54
      @completed 2026-04-10 03:01
      @completed_version 1.5.2

- [x] Fix snapshot import to avoid destructive pre-delete and guarantee rollback (high) #bug #data-loss
      @created 2026-04-10 02:54
      @completed 2026-04-10 03:02
      @completed_version 1.5.2

- [x] Harden archive extraction against path traversal/zip-slip writes (high) #bug #security #data-loss
      @created 2026-04-10 02:54
      @completed 2026-04-10 03:02
      @completed_version 1.5.2

- [x] Use per-package provider base_url in upgrade/import paths #bug #reliability
      @created 2026-04-10 02:54
      @completed 2026-04-10 03:02
      @completed_version 1.5.2

- [x] Fix lock stale-recovery policy so active long-running operations are never stolen #bug #reliability
      @created 2026-04-10 02:54
      @completed 2026-04-10 03:02
      @completed_version 1.5.2

- [x] Make filename marker parsing Unicode-safe (no byte/char index mismatch) (low) #bug #reliability
      @created 2026-04-10 02:54
      @completed 2026-04-10 03:02
      @completed_version 1.5.2

- [x] Fix lock stale-recovery policy so active long-running operations are never stolen #bug #reliability
      @created 2026-04-10 02:54
      @completed 2026-04-10 03:02
      @completed_version 1.5.2

- [x] Replace runtime unwrap panic paths with actionable errors #bug #reliability
      @created 2026-04-10 02:54
      @completed 2026-04-10 03:02
      @completed_version 1.5.2

- [x] Replace runtime unwrap panic paths with actionable errors #bug #reliability
      @created 2026-04-10 02:54
      @completed 2026-04-10 03:02
      @completed_version 1.5.2

- [x] behavior: config reads no longer autocreates config file
      @created 2026-04-17 02:31
      @completed 2026-04-17 02:31
      @completed_version 1.5.3

- [x] bug: fix build error on macos
      @created 2026-04-17 02:40
      @completed 2026-04-17 02:40
      @completed_version 1.5.4

- [x] feat: Readd Zstandard support
      @created 2026-04-20 18:20
      @completed 2026-04-20 18:23
      @completed_commit 972f514

