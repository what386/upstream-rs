# Upstream Package Manager

**Upstream** is a rootless, GitHub-centric package manager for Unix-like systems. It installs and updates software from releases, supports multiple asset types, tracks update channels, and automatically selects the best asset for your OS and CPU architecture.

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

- Install packages directly from GitHub repository releases.
- Automatically detect system architecture (x86_64, ARM64) and OS (Linux, macOS).
- Supports binaries, archives, AppImages, and compressed files.
- Rootless, user-level installation.
- Track multiple update channels (stable, nightly).

---

## **Installation**

### **Auto Install (Recommended)**

The easiest way to install **Upstream** is via the install script. This downloads the latest binary, sets it up in your user path, and enables self-updates.

#### Linux and MacOS

```bash
curl -fsSL https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install.sh | bash
```

#### Windows

```powershell
iwr -useb https://raw.githubusercontent.com/what386/upstream-rs/main/scripts/install.ps1 | iex
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

1. Download the [latest release](https://github.com/what386/upstream-rs/releases/latest) for your platform.
2. For Unix-like systems, Ensure it is executable:

```bash
chmod +x path/to/upstream-rs
```

> ⚠️ Manual installation **does not enable self-updates**.
> To enable self-updates:

```bash
{path/to/upstream-rs} install upstream what386/upstream-rs -k binary
```

---

### **Build from Source**

Requires [Rust and Cargo](https://www.rust-lang.org/tools/install):

```bash
git clone https://github.com/what386/upstream-rs.git
cd upstream-rs
cargo build --release
```

Executable location:

```text
./target/release/upstream-rs
```

> Manual builds **do not enable self-updates**.

---

### **Generate Shell Completions**

Generate pre-built completion files for bash, zsh, fish, and powershell:

```bash
./scripts/completions.sh
```

Files are written to:

```text
./completions/
```

Install generated completions:

#### Bash

```bash
mkdir -p ~/.local/share/bash-completion/completions
cp completions/upstream.bash ~/.local/share/bash-completion/completions/upstream
```

Restart your shell, or run:

```bash
source ~/.bashrc
```

#### Zsh

```bash
mkdir -p ~/.zfunc
cp completions/_upstream ~/.zfunc/_upstream
```

Ensure your `~/.zshrc` has:

```bash
fpath=(~/.zfunc $fpath)
autoload -Uz compinit && compinit
```

Then restart your shell, or run:

```bash
source ~/.zshrc
```

#### Fish

```bash
mkdir -p ~/.config/fish/completions
cp completions/upstream.fish ~/.config/fish/completions/upstream.fish
```

Open a new fish session.

#### PowerShell

```powershell
New-Item -ItemType Directory -Force "$HOME\Documents\PowerShell\Completions" | Out-Null
Copy-Item completions/_upstream.ps1 "$HOME\Documents\PowerShell\Completions\_upstream.ps1" -Force
Add-Content $PROFILE 'if (Test-Path "$HOME\Documents\PowerShell\Completions\_upstream.ps1") { . "$HOME\Documents\PowerShell\Completions\_upstream.ps1" }'
```

Restart PowerShell.

---

## **Usage**

All commands support `--help`:

```bash
upstream-rs <command> --help
```

---

### **Initialize Hooks**

```bash
upstream-rs init
```

- Hooks Upstream into your system’s PATH.
- Use `upstream-rs init --clean` to remove existing hooks first.

---

### **Install a Package**

```bash
upstream-rs install <name> <owner>/<repo> [--kind <type>] [--channel <channel>] [--desktop]
```

Example:

```bash
upstream-rs install mytool foo/my-cool-app --kind binary --desktop
```

- `<name>` → local alias used for future management.
- `<repo_slug>` → repository identifier (`owner/repo`).
- `-k` / `--kind` → asset type (`auto`, `app-image`, `archive`, `compressed`, `binary`, `win-exe`, `checksum`).
- `-p` / `--provider` → provider to source from (`github` or `gitlab`, default `github`).
- `-c` / `--channel` → track `stable` or `nightly` (default `stable`).
- `-t` / `--tag` → install a specific release tag.
- `-m` / `--match-pattern` → prefer assets matching a pattern.
- `-e` / `--exclude-pattern` → exclude assets matching a pattern.
- `-d` / `--desktop` → optional `.desktop` entry creation.

---

### **Remove Packages**

```bash
upstream-rs remove <package1> <package2> ... [--purge]
```

- Uninstall packages.
- `--purge` → remove configuration data. (currently does not work.)

---

### **Upgrade Packages**

```bash
upstream-rs upgrade [<package1> <package2> ...] [--force] [--check]
```

- Updates specified packages, or **all** if no names are given.
- `--force` → reinstall, even if up-to-date.
- `--check` → preview updates without applying them.

---

### **List Installed Packages**

```bash
upstream-rs list [<package>]
```

- No arguments → list all installed packages with metadata.
- With a package name → show detailed metadata for that package.

---

### **Configuration Management**

```bash
upstream-rs config <action> [options]
```

Available actions:

| Action  | Description                                                                                         |
| ------- | --------------------------------------------------------------------------------------------------- |
| `set`   | Set configuration keys (`key.path=value`). Example: `upstream-rs config set github.apiToken=abc123` |
| `get`   | Retrieve keys. Example: `upstream-rs config get github.apiToken`                                    |
| `list`  | List all keys and their values.                                                                     |
| `show`  | Show full configuration as JSON.                                                                    |
| `edit`  | Open configuration file in editor.                                                                  |
| `reset` | Reset configuration to defaults.                                                                    |

---

### **Package Management**

```bash
upstream-rs package <action> [options]
```

Available actions:

| Action     | Description                                                                                   |
| ---------- | --------------------------------------------------------------------------------------------- |
| `pin`      | Pin a package to prevent upgrades. Example: `upstream-rs package pin nvim`                    |
| `unpin`    | Unpin a package. Example: `upstream-rs package unpin nvim`                                    |
| `metadata` | Show full package metadata as JSON. Example: `upstream-rs package metadata nvim`              |
| `get-key`  | Read specific metadata keys. Example: `upstream-rs package get-key nvim install_path version` |
| `set-key`  | Update metadata keys manually. Example: `upstream-rs package set-key nvim is_pinned=false`    |

---

### **Import and Export**

```bash
upstream-rs export <path> [--full]
upstream-rs import <path>
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
