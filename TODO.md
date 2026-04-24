# TODO — upstream-cli v1.6.1

@created: 2026-01-31
@modified: 2026-04-24

## Tasks

- [ ] Add signature verification support (GPG/minisign/cosign) and trust policies per source (high) #feature #security
      @created 2026-02-14 00:58

- [ ] Add per-package checksum pinning for reproducible installs (high) #feature #security
      @created 2026-02-14 00:58

- [ ] feat: lockfiles now only care about pid (not time)
      @created 2026-04-24 19:12


## Completed

- [x] feat: Readd Zstandard support
      @created 2026-04-20 18:20
      @completed 2026-04-20 18:23
      @completed_version 1.5.5
      @completed_commit 972f514

- [x] feat: `package remove` for forcing package deletions
      @created 2026-04-21 00:02
      @completed 2026-04-21 00:02
      @completed_version 1.6.0

- [x] feat: `doctor` checks for orphaned installed directories/files
      @created 2026-04-21 00:05
      @completed 2026-04-21 00:06
      @completed_version 1.6.0
      @completed_commit 408163a

- [x] feat: `doctor` now is compact by default. use --verbose to restore old behavior.
      @created 2026-04-21 00:06
      @completed 2026-04-21 00:06
      @completed_version 1.6.0
      @completed_commit 408163a

- [x] bug: fix pid implementation on windows+macos
      @created 2026-04-24 19:11
      @completed 2026-04-24 19:11
      @completed_version 1.6.1

