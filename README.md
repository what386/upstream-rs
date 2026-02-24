# Upstream Package Manager

**Upstream** is a rootless package manager for "raw" release channels. It installs and updates software from source-code providers like Github as well as normal download pages via web scraping. Upstream supports multiple asset types, tracks update channels, and automatically selects the best asset for your OS and CPU architecture.

---

## **Table of Contents**

1. [Features](#features)
2. [Installation](#installation)
   1. [Auto Install (Recommended)](#auto-install-recommended)
   2. [Install via Cargo (Crates.io)](#install-via-cargo-cratesio)
   3. [Manual Installation](#manual-installation)
   4. [Build from Source](#build-from-source)

3. [Usage](#usage)
   1. [Initialize Hooks](#initialize-hooks)
   2. [Install a Package](#install-a-package)
   3. [Remove Packages](#remove-packages)
   4. [Upgrade Packages](#upgrade-packages)
   5. [List Installed Packages](#list-installed-packages)
   6. [Configuration Management](#configuration-management)
   7. [Package Management](#package-management)
   8. [Import and Export](#import-and-export)

4. [Architecture Detection](#architecture-detection)

---

## **Features**

- Install packages from GitHub, GitLab, Gitea, direct HTTP, and web sources via scraping.
- Automatically detect system architecture (x86_64, ARM64) and OS (Linux, macOS).
- Supports binaries, archives, AppImages, and compressed files.
- Rootless, user-level installation.
- Track multiple update channels (stable, preview, nightly).
- HTTP-backed providers for direct asset URLs (`direct`) and asset discovery from pages (`scraper`).

---

## **Installation**

### **Auto Install (Recommended)**

The easiest way to install **Upstream** is via the install script. This downloads the latest binary, sets it up in your user path, and enables self-updates.

#### Linux

```bash
curl -fsSL https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.bash | bash
```

#### Windows

```powershell
iwr -useb https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.ps1 | iex
```

#### MacOS

```zsh
curl -fsSL https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install/install.zsh | zsh
```

- Ensures **Upstream** can update itself automatically.

---

### **Install via Cargo (Crates.io)**

Since **Upstream** is published on crates.io, you can install it directly with Cargo:

```bash
cargo install upstream-rs
```

- Cargo builds the binary and places it in `$CARGO_HOME/bin` (usually `~/.cargo/bin`).
- Make sure this directory is in your `PATH`:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

- To update later:

```bash
cargo install --force upstream-rs
```

> ⚠️ Installing via Cargo **does not enable self-updates** via Upstream’s "upgrade" mechanism. Use the auto-install script for self-contained updates, or use cargo to update Upstream.

---

### **Manual Installation**

1. Download the [latest release](https://github.com/what386/upstream/releases/latest) for your platform.
2. For Unix-like systems, Ensure it is executable:

```bash
chmod +x path/to/upstream-rs
```

> ⚠️ Manual installation **does not enable self-updates**.
> To enable self-updates:

```bash
{path/to/upstream} install upstream what386/upstream-rs -k binary
```

---

### **Build from Source**

Requires [Rust and Cargo](https://www.rust-lang.org/tools/install):

```bash
git clone https://github.com/what386/upstream-rs.git
cd upstream
cargo build --release
```

Executable location:

```text
./target/release/upstream-rs
```

> Manual builds **do not enable self-updates**.

---

## **Usage**

All commands support `--help`:

```bash
upstream <command> --help
```

---

### **Initialize Hooks**

```bash
upstream init
```

- Hooks Upstream into your system’s PATH.
- Use `upstream init --clean` to remove existing hooks first.

---

### **Install a Package**

```bash
upstream install <name> <owner>/<repo> [--kind <type>] [--channel <channel>] [--desktop]
```

Example:

```bash
upstream install mytool foo/my-cool-app --kind binary --desktop
```

- `<name>` → local alias used for future management.
- `<repo_slug>` → repository identifier (`owner/repo`).
- `-k` / `--kind` → asset type (`auto`, `app-image`, `archive`, `compressed`, `binary`, `win-exe`, `checksum`).
- `-p` / `--provider` → provider to source from (`github`, `gitlab`, `gitea`, `scraper`, `direct`; default `github`).
- `-c` / `--channel` → track `stable`, `preview`, or `nightly` (default `stable`).
- `-t` / `--tag` → install a specific release tag.
- `-m` / `--match-pattern` → prefer assets matching a pattern.
- `-e` / `--exclude-pattern` → avoid assets matching a pattern.
- `-d` / `--desktop` → optional `.desktop` entry creation.

HTTP provider examples:

```bash
# Install directly from a file URL
upstream install awesomeapp https://example.com/awesomeapp.tar.gz -p direct -k archive

# Discover downloadable assets from a release page
upstream install mytool https://example.com/downloads -p scraper
```

---

### **Remove Packages**

```bash
upstream remove <package1> <package2> ... [--purge]
```

- Uninstall packages.
- `--purge` → also remove package-named config/cache/data directories and upstream-owned desktop/icon artifacts when present.

---

### **Upgrade Packages**

```bash
upstream upgrade [<package1> <package2> ...] [--force] [--check]
```

- Updates specified packages, or **all** if no names are given.
- `--force` → reinstall, even if up-to-date.
- `--check` → preview updates without applying them.

---

### **List Installed Packages**

```bash
upstream list [<package>]
```

- No arguments → list all installed packages with metadata.
- With a package name → show detailed metadata for that package.

---

### **Configuration Management**

```bash
upstream config <action> [options]
```

Available actions:

| Action  | Description                                                                                      |
| ------- | ------------------------------------------------------------------------------------------------ |
| `set`   | Set configuration keys (`key.path=value`). Example: `upstream config set github.apiToken=abc123` |
| `get`   | Retrieve keys. Example: `upstream config get github.apiToken`                                    |
| `list`  | List all keys and their values.                                                                  |
| `edit`  | Open configuration file in editor.                                                               |
| `reset` | Reset configuration to defaults.                                                                 |

---

### **Package Management**

```bash
upstream package <action> [options]
```

Available actions:

| Action     | Description                                                                                |
| ---------- | ------------------------------------------------------------------------------------------ |
| `pin`      | Pin a package to prevent upgrades. Example: `upstream package pin nvim`                    |
| `unpin`    | Unpin a package. Example: `upstream package unpin nvim`                                    |
| `metadata` | Show full package metadata as JSON. Example: `upstream package metadata nvim`              |
| `get-key`  | Read specific metadata keys. Example: `upstream package get-key nvim install_path version` |
| `set-key`  | Update metadata keys manually. Example: `upstream package set-key nvim is_pinned=false`    |

---

### **Import and Export**

```bash
upstream export <path> [--full]
upstream import <path>
```

- `export <path>` writes a manifest of installed packages.
- `export <path> --full` creates a full snapshot tarball of the upstream directory.
- `import <path>` restores packages from a manifest or full snapshot archive.

---

## **Architecture Detection**

Upstream automatically detects OS and CPU:

- Linux → x86, ARM
- macOS → x86, ARM

It selects the best asset for your system based on filename patterns and extensions.
If installs fail, please open an issue.
