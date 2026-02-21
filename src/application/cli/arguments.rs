use crate::models::common::enums::{Channel, Filetype, Provider};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "upstream")]
#[command(about = "A package manager for Github releases.")]
#[command(
    long_about = "Upstream is a lightweight package manager that installs and manages \
    applications directly from GitHub releases (and other providers).\n\n\
    Install binaries, AppImages, and other artifacts with automatic updates, \
    version pinning, and simple configuration management.\n\n\
    EXAMPLES:\n  \
    upstream install nvim neovim/neovim --desktop\n  \
    upstream upgrade                # Upgrade all packages\n  \
    upstream list                   # Show installed packages\n  \
    upstream config set github.apiToken=ghp_xxx"
)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Install a package from a GitHub release
    #[command(long_about = "Install a new package from a repository release.\n\n\
        Downloads the specified file type from the latest release (or specified channel) \
        and registers it under the given name for future updates.\n\n\
        EXAMPLES:\n  \
        upstream install rg BurntSushi/ripgrep -k binary\n  \
        upstream install dust bootandy/dust -k archive")]
    Install {
        /// Name to register the application under
        name: String,

        /// Repository identifier (e.g. `owner/repo`)
        repo_slug: String,

        /// Version tag to install (defaults to latest)
        #[arg(short, long)]
        tag: Option<String>,

        /// File type to install
        #[arg(short, long, value_enum, default_value_t = Filetype::Auto)]
        kind: Filetype,

        /// Source provider hosting the repository
        #[arg(short = 'p', long, default_value_t = Provider::Github)]
        provider: Provider,

        /// Custom base URL. Defaults to provider's root
        #[arg(long, requires = "provider")]
        base_url: Option<String>,

        /// Update channel to track
        #[arg(short, long, value_enum, default_value_t = Channel::Stable)]
        channel: Channel,

        /// Match pattern to use as a hint for which asset to prefer
        #[arg(short = 'm', long, name = "match")]
        match_pattern: Option<String>,

        /// Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")
        #[arg(short = 'e', long, name = "exclude")]
        exclude_pattern: Option<String>,

        /// Whether or not to create a .desktop entry for GUI applications
        #[arg(short, long, default_value_t = false)]
        desktop: bool,
    },

    /// Remove one or more installed packages
    #[command(
        long_about = "Uninstall packages and optionally remove cached data.\n\n\
        By default, removes the package binary/files but preserves cached release data. \
        Use --purge to remove everything.\n\n\
        EXAMPLES:\n  \
        upstream remove nvim\n  \
        upstream remove rg fd bat --purge"
    )]
    Remove {
        /// Names of packages to remove
        names: Vec<String>,

        /// Remove all associated cached data
        #[arg(long, default_value_t = false)]
        purge: bool,
    },

    /// Upgrade installed packages to their latest versions
    #[command(long_about = "Check for and install updates to packages.\n\n\
        Without arguments, upgrades all packages. Specify package names to upgrade \
        only those packages. Use --check to preview available updates.\n\n\
        EXAMPLES:\n  \
        upstream upgrade              # Upgrade all\n  \
        upstream upgrade nvim rg      # Upgrade specific packages\n  \
        upstream upgrade --check      # Check for updates\n  \
        upstream upgrade --check --machine-readable # Script-friendly output\n  \
        upstream upgrade nvim --force # Force reinstall")]
    Upgrade {
        /// Packages to upgrade (upgrades all if omitted)
        names: Option<Vec<String>>,

        /// Force upgrade even if already up to date
        #[arg(long, default_value_t = false)]
        force: bool,

        /// Check for available upgrades without applying them
        #[arg(long, default_value_t = false)]
        check: bool,

        /// Use script-friendly check output: one line per update, "name oldver newver"
        #[arg(long, default_value_t = false, requires = "check")]
        machine_readable: bool,
    },

    /// List installed packages and their metadata
    #[command(long_about = "Display information about installed packages.\n\n\
        Without arguments, shows a summary of all installed packages. \
        Provide a package name to see detailed information.\n\n\
        EXAMPLES:\n  \
        upstream list       # List all packages\n  \
        upstream list nvim  # Show details for nvim")]
    List {
        /// Package name for detailed information
        name: Option<String>,
    },

    /// Inspect releases visible from a provider without installing
    #[command(long_about = "Probe a repository/source and show parsed releases.\n\n\
        Useful for validating what upstream can see before installation.\n\n\
        EXAMPLES:\n  \
        upstream probe neovim/neovim\n  \
        upstream probe https://ziglang.org/download/ -p scraper --limit 20\n  \
        upstream probe owner/repo --channel nightly --verbose")]
    Probe {
        /// Repository identifier or URL to probe
        repo_slug: String,

        /// Source provider (defaults to github, or scraper for URLs)
        #[arg(short = 'p', long)]
        provider: Option<Provider>,

        /// Custom base URL for self-hosted providers
        #[arg(long)]
        base_url: Option<String>,

        /// Channel view to display
        #[arg(short, long, value_enum, default_value_t = Channel::Stable)]
        channel: Channel,

        /// Maximum number of releases to display
        #[arg(long, default_value_t = 10)]
        limit: u32,

        /// Include scored candidate assets for each release
        #[arg(long, default_value_t = false)]
        verbose: bool,
    },

    /// Manage upstream configuration
    #[command(long_about = "View and modify upstream's configuration.\n\n\
        Configuration is stored in TOML format and includes settings like \
        API tokens, default providers, and installation preferences.\n\n\
        EXAMPLES:\n  \
        upstream config set github.apiToken=ghp_xxx\n  \
        upstream config get github.apiToken\n  \
        upstream config list\n  \
        upstream config edit")]
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Manage package-specific settings and metadata
    #[command(
        long_about = "Control package behavior and view internal metadata.\n\n\
        Pin packages to prevent upgrades, view installation details, or manually \
        adjust package metadata when needed.\n\n\
        EXAMPLES:\n  \
        upstream package pin nvim\n  \
        upstream package metadata nvim\n  \
        upstream package get-key nvim install_path"
    )]
    Package {
        #[command(subcommand)]
        action: PackageAction,
    },

    /// Initialize upstream by adding it to your shell PATH
    #[command(long_about = "Set up upstream for first-time use.\n\n\
        Adds upstream's bin directory to your PATH by modifying shell configuration \
        files (.bashrc, .zshrc, etc.). Run this once after installation.\n\n\
        EXAMPLES:\n  \
        upstream init\n  \
        upstream init --clean  # Remove old hooks first")]
    Init {
        /// Clean initialization (remove existing hooks first)
        #[arg(long)]
        clean: bool,

        /// Check initialization status without making changes
        #[arg(long, default_value_t = false, conflicts_with = "clean")]
        check: bool,
    },

    /// Import packages from a manifest or full snapshot
    #[command(
        long_about = "Import packages from a previously exported manifest or snapshot.\n\n\
        Reads a manifest and reinstalls each package, or restores a full snapshot \
        created with 'upstream export --full'. Packages that are already installed \
        will be skipped.\n\n\
        EXAMPLES:\n  \
        upstream import ./packages.json           # Import from manifest\n  \
        upstream import ./backup.tar.gz           # Restore full snapshot"
    )]
    Import {
        /// Path to the manifest or snapshot archive
        path: std::path::PathBuf,

        /// Continue importing remaining packages when a package install/upgrade fails
        #[arg(long, default_value_t = false)]
        skip_failed: bool,
    },

    /// Export packages to a manifest or full snapshot
    #[command(long_about = "Export installed packages for backup or transfer.\n\n\
        By default, writes a lightweight manifest containing just enough info to \
        reinstall each package. Use --full to instead create a tarball of the entire \
        upstream directory (a full snapshot).\n\n\
        EXAMPLES:\n  \
        upstream export ./packages.json           # Export manifest\n  \
        upstream export ./backup.tar.gz --full    # Full snapshot")]
    Export {
        /// Output path for the manifest or snapshot archive
        path: std::path::PathBuf,
        /// Export a full snapshot of the upstream directory instead of a manifest
        #[arg(long, default_value_t = false)]
        full: bool,
    },

    /// Run diagnostics to detect installation and integration issues
    #[command(
        long_about = "Inspect upstream installation health and package state.\n\n\
        Checks package paths, symlinks, shell PATH integration, and desktop/icon files. \
        Reports OK/WARN/FAIL with actionable hints.\n\n\
        EXAMPLES:\n  \
        upstream doctor\n  \
        upstream doctor nvim ripgrep"
    )]
    Doctor {
        /// Package names to check (all installed packages if omitted)
        names: Vec<String>,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Set configuration values
    #[command(long_about = "Set one or more configuration values.\n\n\
        Use dot notation for nested keys. Multiple key=value pairs can be set at once.\n\n\
        EXAMPLES:\n  \
        upstream config set github.apiToken=ghp_xxx\n  \
        upstream config set github.apiToken=ghp_xxx defaults.provider=github")]
    Set {
        /// Configuration assignments (format: key.path=value)
        keys: Vec<String>,
    },

    /// Get configuration values
    #[command(long_about = "Retrieve one or more configuration values.\n\n\
        Use dot notation to access nested keys.\n\n\
        EXAMPLES:\n  \
        upstream config get github.apiToken\n  \
        upstream config get github.apiToken defaults.provider")]
    Get {
        /// Configuration keys to retrieve (format: key.path)
        keys: Vec<String>,
    },

    /// List all configuration keys
    List,

    /// Open configuration file in your default editor
    Edit,

    /// Reset configuration to defaults
    Reset,
}

#[derive(Subcommand)]
pub enum PackageAction {
    /// Pin a package to its current version
    #[command(long_about = "Prevent a package from being upgraded.\n\n\
        Pinned packages are skipped during 'upstream upgrade' operations.\n\n\
        EXAMPLE:\n  \
        upstream package pin nvim")]
    Pin {
        /// Name of package to pin
        name: String,
    },

    /// Unpin a package to allow updates
    #[command(long_about = "Remove version pin from a package.\n\n\
        Unpinned packages will be included in future upgrade operations.\n\n\
        EXAMPLE:\n  \
        upstream package unpin nvim")]
    Unpin {
        /// Name of package to unpin
        name: String,
    },

    /// Get specific package metadata fields
    #[command(long_about = "Retrieve raw metadata values for a package.\n\n\
        Access internal package data like install paths, versions, and checksums.\n\n\
        EXAMPLES:\n  \
        upstream package get-key nvim install_path\n  \
        upstream package get-key nvim version checksum")]
    GetKey {
        /// Name of package
        name: String,

        /// Metadata keys to retrieve
        keys: Vec<String>,
    },

    /// Manually set package metadata fields
    #[command(long_about = "Manually modify package metadata.\n\n\
        Advanced operation - use with caution. Typically used for manual corrections \
        or testing.\n\n\
        EXAMPLE:\n  \
        upstream package set-key nvim is_pinned=false")]
    SetKey {
        /// Name of package
        name: String,

        /// Metadata assignments (format: key=value)
        keys: Vec<String>,
    },

    /// Rename package alias without reinstalling
    #[command(long_about = "Rename the local alias of an installed package.\n\n\
        This changes how upstream tracks the package and updates integration aliases \
        (symlink/desktop entry) when possible.\n\n\
        EXAMPLE:\n  \
        upstream package rename nvim neovim")]
    Rename {
        /// Existing package alias
        old_name: String,

        /// New package alias
        new_name: String,
    },

    /// Display all metadata for a package
    #[command(long_about = "Show complete package metadata in JSON format.\n\n\
        Displays all internal data for the specified package including installation \
        details, version info, and configuration.\n\n\
        EXAMPLE:\n  \
        upstream package metadata nvim")]
    Metadata {
        /// Name of package
        name: String,
    },
}
