# TODO — upstream-cli v1.4.4

@created: 2026-01-31
@modified: 2026-02-26

## Tasks

- [ ] Add signature verification support (GPG/minisign/cosign) and trust policies per source (high) #feature #security
      @created 2026-02-14 00:58

- [ ] Add per-package checksum pinning for reproducible installs (high) #feature #security
      @created 2026-02-14 00:58

- [ ] community package registry? #feature #website
      @created 2026-02-14 01:09

- [ ] Revert publish script tag normalization and use provided version argument directly #bug #release
      @created 2026-02-26 18:33


## Completed

- [x] Add --ignore-checksums flag for install and upgrade to skip checksum verification (high) #feature #cli #security
      @created 2026-02-23 17:21
      @completed 2026-02-23 17:24
      @completed_version 1.4.1

- [x] Improve cross-platform handling for Windows installers and macOS app bundles #feature #platform #investigating
      @created 2026-02-14 00:58
      @completed 2026-02-24 20:35
      @completed_version 1.4.3

- [x] Fix symlink recreation during upgrade rollback when previous link is dangling (high) #bug #cli
      @created 2026-02-25 19:45
      @completed 2026-02-25 19:47
      @completed_version 1.4.4

- [x] Fix release automation to commit changelog before tagging and harden notes extraction (high) #bug #release #ci
      @created 2026-02-26 16:58
      @completed 2026-02-26 16:59

- [x] Improve doctor to detect dangling symlinks and report broken symlink targets explicitly (high) #bug #cli #ux
      @created 2026-02-26 18:23
      @completed 2026-02-26 18:25

