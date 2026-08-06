# TODO — upstream-cli

@created: 2026-01-31
@modified: 2026-08-06


## Tasks

- [ ] chore: update package registry to have proper trust rules
      @created 2026-07-22 21:28

- [ ] consider: crash recovery
      @created 2026-07-22 23:44

- [ ] consider: split CLI definitions into command-family modules if maintenance warrants it
      @created 2026-08-04 21:58

- [ ] refactor: replace high-arity command dispatch calls with typed request structs
      @created 2026-08-04 21:58

- [ ] docs: document contributor workflows and architecture invariants
      @created 2026-08-04 21:58


## Completed

- [x] behavior: centralize and harden Windows PATH registry management #windows #path #correctness
      @created 2026-08-06 22:33
      @completed 2026-08-06 22:33

- [x] feat: add a Windows repair script for legacy installs #windows #repair
      @created 2026-08-06 22:33
      @completed 2026-08-06 22:33

- [x] fix: retry Windows executable renames during package upgrades #windows #upgrade
      @created 2026-08-06 22:33
      @completed 2026-08-06 22:33
