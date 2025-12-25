# Upstream Package Manager

**Upstream** is a rootless, GitHub-centric package manager designed to install and update software on Unix-like systems. It supports multiple asset types, tracks releases, and automatically selects the best asset for your system architecture.

---

## Features

- Install packages directly from git-like repository releases.
- Auto-detect system architecture (x86_64, ARM64) and OS (Linux, macOS).
- Supports multiple asset types: appimages, binaries, archives, and compressed files.
- Rootless, user-level installation.

---

## Installation

#### todo: install via bash script. this way, upstream can install/upgrade itself.

### Auto:

Run the following command (rootless):

```bash
curl -fsSL https://raw.githubusercontent.com/what386/upstream/main/install.sh | bash
```

### Manual:

#### Linux

1. Download the [latest release](https://github.com/what386/upstream/releases/latest) for your platform

#### MacOS

_(Coming soon? I don't have a Mac, so I can't test MacOS. It should work, though.)_

### Build from source

Clone the repository and build with cargo:

```bash
git clone https://github.com/what386/upstream.git
cd upstream
cargo build --release
```

The executable will be located in **./target/release/upstream-rs**

---

## Usage

Upstream provides a set of commands for installing, updating, managing, and inspecting packages.
For detailed information on flags and options, run:

```bash
upstream <command> --help
```

### Install a Package

```bash
upstream install <owner>/<repo> -k <type> -n <name>
```

Installs a package from a supported provider (e.g., GitHub).
Defaults to Github if the provider is not specified.

---

### Update Packages

```bash
upstream upgrade [<package>]
```

Installs available updates for all packages, or a specific package if provided.
To check for updates without installing them, use the "--check" flag.
Run without arguments to update all packages.

---

### Remove

```bash
upstream remove <package>
```

Uninstalls a package.

---

### List Packages

```bash
upstream list [<package>]
```

Displays metadata about packages.
Run without arguments to list all packages.

---

### Package Info

```bash
upstream info <package>
```

Shows install path, provider, asset type, last update, and other metadata.

---

## Architecture Detection

Upstream automatically detects your OS and CPU architecture:

- Linux (x86 or ARM)
- macOS (x86 or ARM)

It selects the most appropriate release asset by matching filename patterns and extensions.
If you encounter broken application installs, please open an issue.

---

## Configuration

You can set provider-specific configuration keys such as API tokens:

```bash
upstream config --set-key github.apiToken=xxx
```

Upstream uses tokens automatically when required.
A GitHub token is optional but recommended to avoid rate limits.
