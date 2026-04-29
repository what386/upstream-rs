# TODO — upstream-cli v1.9.0

@created: 2026-01-31
@modified: 2026-04-29

## Tasks

- [ ] Add signature verification support (GPG/minisign/cosign) and trust policies per source (high) #feature #security
      @created 2026-02-14 00:58

- [ ] Add per-package checksum pinning for reproducible installs (high) #feature #security
      @created 2026-02-14 00:58


## Completed

- [x] bug: fix potential orphaned entries in PATHS file
      @created 2026-04-28 00:23
      @completed 2026-04-28 00:25
      @completed_version 1.7.0

- [x] feat: rename `init` to `hooks`. Add subcommands to hooks: init, clean, purge, check
      @created 2026-04-28 00:22
      @completed 2026-04-28 00:25
      @completed_version 1.7.0

- [x] feat: improve wording in help text
      @created 2026-04-28 00:25
      @completed 2026-04-28 00:25
      @completed_version 1.7.0

- [x] feat: add `reinstall` command
      @created 2026-04-28 23:40
      @completed 2026-04-28 23:41
      @completed_version 1.8.0

- [x] feat: add `build` command that builds packages from source code
      @created 2026-04-28 23:40
      @completed 2026-04-28 23:41
      @completed_version 1.8.0

- [x] feat: add more profiles for `build` feature
      @created 2026-04-29 01:33
      @completed 2026-04-29 01:33
      @completed_version 1.9.0

- [x] feat: track branch build updates by commit hash in upgrade/check #feature #cli
      @created 2026-04-29 19:05
      @completed 2026-04-29 19:07

- [x] feat: make build --tag and --branch mutually exclusive #feature #cli
      @created 2026-04-29 19:05
      @completed 2026-04-29 19:07

- [x] feat: add --branch option to build command #feature #cli
      @created 2026-04-29 19:05
      @completed 2026-04-29 19:07

