# Upstream

**Upstream** is a rootless package manager for installing software directly from release sources like GitHub, GitLab, and arbitrary download pages.

It fetches release assets, selects the best match for your system, and keeps them updated.

---

## Features

* Install from GitHub, GitLab, Gitea, direct URLs, or scraped pages
* Automatic OS + architecture detection (Linux/macOS, x86_64/ARM)
* Supports binaries, archives, AppImages, and compressed files
* Rootless (user-level installs)
* Track update channels: `stable`, `preview`, `nightly`
* Flexible asset matching and filtering
* Dry-run previews for install, build, upgrade, remove, rollback, and reinstall workflows
* Optional checksum and signature verification modes

---

## Installation

### Recommended (auto-install)

```bash
# Linux
curl -fsSL https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.bash | bash

# Linux (Fish)
curl -fsSL https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.fish | fish

# macOS
curl -fsSL https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.zsh | zsh

# Windows (PowerShell)
iwr -useb https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.ps1 | iex
```

This installs the binary and enables self-updates. Upstream manages package completion installation itself.

---

### Install with Cargo

```bash
cargo install upstream
```

Ensure Cargo bin is in PATH:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

> ⚠️ Cargo installs do not support `upstream upgrade` self-updates.

---

### Manual install

1. Download a release from
   [https://github.com/what386/upstream-rs/releases/latest](https://github.com/what386/upstream-rs/releases/latest)
2. Make it executable:

```bash
chmod +x upstream
```

Optional: install upstream via itself:

```bash
./upstream install upstream what386/upstream-rs -k binary
```

---

## Quick Start

### Initialize

```bash
upstream hooks init
```

### Install a package

```bash
upstream install mytool owner/repo
```

Example:

```bash
upstream install rg BurntSushi/ripgrep
```

---

### Search and inspect sources

```bash
upstream search ripgrep
upstream probe BurntSushi/ripgrep
```

---

### Upgrade

```bash
upstream upgrade
```

---

### Remove

```bash
upstream remove mytool
```

---

### List

```bash
upstream list
```

---

## Usage

### Install

```bash
upstream install <name> <source> [options]
```

* `<name>` → local alias
* `<source>` → repo (`owner/repo`) or URL

Options:

* `--kind` → asset type (`auto`, `archive`, `binary`, etc.)
* `--provider` → override auto-detection (`github`, `gitlab`, `gitea`, `direct`, `scraper`)
* `--channel` → `stable`, `preview`, `nightly`
* `--tag` → specific version
* `--match-pattern` / `--exclude-pattern`
* `--desktop` → create launcher entry
* `--yes` → accept the recommended discovered asset without prompting
* `--dry-run` → preview resolution without downloading or writing files
* `--trust` → verification mode (`none`, `best-effort`, `checksum`, `signature`, `all`)

Examples:

```bash
# GitHub install
upstream install fd sharkdp/fd

# Direct download
upstream install app https://example.com/app.tar.gz -k archive

# Download assets from a download page
upstream install tool https://example.com/downloads
```

Archives that contain platform-specific subdirectories are resolved automatically.
Use `--match-pattern` or `--exclude-pattern` to steer selection when an archive
ships multiple compatible payloads.

---

### Upgrade

```bash
upstream upgrade [packages...] [--check] [--force] [--dry-run]
```

Use `--check --machine-readable` for script-friendly update checks.

---

### Search and Probe

```bash
upstream search <query> [--limit 20]
upstream probe <source> [--channel stable] [--verbose]
```

---

### Remove

```bash
upstream remove <packages...> [--purge]
```

---

### Config

```bash
upstream config set key=value
upstream config get key
upstream config list
upstream config edit
```

---

### Package management

```bash
upstream package pin <name>
upstream package unpin <name>
upstream package remove <name>
upstream package metadata <name>
```

---

### Import / Export

```bash
upstream export file.json
upstream import file.json
```

---

## Shell Completions

Upstream automatically installs package completion scripts for detected local
shells when a release includes matching `bash`, `fish`, or `zsh` files such as
`<name>.fish`, `completions.bash`, or `completions/*.zsh`. Archives and
AppImages are scanned after extraction.

Install manually via helper scripts:

```bash
scripts/install/completions.sh bash
scripts/install/completions.sh fish
scripts/install/completions.sh zsh
scripts/install/completions.sh elvish
```

```powershell
pwsh -File scripts/install/completions.ps1
```

## Architecture Detection

Upstream automatically selects assets based on:

* OS: Linux, macOS
* Arch: x86_64, ARM64

Selection is based on filename patterns and extensions.

---

## Notes

* Upstream installs packages in user space (no root required)
* It does not manage system dependencies

---

## License

MIT OR Apache-2.0
