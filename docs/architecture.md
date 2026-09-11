# Architecture

Upstream is organized as a pipeline with explicit boundaries:

1. `application/cli` parses arguments and dispatches commands.
2. `application/operations` coordinates a user-visible transaction.
3. `providers` resolves sources, releases, assets, and downloads.
4. `services/packaging` stages, installs, activates, upgrades, removes, and
   rolls back artifacts.
5. `storage` persists configuration, package metadata, locks, and rollback data.
6. `output` owns prompts, tables, paging, status, and machine-readable output.

`models` should describe data and policy values, not perform filesystem or
network work. `utils` contains shared platform and filesystem primitives.

## Transaction boundary

Package operations should stage new files before changing active links or
metadata. A failed replacement must remove the partial result and leave the
previous package usable. Persistent rollback artifacts are distinct from the
temporary recovery copies used during an in-progress replacement.

Cancellation is cooperative: the first interrupt requests cleanup; a second
interrupt may terminate immediately and can leave recovery work behind.

## Identity

The package ID is the canonical provider-and-source identity. It is not the
installed executable name. Executables are stored as aliases with their
artifact paths. Filesystem-safe names belong at path boundaries only; do not
use a sanitized filename as the package identity.

## Providers

Provider adapters normalize external APIs into the shared release, asset, and
search models. Provider-specific DTOs stay inside their adapter modules.
`ProviderManager` selects the adapter and applies source normalization before
the operation layer sees it. HTTP direct and scraped sources are separate from
forge providers because they do not provide equivalent repository metadata.

## Output

Human-readable output and JSON output are separate contracts. New operations
should pass structured results to `output` rather than assembling terminal
strings in business logic. Prompts must remain explicit for destructive work;
`--yes` bypasses confirmation but never supplies required interactive input.

## Platform boundary

Linux is the primary platform. Windows and macOS support are experimental, with
Windows currently more tested than macOS. Platform-specific behavior belongs in
the relevant service or `utils/platform` module and must not leak into shared
operation policy.
