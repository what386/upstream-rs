# Upstream Package Manager

**Upstream** is a rootless, GitHub-centric package manager for Unix-like systems. It installs and updates software from releases, supports multiple asset types, tracks update channels, and automatically selects the best asset for your OS and CPU architecture.

---

## **Features**

* Install packages directly from git-like repository releases.
* Automatically detect system architecture (x86_64, ARM64) and OS (Linux, macOS).
* Supports binaries, archives, appimages, and compressed files.
* Rootless, user-level installation.
* Track multiple update channels (stable, beta, nightly).

---

## **Installation**

### **Auto (Recommended)**

The easiest way to install **Upstream** is via the install script. This downloads the latest binary, sets it up in your user path, and enables self-updates.

```bash
curl -fsSL https://raw.githubusercontent.com/what386/upstream/main/install.sh | bash
```

This method ensures **Upstream** can update itself in the future.

---

### **Manual Installation (Linux)**

1. Download the [latest release](https://github.com/what386/upstream/releases/latest) for your platform.

2. Ensure that it is executable:

```bash
chmod +x path/to/upstream-rs
```

> ⚠️ Manual installation **does not enable self-updates**.
> To enable self-updates, install Upstream through itself:

```bash
{path/to/upstream-rs} install what386/upstream-rs -k binary -n upstream
```

---

### **Manual Installation (MacOS)**

MacOS support is experimental. Running Upstream on MacOS **may** work, but testing is limited. Please report any issues.

---

### **Build from Source**

Requires both [Rust and Cargo](https://www.rust-lang.org/tools/install):

```bash
git clone https://github.com/what386/upstream-rs.git
cd upstream-rs
cargo build --release
```

Executable location:

```text
./target/release/upstream-rs
```

---

## **Usage**

All commands can display help:

```bash
upstream <command> --help
```

---

### **Initialize hooks**

```bash
upstream --init
```

* Hooks Upstream into your system's PATH, for command-line applications.
* Use `--clean` to remove existing hooks.

---

### **Install a Package**

```bash
upstream install <owner>/<repo> -k <type> -n <name> [--update-channel <channel>] [--create-entry]
```

Example:

```bash
upstream install what386/mytool -k binary -n mytool --update-channel stable --create-entry
```

* `repo_slug` → repository identifier (e.g., `owner/repo`).
* `-k` / `--kind` → asset type (`binary`, `archive`, `appimage`, `compressed`).
* `-n` / `--name` → local alias for the installed package.
* `--update-channel` → track `stable`, `beta`, or `nightly` releases.
* `--create-entry` → optional .desktop entry creation.

---

### **Remove Packages**

```bash
upstream remove <package1> <package2> ... [--purge]
```

* Uninstall one or more packages.
* `--purge` → also remove configuration data.

---

### **Upgrade Packages**

```bash
upstream upgrade [<package1> <package2> ...] [--force] [--check]
```

* Updates specified packages or **all packages** if no names are given.
* `--force` → reinstall even if already up to date.
* `--check` → see available updates without applying them.

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

| Action  | Description                                                                                                  |
| ------- | ------------------------------------------------------------------------------------------------------------ |
| `set`   | Set one or more configuration keys (`key.path=value`). Example: `upstream config set github.apiToken=abc123` |
| `get`   | Retrieve one or more keys. Example: `upstream config get github.apiToken`                                    |
| `list`  | List all keys and their values.                                                                              |
| `show`  | Show the full configuration as JSON.                                                                         |
| `edit`  | Open the configuration file in your editor.                                                                  |
| `reset` | Reset configuration to defaults.                                                                             |

---

### **View Package Info**

```bash
upstream info <package>
```

Shows metadata like install path, provider, asset type, update channel, last update, and more.

---

## **Architecture Detection**

Upstream automatically detects your OS and CPU:

* Linux → x86, ARM
* macOS → x86, ARM

It selects the best asset for your system based on filename patterns and extensions.
If installs fail, please open an issue.
