# Compression Fixtures

Archive fixture targets for `src/services/integration/compression_handler.rs`.

The `archives/` tree is checked in directly and can be consumed by tests as-is.

The hardlink tar fixtures are crafted archives, not normal filesystem captures:
- `tar-hardlink-safe.tar.gz` contains `pkg/link.txt` as a hardlink to `pkg/target.txt`.
- `tar-hardlink-missing-target.tar.gz` contains a hardlink to `pkg/missing.txt`.
- `tar-hardlink-absolute-target.tar.gz` contains a hardlink target of `/tmp/upstream-hardlink-escape.txt`.
- `tar-hardlink-traversal-target.tar.gz` contains a hardlink target of `../escape.txt`.

The negative zip fixtures cover path safety. ZIP hardlinks are not modeled by
the current extraction code.

Regenerate crafted fixtures with Python's standard `tarfile`, `gzip`, and
`zipfile` modules so hardlink targets can be set explicitly.
