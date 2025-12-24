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

#### todo: install via bash script. this way, upstream-rs can install/upgrade itself.

### Linux

1. Download the [latest release](https://github.com/what386/upstream-rs/releases/latest) for your platform

### MacOS

_(Coming soon? I don't have a Mac, so I can't test MacOS. It should work, though.)_

### Build from source

Clone the repository and build with cargo:

```bash
git clone https://github.com/what386/upstream-rs.git
cd upstream-rs
cargo build --release
```

The executable will be located in **./target/release/upstream-rs**

---

## Usage

Upstream provides a set of commands for installing, updating, managing, and inspecting packages.
For detailed information on flags and options, run:

```bash
upstream-rs <command> --help
```

### Install a Package

```bash
upstream-rs install <owner>/<repo> -k <type> -n <name>
```

Installs a package from a supported provider (e.g., GitHub).
Defaults to Github if the provider is not specified.

---

### Update Packages

```bash
upstream-rs upgrade [<package>]
```

Installs available updates for all packages, or a specific package if provided.
To check for updates without installing them, use the "--check" flag.
Run without arguments to update all packages.

---

### Remove

```bash
upstream-rs remove <package>
```

Uninstalls a package.

---

### List Packages

```bash
upstream-rs list [<package>]
```

Displays metadata about packages.
Run without arguments to list all packages.

---

### Package Info

```bash
upstream-rs info <package>
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
upstream-rs config --set-key github.apiToken=xxx
```

Upstream uses tokens automatically when required.
A GitHub token is optional but recommended to avoid rate limits.
