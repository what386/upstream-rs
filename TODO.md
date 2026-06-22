# TODO — upstream-cli

@created: 2026-01-31
@modified: 2026-06-22

## Tasks

- [ ] feat: consider bulk installs with `upstream install --package REPO=NAME --package REPO=NAME`
      @created 2026-06-21 15:25


## Completed

- [x] BREAKING: rollback is now names-only. transactions are no longer recorded. CLI shape reverted.
      @created 2026-06-21 23:08
      @completed 2026-06-21 23:22

- [x] BREAKING: remove 'metadata' storage and pin reason
      @created 2026-06-21 23:10
      @completed 2026-06-21 23:22

- [x] BREAKING: make migrate a flag in doctor instead of its own first class subcommand
      @created 2026-06-21 23:44
      @completed 2026-06-21 23:44

- [x] feat: improve list <package> UI
      @created 2026-06-22 15:40
      @completed 2026-06-22 17:25

- [x] feat: add install type and commit hash to list
      @created 2026-06-22 15:41
      @completed 2026-06-22 17:25

- [x] internal: move packages.json into a SQL db
      @created 2026-06-21 16:31
      @completed 2026-06-22 17:25

