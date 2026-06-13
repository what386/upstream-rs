# Backup, Import, and Export

Upstream supports two backup styles:

- A lightweight manifest that records enough package metadata to reinstall.
- A full snapshot archive of the local Upstream data directory.

## Export a Manifest

```bash
upstream export ./packages.json
```

The manifest is intended for migration or replication. It does not contain installed binaries.

Import it on another machine:

```bash
upstream import ./packages.json
```

Manifest imports add package references only. They do not restore installed files, executable paths, icons, rollback data, or recorded release versions. After importing a manifest, run `upstream install` for the packages you want to materialize on the new machine, using the same local alias and source:

```bash
upstream install BurntSushi/ripgrep rg -k binary
```

Use a full snapshot when you need to restore installed artifacts and runtime paths exactly.

## Export a Full Snapshot

```bash
upstream export ./backup.tar.gz --full
```

A full snapshot captures the local Upstream data directory. Restore it with:

```bash
upstream import ./backup.tar.gz --as snapshot
```

Snapshot imports replace local Upstream data after confirmation. Use them when restoring the same environment or moving a complete local state.

## Import Trusted Keys

```bash
upstream import ./minisign.pub --as keys
upstream import ./cosign.pub --as keys
```

Autodetection usually works, but `--as keys` is useful in scripts.

## Autodetection and `--as`

Import can autodetect:

| Input | Import kind |
| --- | --- |
| `*.tar.gz`, `*.tgz` | Snapshot |
| JSON manifest with supported version | Manifest |
| minisign/cosign public key files | Keys |

Force the import type when autodetection is ambiguous:

```bash
upstream import ./input.bin --as keys
upstream import ./packages.json --as manifest
upstream import ./backup.tgz --as snapshot
```

## Partial Failures

For manifest imports, `--skip-failed` continues processing remaining entries if an individual package metadata import fails:

```bash
upstream import ./packages.json --skip-failed
```

`--skip-failed` has no effect for key or snapshot imports.
