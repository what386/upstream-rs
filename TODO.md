# TODO — upstream-cli v1.2.0

@created: 2026-01-31
@modified: 2026-02-14

## Tasks

- [ ] Add signature verification support (GPG/minisign/cosign) and trust policies per source (high) #feature #security
      @created 2026-02-14 00:58

- [ ] Add per-package checksum pinning for reproducible installs (high) #feature #security
      @created 2026-02-14 00:58

- [ ] Improve cross-platform handling for Windows installers and macOS app bundles #feature #platform #investigating
      @created 2026-02-14 00:58

- [ ] Generate and ship shell completions with synchronized CLI docs #feature #docs
      @created 2026-02-14 00:58

- [ ] community package registry? #feature #website
      @created 2026-02-14 01:09

## Completed

- [x] fix archives not respecting the name argument (high)
      @created 2026-02-06 21:38
      @completed 2026-02-06 21:38
      @completed_version 1.0.2

- [x] fallback for missing icon? maybe include a default icon when lookup fails #feature
      @created 2026-02-04 03:11
      @completed 2026-02-06 22:13
      @completed_version 1.0.3

- [x] Fix GitHub latest-release JSON parsing when fields are null (high) #bug
      @created 2026-02-07 04:34
      @completed 2026-02-07 04:35
      @completed_version 1.0.4

- [x] Add verify and doctor commands for install integrity and system diagnostics (high) #feature #ops
      @created 2026-02-14 00:58
      @completed 2026-02-14 01:08

- [x] Add conditional HTTP checks with ETag/Last-Modified to speed update scans #feature #http #performance
      @created 2026-02-14 00:58
      @completed 2026-02-14 01:25
