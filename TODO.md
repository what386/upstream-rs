# TODO — upstream-cli v1.3.2

@created: 2026-01-31
@modified: 2026-02-21

## Tasks

- [ ] Add signature verification support (GPG/minisign/cosign) and trust policies per source (high) #feature #security
      @created 2026-02-14 00:58

- [ ] Add per-package checksum pinning for reproducible installs (high) #feature #security
      @created 2026-02-14 00:58

- [ ] Improve cross-platform handling for Windows installers and macOS app bundles #feature #platform #investigating
      @created 2026-02-14 00:58

- [ ] community package registry? #feature #website
      @created 2026-02-14 01:09

- [ ] lockfile (high) #feature #bugfix
      @created 2026-02-21 12:59


## Completed

- [x] Add lockfile to prevent concurrent mutating operations (high) #bug #reliability #ops
      @created 2026-02-21 13:00
      @completed 2026-02-21 13:01

- [x] Refactor lock storage to acquire from Commands at dispatch start #refactor #reliability
      @created 2026-02-21 13:07
      @completed 2026-02-21 13:07

