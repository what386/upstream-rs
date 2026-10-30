# Architecture

Upstream is organized as a pipeline with explicit boundaries:

1. `application/cli` parses arguments and dispatches commands.
2. `application/operations` coordinates a user-visible transaction.
3. `providers` resolves sources, releases, assets, and downloads.
4. `services/packaging` stages, installs, activates, upgrades, removes, and
   performs transient replacement recovery.
5. `storage` persists configuration, package metadata, and locks.
6. `output` owns prompts, tables, paging, status, and machine-readable output.

`models` should describe data and policy values, not perform filesystem or
network work. `utils` contains shared platform and filesystem primitives.

## Transaction boundary

Package operations should stage new files before changing active links or
metadata. A failed replacement must remove the partial result and leave the
previous package usable. The temporary `.old` snapshot remains until activation
and metadata persistence succeed.

Cancellation is cooperative: the first interrupt requests cleanup; a second
interrupt may terminate immediately and can leave recovery work behind.

## Package integrations

`services/integration/unit_manager.rs` discovers systemd user units in release
archives, standalone release assets, build sources, and extracted AppImages.
Explicit `systemd/user` and `linux-systemd/user` directories identify user units;
flat files require a user target such as `default.target` or
`graphical-session.target`. Matching timer/service companions are kept together.
System-unit directories and unrendered `.in` templates are excluded.

Units are staged with the package, installed without enabling or starting them,
and tracked under `state/units` for replacement recovery and removal. Existing
unowned user files are never overwritten. Commands matching an installed package
executable are adjusted to its final path. Successful installs report the unit
names and manual enable/start commands. `systemctl --user daemon-reload` is best
effort so installation works without a running user manager. The initial backend
is Linux only.

Integration paths honor XDG directories: application entries, user units, Bash
and Zsh completions use the data directory; Fish completions and Fish/Nushell
profile hooks use the config directory. Icon discovery also uses the data
directory. Test fixtures supply isolated roots explicitly.

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

Search results carry ordered key/value metadata supplied by the provider.
Integer and text values become dynamic columns in the shared search/find table;
Description uses the remaining terminal width. GitHub supplies `stars` and `lang`,
while AppImageHub omits those fields. JSON exposes this metadata alongside the
legacy `stars` and `language` fields for compatibility.

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
