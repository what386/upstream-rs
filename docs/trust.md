# Trust and Verification

Upstream can verify downloaded release assets using checksums, signatures, or both. Trust behavior is controlled per install/reinstall/upgrade with `--trust`.

## Trust Modes

| Mode | Behavior |
| --- | --- |
| `none` | Skip checksum and signature verification |
| `best-effort` | Try available verification data, but do not require every form |
| `checksum` | Require checksum verification |
| `signature` | Require signature verification |
| `all` | Require checksum and signature verification |

Examples:

```bash
upstream install rg BurntSushi/ripgrep --trust best-effort
upstream install tool owner/repo --trust checksum
upstream upgrade --trust signature
upstream reinstall app --trust none
```

## Checksums

Upstream searches release assets for checksum files and verifies the selected install asset when a matching digest is available. Supported checksum formats include common SHA256 layouts and ordered checksum manifests.

Use `--trust checksum` when a package is expected to publish checksum assets and you want failure when they are missing or mismatched.

## Signatures

Upstream supports trusted minisign and cosign public keys. Import keys with:

```bash
upstream import ./minisign.pub --as keys
upstream import ./cosign.pub --as keys
```

Imported keys are merged into the config and deduplicated.

Use `--trust signature` when a package is expected to publish a signature asset matching the selected download.

## Best Effort vs Strict Modes

`best-effort` is useful for mixed package sets where some projects publish checksums/signatures and others do not. Strict modes are better for packages where verification artifacts are part of the expected release process.

For automation, prefer explicit trust modes and fail closed for high-value packages:

```bash
upstream install critical-tool owner/repo --trust all
```

## Probing Before Installing

Use `probe --verbose` to inspect releases and candidate assets before install:

```bash
upstream probe owner/repo --verbose
```
