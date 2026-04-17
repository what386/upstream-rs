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

---

## Installation

### Recommended (auto-install)

```bash
# Linux
curl -fsSL https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.bash | bash

# macOS
curl -fsSL https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.zsh | zsh

# Windows (PowerShell)
iwr -useb https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.ps1 | iex
```

This installs the binary and enables self-updates.

---

### Install with Cargo

```bash
cargo install upstream-rs
```

Ensure Cargo bin is in PATH:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

> ⚠️ Cargo installs do not support `upstream upgrade` self-updates.

---

### Manual install

1. Download a release from
   [https://github.com/what386/upstream/releases/latest](https://github.com/what386/upstream/releases/latest)
2. Make it executable:

```bash
chmod +x upstream-rs
```

Optional: install upstream via itself:

```bash
./upstream-rs install upstream what386/upstream-rs -k binary
```

---

## Quick Start

### Initialize

```bash
upstream init
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
* `--provider` → `github`, `gitlab`, `gitea`, `direct`, `scraper`
* `--channel` → `stable`, `preview`, `nightly`
* `--tag` → specific version
* `--match-pattern` / `--exclude-pattern`
* `--desktop` → create launcher entry

Examples:

```bash
# GitHub install
upstream install fd sharkdp/fd

# Direct download
upstream install app https://example.com/app.tar.gz -p direct -k archive

# Scrape a page
upstream install tool https://example.com/downloads -p scraper
```

---

### Upgrade

```bash
upstream upgrade [packages...] [--check] [--force]
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

Download a completion file from releases or generate one.

### Install

#### Bash

```bash
mkdir -p ~/.local/share/bash-completion/completions
cp upstream ~/.local/share/bash-completion/completions/
```

#### Fish

```fish
mkdir -p ~/.config/fish/completions
cp upstream.fish ~/.config/fish/completions/
```

#### Zsh

```zsh
mkdir -p ~/.zfunc
cp _upstream ~/.zfunc/
```

Add to `.zshrc` if needed:

```zsh
fpath=(~/.zfunc $fpath)
autoload -Uz compinit && compinit
```

---

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
