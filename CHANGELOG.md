# Changelog — upstream-cli

*Generated on 2026-04-10*

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



