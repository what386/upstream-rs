# Package registry maintenance

Registry definitions live in `registry/packages/`. The readable and minified
version 1 indexes are generated artifacts; update both after changing a package:

```sh
just registry-gen-index
just registry-validate
```

Every recipe declares a trust policy. Build recipes must use `none` because the
build path does not apply registry release-asset verification. Release recipes
may use `checksum` only when every selected asset for Linux, macOS, and Windows
on x86_64 and aarch64 is covered across recent stable releases. Missing assets,
intermittent coverage, and unsupported checksum formats require `best-effort`.
The registry does not use `signature` or `all` because it cannot distribute a
pinned public key as part of a clean installation.

Run the read-only network audit before reviewing release trust policies:

```sh
just registry-audit
just registry-audit --package upstream
```

The report records each selected asset, checksum asset, parsed format, and
coverage result. Provider and network failures exit separately from strict
coverage regressions. The audit never edits definitions or generated indexes.
It also runs weekly in the non-gating `Registry trust audit` workflow; that job
uploads its report and fails visibly if a package configured for strict checksum
verification loses coverage. Pull-request registry checks remain deterministic
and do not depend on the network.

When reviewed metadata changes, increment that package's revision exactly once.
