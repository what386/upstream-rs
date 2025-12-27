# Upstream Package Manager

**Upstream** is a rootless, GitHub-centric package manager for Unix-like systems. It installs and updates software from releases, supports multiple asset types, tracks update channels, and automatically selects the best asset for your OS and CPU architecture.

---

## **Table of Contents**

1. [Features](#features)
2. [Installation](#installation)

   1. [Auto Install (Recommended)](#auto-install-recommended)
   2. [Install via Cargo (Crates.io)](#install-via-cargo-cratesio)
   3. [Manual Installation (Linux)](#manual-installation-linux)
   4. [Manual Installation (MacOS)](#manual-installation-macos)
   5. [Build from Source](#build-from-source)
3. [Usage](#usage)

   1. [Initialize Hooks](#initialize-hooks)
   2. [Install a Package](#install-a-package)
   3. [Remove Packages](#remove-packages)
   4. [Upgrade Packages](#upgrade-packages)
   5. [List Installed Packages](#list-installed-packages)
   6. [Configuration Management](#configuration-management)
   7. [View Package Info](#view-package-info)
4. [Architecture Detection](#architecture-detection)

---

## **Features**

* Install packages directly from GitHub repository releases.
* Automatically detect system architecture (x86_64, ARM64) and OS (Linux, macOS).
* Supports binaries, archives, AppImages, and compressed files.
* Rootless, user-level installation.
* Track multiple update channels (stable, beta, nightly).

---

## **Installation**

### **Auto Install (Recommended)**

The easiest way to install **Upstream** is via the install script. This downloads the latest binary, sets it up in your user path, and enables self-updates.

```bash
curl -fsSL https://raw.githubusercontent.com/what386/upstream/main/install.sh | bash
```

* Ensures **Upstream** can update itself automatically.

---

### **Install via Cargo (Crates.io)**

Since **Upstream** is published on crates.io, you can install it directly with Cargo:

```bash
cargo install upstream-rs
```

* Cargo builds the binary and places it in `$CARGO_HOME/bin` (usually `~/.cargo/bin`).
* Make sure this directory is in your `PATH`:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

* To update later:

```bash
cargo install --force upstream-rs
```

> ⚠️ Installing via Cargo **does not enable self-updates** via Upstream’s "upgrade" mechanism. Use the auto-install script for self-contained updates.

---

### **Manual Installation (Linux)**

1. Download the [latest release](https://github.com/what386/upstream/releases/latest) for your platform.
2. Ensure it is executable:

```bash
chmod +x path/to/upstream-rs
```

> ⚠️ Manual installation **does not enable self-updates**.
> To enable self-updates:

```bash
{path/to/upstream-rs} install what386/upstream-rs -k binary -n upstream
```

---

### **Manual Installation (MacOS)**

MacOS support is experimental. Running Upstream on MacOS **may** work, but testing is limited. Please report any issues.

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

## **Usage**

All commands support `--help`:

```bash
upstream <command> --help
```

---

### **Initialize Hooks**

```bash
upstream --init
```

* Hooks Upstream into your system’s PATH.
* Use `upstream --clean` to remove existing hooks.

---

### **Install a Package**

```bash
upstream install <owner>/<repo> -k <type> -n <name> [--update-channel <channel>] [--create-entry]
```

Example:

```bash
upstream install foo/my-cool-app -k binary -n mytool --create-entry
```

* `repo_slug` → repository identifier (`owner/repo`).
* `-k` / `--kind` → asset type (`binary`, `archive`, `appimage`, `compressed`).
* `-n` / `--name` → local alias.
* `-p` / `--provider` → provider to source from (defaults to `Github`)
* `--update-channel` → track `stable`, `beta`, or `nightly`. (defaults to `Stable`)
* `--create-entry` → optional .desktop entry creation.

---

### **Remove Packages**

```bash
upstream remove <package1> <package2> ... [--purge]
```

* Uninstall packages.
* `--purge` → remove configuration data. (currently does not work.)

---

### **Upgrade Packages**

```bash
upstream upgrade [<package1> <package2> ...] [--force] [--check]
```

* Updates specified packages, or **all** if no names are given.
* `--force` → reinstall, even if up-to-date.
* `--check` → preview updates without applying them.

---

### **List Installed Packages**

```bash
upstream list [<package>]
```

* No arguments → list all installed packages with metadata.
* With a package name → show detailed metadata for that package.

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
| `show`  | Show full configuration as JSON.                                                                 |
| `edit`  | Open configuration file in editor.                                                               |
| `reset` | Reset configuration to defaults.                                                                 |

---

### **View Package Info**

```bash
upstream info <package>
```

Shows metadata: install path, provider, asset type, update channel, last update, and more.

---

## **Architecture Detection**

Upstream automatically detects OS and CPU:

* Linux → x86, ARM
* macOS → x86, ARM

It selects the best asset for your system based on filename patterns and extensions.
If installs fail, please open an issue.
