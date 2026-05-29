use crate::models::common::enums::{Channel, Filetype, Provider, TrustMode};
use clap::{Parser, Subcommand};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum BuildProfile {
    Rust,
    Dotnet,
    Go,
    Zig,
    Cmake,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ImportAs {
    Keys,
    Manifest,
    Snapshot,
}

#[derive(Parser)]
#[command(name = "upstream")]
#[command(about = "A package manager for everything else.")]
#[command(
    long_about = "Upstream is a lightweight package manager that installs and manages \
    applications from most software sources that do not have their own package manager.\n\n\
    Install binaries, AppImages, and other artifacts with automatic updates, \
    version pinning, and minimal configuration.\n\n\
    EXAMPLES:\n  \
    upstream install nvim neovim/neovim --desktop\n  \
    upstream upgrade                # Upgrade all packages\n  \
    upstream list                   # Show installed packages\n  \
    upstream config set github.api_token=ghp_xxx"
)]
#[command(
    version,
    long_version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("GIT_HASH"), ")")
)]
pub struct Cli {
    /// Accept confirmation prompts
    #[arg(short = 'y', long, global = true, default_value_t = false)]
    pub yes: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Install a package from an upstream release source
    #[command(long_about = "Install a new package from a download source.\n\n\
        Downloads the specified file type from the latest release (or specified channel) \
        and registers it under the given name for future updates.\n\n\
        EXAMPLES:\n  \
        upstream install rg BurntSushi/ripgrep -k binary\n  \
        upstream install dust bootandy/dust -k archive\n  \
        upstream install rg BurntSushi/ripgrep --trust none")]
    Install {
        /// Name to register the application under
        name: String,

        /// Repository identifier or URL
        repo_slug: String,

        /// Version tag to install (defaults to latest)
        #[arg(short, long)]
        tag: Option<String>,

        /// File type to install
        #[arg(short, long, value_enum, default_value_t = Filetype::Auto)]
        kind: Filetype,

        /// Source provider hosting the repository. Defaults to auto-detection.
        #[arg(short = 'p', long)]
        provider: Option<Provider>,

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

        /// Trust verification mode for downloaded assets
        #[arg(long = "trust", value_enum, default_value_t = TrustMode::BestEffort)]
        trust_mode: TrustMode,

        /// Preview install resolution without downloading or writing files
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Build and install from source for release tags without artifacts
    #[command(long_about = "Build and install a package from source.\n\n\
        Use this command when release tags exist but prebuilt artifacts are missing \
        or unsuitable for your system.\n\n\
        Mirrors install-style source resolution, with optional automatic profile detection.\n\n\
        EXAMPLES:\n  \
        upstream build rg BurntSushi/ripgrep\n  \
        upstream build rg BurntSushi/ripgrep --branch main\n  \
        upstream build rg BurntSushi/ripgrep --build-profile rust\n  \
        upstream build app owner/repo --build-profile dotnet --tag v1.2.3\n  \
        upstream build tool owner/repo --build-profile rust --build-output target/release/tool")]
    Build {
        /// Name to register the application under
        name: String,

        /// Repository identifier or URL
        repo_slug: String,

        /// Version tag to build (defaults to latest)
        #[arg(short, long, conflicts_with = "branch")]
        tag: Option<String>,

        /// Branch name to build from (uses latest commit from that branch)
        #[arg(long, conflicts_with = "tag")]
        branch: Option<String>,

        /// Source provider hosting the repository. Defaults to auto-detection.
        #[arg(short = 'p', long)]
        provider: Option<Provider>,

        /// Custom base URL. Defaults to provider's root
        #[arg(long, requires = "provider")]
        base_url: Option<String>,

        /// Update channel to track
        #[arg(short, long, value_enum, default_value_t = Channel::Stable)]
        channel: Channel,

        /// Whether or not to create a .desktop entry for GUI applications
        #[arg(short, long, default_value_t = false)]
        desktop: bool,

        /// Build profile used to compile/install from source (auto-detected when omitted)
        #[arg(long, value_enum)]
        build_profile: Option<BuildProfile>,

        /// Optional explicit output path for the compiled executable
        #[arg(long)]
        build_output: Option<String>,

        /// Preview build resolution without compiling or writing files
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Remove one or more installed packages
    #[command(
        long_about = "Uninstall packages and optionally remove cached data.\n\n\
        By default, removes the package binary/files but preserves cached release data. \
        Use --purge to remove everything. Use --force to ignore uninstall errors \
        (for example, missing files) and still remove package metadata.\n\n\
        EXAMPLES:\n  \
        upstream remove nvim\n  \
        upstream remove rg fd bat --purge\n  \
        upstream remove rg --force\n  \
        upstream remove rg --dry-run"
    )]
    Remove {
        /// Names of packages to remove
        names: Vec<String>,

        /// Remove all associated cached data
        #[arg(long, default_value_t = false)]
        purge: bool,

        /// Ignore uninstall errors and remove metadata anyway
        #[arg(long, default_value_t = false)]
        force: bool,

        /// Preview removal actions without deleting files or metadata
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Restore or prune stored rollback artifacts
    #[command(long_about = "Manage package rollback points.\n\n\
        Restore previously captured installs, or prune stored rollback artifacts.\n\n\
        EXAMPLES:\n  \
        upstream rollback rg\n  \
        upstream rollback rg fd --dry-run\n  \
        upstream rollback --prune\n  \
        upstream rollback --prune rg")]
    Rollback {
        /// Package names to restore or prune
        names: Vec<String>,

        /// Prune rollback artifacts instead of restoring
        #[arg(long, default_value_t = false)]
        prune: bool,

        /// Preview rollback/prune actions without modifying files or metadata
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Reinstall one or more packages (remove then install)
    #[command(
        long_about = "Reinstall packages by uninstalling and then installing them again.\n\n\
        Reinstall uses each package's stored source metadata. Release installs attempt \
        the currently recorded version tag; build installs rebuild from source.\n\n\
        EXAMPLES:\n  \
        upstream reinstall nvim\n  \
        upstream reinstall rg fd\n  \
        upstream reinstall rg --force\n  \
        upstream reinstall rg --trust none"
    )]
    Reinstall {
        /// Names of packages to reinstall
        names: Vec<String>,

        /// Trust verification mode for release-asset reinstalls
        #[arg(long = "trust", value_enum, default_value_t = TrustMode::BestEffort)]
        trust_mode: TrustMode,

        /// Ignore uninstall errors and remove metadata anyway before reinstalling
        #[arg(long, default_value_t = false)]
        force: bool,

        /// Preview reinstall resolution without removing, building, or writing files
        #[arg(long, default_value_t = false)]
        dry_run: bool,
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
        upstream upgrade nvim --force # Force reinstall\n  \
        upstream upgrade --trust none")]
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

        /// Trust verification mode for downloaded assets
        #[arg(long = "trust", value_enum, default_value_t = TrustMode::BestEffort)]
        trust_mode: TrustMode,

        /// Preview upgrade resolution without downloading or writing files
        #[arg(long, default_value_t = false)]
        dry_run: bool,
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

        /// Print raw package metadata as JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Show upstream release notes for an installed package
    #[command(long_about = "Show release notes for an installed package.\n\n\
        By default, prints release bodies newer than the installed version up to \
        the latest release for the package's tracked channel. Use --from and --to \
        to override the range endpoints by release tag.\n\n\
        EXAMPLES:\n  \
        upstream changelog nvim\n  \
        upstream changelog nvim --from v0.10.0\n  \
        upstream changelog nvim --from v0.10.0 --to v0.11.0")]
    Changelog {
        /// Installed package name
        name: String,

        /// Override the starting release tag
        #[arg(long = "from")]
        from_tag: Option<String>,

        /// Override the ending release tag
        #[arg(long = "to")]
        to_tag: Option<String>,
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

    /// Search provider repositories by keyword(s)
    #[command(long_about = "Search for repositories on a provider.\n\n\
        Defaults to GitHub when provider is omitted.\n\n\
        EXAMPLES:\n  \
        upstream search ripgrep\n  \
        upstream search rip grep --limit 5\n  \
        upstream search my tool -p github\n  \
        upstream search widget -p gitlab --base-url https://gitlab.example.com")]
    Search {
        /// Query words (joined with spaces)
        #[arg(required = true, num_args(1..), value_delimiter = ' ')]
        query_words: Vec<String>,

        /// Source provider to search (defaults to github)
        #[arg(short = 'p', long)]
        provider: Option<Provider>,

        /// Custom base URL for self-hosted providers
        #[arg(long, requires = "provider")]
        base_url: Option<String>,

        /// Maximum number of results to display
        #[arg(long, default_value_t = 10)]
        limit: u32,
    },

    /// Manage upstream configuration
    #[command(long_about = "View and modify upstream's configuration.\n\n\
        Configuration is stored in TOML format and includes settings like \
        API tokens, default providers, and installation preferences.\n\n\
        EXAMPLES:\n  \
        upstream config set github.api_token=ghp_xxx\n  \
        upstream config get trust\n  \
        upstream config list\n  \
        upstream config edit")]
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Manage package-specific behavior
    #[command(
        long_about = "Control package behavior.\n\n\
        Pin packages to prevent upgrades or rename installed package aliases.\n\n\
        EXAMPLES:\n  \
        upstream package pin nvim\n  \
        upstream package unpin nvim\n  \
        upstream package rename nvim neovim"
    )]
    Package {
        #[command(subcommand)]
        action: PackageAction,
    },

    /// Manage shell integration hooks and local upstream data
    #[command(long_about = "Manage upstream shell integration hooks.\n\n\
        Use these commands to add, verify, or remove shell PATH hooks. \
        Purge removes shell hooks and deletes the local upstream data directory.\n\n\
        EXAMPLES:\n  \
        upstream hooks init\n  \
        upstream hooks check\n  \
        upstream hooks clean\n  \
        upstream --yes hooks purge")]
    Hooks {
        #[command(subcommand)]
        action: HooksAction,
    },

    /// Import trusted keys, package metadata manifests, or full snapshots
    #[command(
        long_about = "Import trusted keys, package metadata manifests, or full snapshots.\n\n\
        Autodetects the input type by content/extension, prompts for confirmation by default, \
        and then performs the selected import operation.\n\n\
        EXAMPLES:\n  \
        upstream import ./minisign.pub            # Import trusted minisign keys\n  \
        upstream import ./cosign.pub              # Import trusted cosign PEM keys\n  \
        upstream import ./packages.json           # Import package metadata manifest\n  \
        upstream import ./backup.tar.gz           # Restore full snapshot\n  \
        upstream --yes import ./input.bin --as keys"
    )]
    Import {
        /// Path to a keys file, metadata manifest, or snapshot archive
        path: std::path::PathBuf,

        /// Continue importing remaining entries when metadata manifest processing fails
        #[arg(long, default_value_t = false)]
        skip_failed: bool,

        /// Force the input type instead of autodetection
        #[arg(long = "as", value_enum)]
        import_as: Option<ImportAs>,
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
        Reports a compact summary by default and includes actionable hints. \
        Use --verbose to print each individual check result.\n\n\
        EXAMPLES:\n  \
        upstream doctor\n  \
        upstream doctor --verbose\n  \
        upstream doctor nvim ripgrep"
    )]
    Doctor {
        /// Package names to check (all installed packages if omitted)
        names: Vec<String>,

        /// Print each check result line in addition to summary output
        #[arg(long, default_value_t = false)]
        verbose: bool,

        /// Attempt automatic repairs for detected issues
        #[arg(long, default_value_t = false)]
        fix: bool,
    },
}

impl Commands {
    pub fn requires_lock(&self) -> bool {
        match self {
            Commands::List { .. } => false,
            Commands::Changelog { .. } => false,
            Commands::Doctor { fix, .. } => *fix,
            Commands::Search { .. } => false,
            Commands::Hooks { action } => !matches!(action, HooksAction::Check),
            Commands::Package { .. } => true,
            Commands::Config { action } => {
                !matches!(action, ConfigAction::Get { .. } | ConfigAction::List { .. })
            }
            Commands::Install { .. }
            | Commands::Build { .. }
            | Commands::Remove { .. }
            | Commands::Rollback { .. }
            | Commands::Reinstall { .. }
            | Commands::Upgrade { .. }
            | Commands::Probe { .. }
            | Commands::Import { .. }
            | Commands::Export { .. } => true,
        }
    }
}

#[derive(Subcommand)]
pub enum HooksAction {
    /// Add upstream shell integration hooks
    #[command(
        long_about = "Add upstream shell integration hooks and create required local directories.\n\n\
        EXAMPLE:\n  \
        upstream hooks init"
    )]
    Init,

    /// Check upstream shell integration hooks
    #[command(
        long_about = "Check upstream shell integration hooks and required local directories.\n\n\
        EXAMPLE:\n  \
        upstream hooks check"
    )]
    Check,

    /// Remove upstream shell integration hooks
    #[command(
        long_about = "Remove upstream shell integration hooks without deleting installed package data.\n\n\
        EXAMPLE:\n  \
        upstream hooks clean"
    )]
    Clean,

    /// Remove hooks and delete the local upstream data directory
    #[command(
        long_about = "Remove upstream shell integration hooks and delete the local upstream data directory.\n\n\
        This deletes installed package files and metadata under ~/.upstream. \
        Pass global --yes to skip the confirmation prompt.\n\n\
        EXAMPLE:\n  \
        upstream --yes hooks purge"
    )]
    Purge,
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Set configuration values
    #[command(long_about = "Set one or more configuration values.\n\n\
        Use dot notation for nested keys. Multiple key=value pairs can be set at once.\n\n\
        EXAMPLES:\n  \
        upstream config set github.api_token=ghp_xxx\n  \
        upstream config set gitlab.api_token=glpat_xxx")]
    Set {
        /// Configuration assignments (format: key.path=value)
        #[arg(required = true)]
        keys: Vec<String>,
    },

    /// Get configuration values
    #[command(long_about = "Retrieve one or more configuration values.\n\n\
        Use dot notation to access nested keys.\n\n\
        EXAMPLES:\n  \
        upstream config get trust\n  \
        upstream config get github.api_token gitlab.api_token")]
    Get {
        /// Configuration keys to retrieve (format: key.path)
        #[arg(required = true)]
        keys: Vec<String>,
    },

    /// List all configuration keys
    List {
        /// Print sensitive values instead of redacting them
        #[arg(long, default_value_t = false)]
        show_secrets: bool,
    },

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

        /// Optional reason for pinning this package
        #[arg(long)]
        reason: Option<String>,
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

}

#[cfg(test)]
mod tests {
    use super::{BuildProfile, Cli, Commands, ConfigAction, HooksAction, ImportAs, PackageAction};
    use crate::models::common::enums::TrustMode;
    use clap::Parser;

    #[test]
    fn install_parses_trust_mode_flag() {
        let cli = Cli::parse_from([
            "upstream",
            "install",
            "rg",
            "BurntSushi/ripgrep",
            "--trust",
            "none",
        ]);
        assert!(!cli.yes);

        match cli.command {
            Commands::Install { trust_mode, .. } => assert_eq!(trust_mode, TrustMode::None),
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn install_provider_is_optional_and_global_yes_is_parsed() {
        let cli = Cli::parse_from([
            "upstream",
            "--yes",
            "install",
            "tool",
            "https://example.test",
        ]);
        assert!(cli.yes);

        match cli.command {
            Commands::Install { provider, .. } => {
                assert!(provider.is_none());
            }
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn global_yes_applies_to_commands_without_local_yes_flag() {
        let cli = Cli::parse_from(["upstream", "--yes", "remove", "rg"]);
        assert!(cli.yes);

        match cli.command {
            Commands::Remove { names, .. } => assert_eq!(names, vec!["rg".to_string()]),
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn install_parses_dry_run_flag() {
        let cli = Cli::parse_from(["upstream", "install", "tool", "owner/repo", "--dry-run"]);

        match cli.command {
            Commands::Install { dry_run, .. } => assert!(dry_run),
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn build_parses_profile_and_output_flags() {
        let cli = Cli::parse_from([
            "upstream",
            "build",
            "rg",
            "BurntSushi/ripgrep",
            "--build-profile",
            "rust",
            "--build-output",
            "target/release/rg",
        ]);

        match cli.command {
            Commands::Build {
                provider,
                branch,
                build_profile,
                build_output,
                ..
            } => {
                assert!(provider.is_none());
                assert!(branch.is_none());
                assert_eq!(build_profile, Some(BuildProfile::Rust));
                assert_eq!(build_output.as_deref(), Some("target/release/rg"));
            }
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn build_profile_is_optional() {
        let cli = Cli::parse_from(["upstream", "build", "rg", "BurntSushi/ripgrep"]);
        match cli.command {
            Commands::Build { build_profile, .. } => assert!(build_profile.is_none()),
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn build_parses_dry_run_flag() {
        let cli = Cli::parse_from(["upstream", "build", "rg", "BurntSushi/ripgrep", "--dry-run"]);
        match cli.command {
            Commands::Build { dry_run, .. } => assert!(dry_run),
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn build_parses_branch_flag() {
        let cli = Cli::parse_from([
            "upstream",
            "build",
            "rg",
            "BurntSushi/ripgrep",
            "--branch",
            "main",
        ]);
        match cli.command {
            Commands::Build { branch, tag, .. } => {
                assert_eq!(branch.as_deref(), Some("main"));
                assert!(tag.is_none());
            }
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn build_rejects_tag_and_branch_together() {
        assert!(
            Cli::try_parse_from([
                "upstream",
                "build",
                "rg",
                "BurntSushi/ripgrep",
                "--tag",
                "v1.0.0",
                "--branch",
                "main",
            ])
            .is_err()
        );
    }

    #[test]
    fn build_rejects_match_and_exclude_flags() {
        assert!(
            Cli::try_parse_from([
                "upstream",
                "build",
                "rg",
                "BurntSushi/ripgrep",
                "--match",
                "linux",
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from([
                "upstream",
                "build",
                "rg",
                "BurntSushi/ripgrep",
                "--exclude",
                "debug",
            ])
            .is_err()
        );
    }

    #[test]
    fn upgrade_parses_trust_mode_flag() {
        let cli = Cli::parse_from(["upstream", "upgrade", "--trust", "signature"]);

        match cli.command {
            Commands::Upgrade { trust_mode, .. } => assert_eq!(trust_mode, TrustMode::Signature),
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn reinstall_parses_names_and_trust_mode_flag() {
        let cli = Cli::parse_from(["upstream", "reinstall", "rg", "fd", "--trust", "all"]);

        match cli.command {
            Commands::Reinstall {
                names, trust_mode, ..
            } => {
                assert_eq!(names, vec!["rg".to_string(), "fd".to_string()]);
                assert_eq!(trust_mode, TrustMode::All);
            }
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn reinstall_parses_force_flag() {
        let cli = Cli::parse_from(["upstream", "reinstall", "rg", "--force"]);
        match cli.command {
            Commands::Reinstall { force, .. } => assert!(force),
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn upgrade_parses_dry_run_flag() {
        let cli = Cli::parse_from(["upstream", "upgrade", "--dry-run"]);
        match cli.command {
            Commands::Upgrade { dry_run, .. } => assert!(dry_run),
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn changelog_parses_range_flags() {
        let cli = Cli::parse_from([
            "upstream",
            "changelog",
            "nvim",
            "--from",
            "v0.10.0",
            "--to",
            "v0.11.0",
        ]);

        match cli.command {
            Commands::Changelog {
                name,
                from_tag,
                to_tag,
            } => {
                assert_eq!(name, "nvim");
                assert_eq!(from_tag.as_deref(), Some("v0.10.0"));
                assert_eq!(to_tag.as_deref(), Some("v0.11.0"));
            }
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn reinstall_parses_dry_run_flag() {
        let cli = Cli::parse_from(["upstream", "reinstall", "rg", "--dry-run"]);
        match cli.command {
            Commands::Reinstall { dry_run, .. } => assert!(dry_run),
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn remove_parses_dry_run_flag() {
        let cli = Cli::parse_from(["upstream", "remove", "rg", "--dry-run"]);
        match cli.command {
            Commands::Remove { dry_run, .. } => assert!(dry_run),
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn rollback_parses_prune_and_dry_run_flags() {
        let cli = Cli::parse_from(["upstream", "rollback", "rg", "--prune", "--dry-run"]);
        match cli.command {
            Commands::Rollback {
                names,
                prune,
                dry_run,
            } => {
                assert_eq!(names, vec!["rg".to_string()]);
                assert!(prune);
                assert!(dry_run);
            }
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn remove_parses_force_flag() {
        let cli = Cli::parse_from(["upstream", "remove", "rg", "--force"]);
        match cli.command {
            Commands::Remove { force, .. } => assert!(force),
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn import_parses_as_flag() {
        let cli = Cli::parse_from(["upstream", "import", "minisign.pub", "--as", "keys"]);

        match cli.command {
            Commands::Import {
                path,
                skip_failed,
                import_as,
            } => {
                assert_eq!(path, std::path::PathBuf::from("minisign.pub"));
                assert!(!skip_failed);
                assert_eq!(import_as, Some(ImportAs::Keys));
            }
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn doctor_parses_verbose_flag() {
        let cli = Cli::parse_from(["upstream", "doctor", "--verbose"]);

        match cli.command {
            Commands::Doctor { verbose, .. } => assert!(verbose),
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn hooks_parses_actions() {
        let cli = Cli::parse_from(["upstream", "hooks", "init"]);
        assert!(matches!(
            cli.command,
            Commands::Hooks {
                action: HooksAction::Init
            }
        ));

        let cli = Cli::parse_from(["upstream", "hooks", "purge"]);
        assert!(matches!(
            cli.command,
            Commands::Hooks {
                action: HooksAction::Purge
            }
        ));
    }

    #[test]
    fn init_command_is_removed() {
        assert!(Cli::try_parse_from(["upstream", "init"]).is_err());
    }

    #[test]
    fn requires_lock_skips_read_only_commands() {
        assert!(
            !Commands::List {
                name: None,
                json: false,
            }
            .requires_lock()
        );
        assert!(
            !Commands::Doctor {
                names: vec![],
                verbose: false,
                fix: false,
            }
            .requires_lock()
        );
        assert!(
            !Commands::Hooks {
                action: HooksAction::Check,
            }
            .requires_lock()
        );
        assert!(
            !Commands::Config {
                action: ConfigAction::Get {
                    keys: vec!["github.api_token".to_string()],
                },
            }
            .requires_lock()
        );
        assert!(
            !Commands::Config {
                action: ConfigAction::List {
                    show_secrets: false,
                },
            }
            .requires_lock()
        );
        assert!(
            !Commands::Search {
                query_words: vec!["ripgrep".to_string()],
                provider: None,
                base_url: None,
                limit: 10,
            }
            .requires_lock()
        );
    }

    #[test]
    fn search_parses_variadic_query_words_and_limit() {
        let cli = Cli::parse_from(["upstream", "search", "rip", "grep", "--limit", "5"]);

        match cli.command {
            Commands::Search {
                query_words,
                provider,
                base_url,
                limit,
            } => {
                assert_eq!(query_words, vec!["rip".to_string(), "grep".to_string()]);
                assert!(provider.is_none());
                assert!(base_url.is_none());
                assert_eq!(limit, 5);
            }
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn search_parses_provider_and_base_url() {
        let cli = Cli::parse_from([
            "upstream",
            "search",
            "ripgrep",
            "--provider",
            "gitlab",
            "--base-url",
            "https://gitlab.example.com",
        ]);

        match cli.command {
            Commands::Search {
                query_words,
                provider,
                base_url,
                limit,
            } => {
                assert_eq!(query_words, vec!["ripgrep".to_string()]);
                assert_eq!(
                    provider,
                    Some(crate::models::common::enums::Provider::Gitlab)
                );
                assert_eq!(base_url.as_deref(), Some("https://gitlab.example.com"));
                assert_eq!(limit, 10);
            }
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn search_requires_provider_when_base_url_is_set() {
        assert!(
            Cli::try_parse_from([
                "upstream",
                "search",
                "ripgrep",
                "--base-url",
                "https://gitlab.example.com",
            ])
            .is_err()
        );
    }

    #[test]
    fn search_requires_query_words() {
        assert!(Cli::try_parse_from(["upstream", "search"]).is_err());
    }

    #[test]
    fn config_set_and_get_require_operands() {
        assert!(Cli::try_parse_from(["upstream", "config", "set"]).is_err());
        assert!(Cli::try_parse_from(["upstream", "config", "get"]).is_err());
    }

    #[test]
    fn config_list_parses_show_secrets() {
        let cli = Cli::parse_from(["upstream", "config", "list", "--show-secrets"]);

        match cli.command {
            Commands::Config {
                action: ConfigAction::List { show_secrets },
            } => assert!(show_secrets),
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn requires_lock_keeps_writing_and_side_effectful_commands_locked() {
        assert!(
            Commands::Install {
                name: "ripgrep".to_string(),
                repo_slug: "BurntSushi/ripgrep".to_string(),
                tag: None,
                kind: crate::models::common::enums::Filetype::Auto,
                provider: Some(crate::models::common::enums::Provider::Github),
                base_url: None,
                channel: crate::models::common::enums::Channel::Stable,
                match_pattern: None,
                exclude_pattern: None,
                desktop: false,
                trust_mode: TrustMode::BestEffort,
                dry_run: false,
            }
            .requires_lock()
        );
        assert!(
            Commands::Upgrade {
                names: None,
                force: false,
                check: true,
                machine_readable: false,
                trust_mode: TrustMode::BestEffort,
                dry_run: false,
            }
            .requires_lock()
        );
        assert!(
            Commands::Build {
                name: "ripgrep".to_string(),
                repo_slug: "BurntSushi/ripgrep".to_string(),
                tag: None,
                branch: None,
                provider: Some(crate::models::common::enums::Provider::Github),
                base_url: None,
                channel: crate::models::common::enums::Channel::Stable,
                desktop: false,
                build_profile: Some(BuildProfile::Rust),
                build_output: None,
                dry_run: false,
            }
            .requires_lock()
        );
        assert!(
            Commands::Reinstall {
                names: vec!["ripgrep".to_string()],
                trust_mode: TrustMode::BestEffort,
                force: false,
                dry_run: false,
            }
            .requires_lock()
        );
        assert!(
            Commands::Rollback {
                names: vec!["ripgrep".to_string()],
                prune: false,
                dry_run: true,
            }
            .requires_lock()
        );
        assert!(
            Commands::Remove {
                names: vec!["ripgrep".to_string()],
                purge: false,
                force: false,
                dry_run: true,
            }
            .requires_lock()
        );
        assert!(
            Commands::Config {
                action: ConfigAction::Set {
                    keys: vec!["github.api_token=ghp_xxx".to_string()],
                },
            }
            .requires_lock()
        );
        assert!(
            Commands::Package {
                action: PackageAction::Pin {
                    name: "ripgrep".to_string(),
                    reason: None,
                },
            }
            .requires_lock()
        );
        assert!(
            Commands::Export {
                path: "packages.json".into(),
                full: false,
            }
            .requires_lock()
        );
        assert!(
            Commands::Config {
                action: ConfigAction::Edit,
            }
            .requires_lock()
        );
        assert!(
            Commands::Config {
                action: ConfigAction::Reset,
            }
            .requires_lock()
        );
        assert!(
            Commands::Hooks {
                action: HooksAction::Init,
            }
            .requires_lock()
        );
        assert!(
            Commands::Hooks {
                action: HooksAction::Clean,
            }
            .requires_lock()
        );
        assert!(
            Commands::Hooks {
                action: HooksAction::Purge,
            }
            .requires_lock()
        );
    }
}
