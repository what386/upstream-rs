use crate::models::common::enums::{Channel, Filetype, Provider};
use clap::{Parser, Subcommand};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum BuildProfile {
    Rust,
    Dotnet,
    Go,
    Zig,
    Cmake,
}

#[derive(Parser)]
#[command(name = "upstream")]
#[command(about = "A package manager for everything else.")]
#[command(
    long_about = "Upstream is a lightweight package manager that installs and manages \
    applications from most software sources that dont have their own package manager.\n\n\
    Install binaries, AppImages, and other artifacts with automatic updates, \
    version pinning, and (hopefully) minimal configuration.\n\n\
    EXAMPLES:\n  \
    upstream install nvim neovim/neovim --desktop\n  \
    upstream upgrade                # Upgrade all packages\n  \
    upstream list                   # Show installed packages\n  \
    upstream config set github.api_token=ghp_xxx"
)]
#[command(version)]
pub struct Cli {
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
        upstream install rg BurntSushi/ripgrep --ignore-checksums")]
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

        /// Skip checksum verification for downloaded assets
        #[arg(long, default_value_t = false)]
        ignore_checksums: bool,

        /// Accept the recommended discovered asset without prompting
        #[arg(long, short = 'y', default_value_t = false)]
        yes: bool,
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

        /// Repository identifier (e.g. `owner/repo`)
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

        /// Accept the recommended discovered source/release without prompting
        #[arg(long, short = 'y', default_value_t = false)]
        yes: bool,

        /// Build profile used to compile/install from source (auto-detected when omitted)
        #[arg(long, value_enum)]
        build_profile: Option<BuildProfile>,

        /// Optional explicit output path for the compiled executable
        #[arg(long)]
        build_output: Option<String>,
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

    /// Reinstall one or more packages (remove then install)
    #[command(
        long_about = "Reinstall packages by uninstalling and then installing them again.\n\n\
        Reinstall uses each package's stored source metadata. Release installs attempt \
        the currently recorded version tag; build installs rebuild from source.\n\n\
        EXAMPLES:\n  \
        upstream reinstall nvim\n  \
        upstream reinstall rg fd\n  \
        upstream reinstall rg --ignore-checksums"
    )]
    Reinstall {
        /// Names of packages to reinstall
        names: Vec<String>,

        /// Skip checksum verification for release-asset reinstalls
        #[arg(long, default_value_t = false)]
        ignore_checksums: bool,
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
        upstream upgrade --ignore-checksums")]
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

        /// Skip checksum verification for downloaded assets
        #[arg(long, default_value_t = false)]
        ignore_checksums: bool,
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
        upstream config set github.api_token=ghp_xxx\n  \
        upstream config get github.api_token\n  \
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
        upstream package remove nvim\n  \
        upstream package metadata nvim\n  \
        upstream package get-key nvim install_path"
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
        upstream hooks purge --yes")]
    Hooks {
        #[command(subcommand)]
        action: HooksAction,
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
    },
}

impl Commands {
    pub fn requires_lock(&self) -> bool {
        match self {
            Commands::List { .. } => false,
            Commands::Doctor { .. } => false,
            Commands::Hooks { action } => !matches!(action, HooksAction::Check),
            Commands::Package { action } => !matches!(
                action,
                PackageAction::GetKey { .. } | PackageAction::Metadata { .. }
            ),
            Commands::Config { action } => {
                !matches!(action, ConfigAction::Get { .. } | ConfigAction::List)
            }
            Commands::Install { .. }
            | Commands::Build { .. }
            | Commands::Remove { .. }
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
        Pass --yes to skip the confirmation prompt.\n\n\
        EXAMPLE:\n  \
        upstream hooks purge --yes"
    )]
    Purge {
        /// Skip the confirmation prompt
        #[arg(long, short = 'y', default_value_t = false)]
        yes: bool,
    },
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
        keys: Vec<String>,
    },

    /// Get configuration values
    #[command(long_about = "Retrieve one or more configuration values.\n\n\
        Use dot notation to access nested keys.\n\n\
        EXAMPLES:\n  \
        upstream config get github.api_token\n  \
        upstream config get github.api_token gitlab.api_token")]
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

    /// Remove a package entry from upstream metadata
    #[command(long_about = "Delete a package from local upstream metadata.\n\n\
        This removes only metadata tracking. It does not remove installed files, \
        symlinks, or other runtime integrations.\n\n\
        EXAMPLE:\n  \
        upstream package remove nvim")]
    Remove {
        /// Name of package to remove from metadata
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

#[cfg(test)]
mod tests {
    use super::{BuildProfile, Cli, Commands, ConfigAction, HooksAction, PackageAction};
    use clap::Parser;

    #[test]
    fn install_parses_ignore_checksums_flag() {
        let cli = Cli::parse_from([
            "upstream",
            "install",
            "rg",
            "BurntSushi/ripgrep",
            "--ignore-checksums",
        ]);

        match cli.command {
            Commands::Install {
                ignore_checksums, ..
            } => assert!(ignore_checksums),
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn install_provider_is_optional_and_yes_is_parsed() {
        let cli = Cli::parse_from([
            "upstream",
            "install",
            "tool",
            "https://example.test",
            "--yes",
        ]);

        match cli.command {
            Commands::Install { provider, yes, .. } => {
                assert!(provider.is_none());
                assert!(yes);
            }
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
    fn upgrade_parses_ignore_checksums_flag() {
        let cli = Cli::parse_from(["upstream", "upgrade", "--ignore-checksums"]);

        match cli.command {
            Commands::Upgrade {
                ignore_checksums, ..
            } => assert!(ignore_checksums),
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn reinstall_parses_names_and_ignore_checksums_flag() {
        let cli = Cli::parse_from(["upstream", "reinstall", "rg", "fd", "--ignore-checksums"]);

        match cli.command {
            Commands::Reinstall {
                names,
                ignore_checksums,
            } => {
                assert_eq!(names, vec!["rg".to_string(), "fd".to_string()]);
                assert!(ignore_checksums);
            }
            other => panic!("unexpected command parsed: {}", other),
        }
    }

    #[test]
    fn package_remove_parses_name() {
        let cli = Cli::parse_from(["upstream", "package", "remove", "ripgrep"]);

        match cli.command {
            Commands::Package {
                action: PackageAction::Remove { name },
            } => assert_eq!(name, "ripgrep"),
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

        let cli = Cli::parse_from(["upstream", "hooks", "purge", "--yes"]);
        assert!(matches!(
            cli.command,
            Commands::Hooks {
                action: HooksAction::Purge { yes: true }
            }
        ));
    }

    #[test]
    fn init_command_is_removed() {
        assert!(Cli::try_parse_from(["upstream", "init"]).is_err());
    }

    #[test]
    fn requires_lock_skips_read_only_commands() {
        assert!(!Commands::List { name: None }.requires_lock());
        assert!(
            !Commands::Doctor {
                names: vec![],
                verbose: false,
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
            !Commands::Package {
                action: PackageAction::GetKey {
                    name: "ripgrep".to_string(),
                    keys: vec!["version".to_string()],
                },
            }
            .requires_lock()
        );
        assert!(
            !Commands::Package {
                action: PackageAction::Metadata {
                    name: "ripgrep".to_string(),
                },
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
                action: ConfigAction::List,
            }
            .requires_lock()
        );
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
                ignore_checksums: false,
                yes: false,
            }
            .requires_lock()
        );
        assert!(
            Commands::Upgrade {
                names: None,
                force: false,
                check: true,
                machine_readable: false,
                ignore_checksums: false,
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
                yes: false,
                build_profile: Some(BuildProfile::Rust),
                build_output: None,
            }
            .requires_lock()
        );
        assert!(
            Commands::Reinstall {
                names: vec!["ripgrep".to_string()],
                ignore_checksums: false,
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
                action: PackageAction::Remove {
                    name: "ripgrep".to_string(),
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
                action: HooksAction::Purge { yes: true },
            }
            .requires_lock()
        );
    }
}
