use clap::{Parser, Subcommand};

use crate::models::common::enums::{Channel, Filetype, Provider};

#[derive(Parser)]
#[command(name = "upstream")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Initialize upstream by hooking into PATH
    #[arg(long, default_value_t = false)]
    pub init: bool,

    /// Clean initialization (remove existing hooks)
    #[arg(long,  default_value_t = false, requires = "init")]
    pub clean: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Install a new package
    Install {
        /// Repository identifier (e.g. `owner/repo`)
        repo_slug: String,

        /// Source provider hosting the repository
        #[arg(default_value_t = Provider::Github)]
        provider: Provider,

        /// File type to install (e.g. binary, archive, app-image)
        #[arg(short, long, value_enum)]
        kind: Filetype,

        /// Name to register the installed application under
        #[arg(short, long)]
        name: String,

        /// Update channel to track (stable, beta, nightly, etc.)
        #[arg(short, long, value_enum, default_value_t = Channel::Stable)]
        update_channel: Channel,

        /// Whether to create a .desktop entry
        #[arg(long, default_value_t = false)]
        create_entry: bool,
    },
    /// Remove one or more package(s)
    Remove {
        /// Names of packages to remove
        names: Vec<String>,

        /// Whether to remove all associated cached data
        #[arg(long, default_value_t = false)]
        purge: bool,
    },
    /// Upgrade one, several or all package(s)
    Upgrade {
        /// Optional list of packages to upgrade
        /// (upgrades all if omitted)
        names: Option<Vec<String>>,

        /// Force upgrade even if already up to date
        #[arg(long, default_value_t = false)]
        force: bool,

        /// Check for available upgrades without applying them
        #[arg(long, default_value_t = false)]
        check: bool,
    },
    /// List package metadata
    List {
        /// Optional package name for extra detail
        /// (Lists all packages if omitted)
        name: Option<String>,
    },
    /// Manage application configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Set one or more configuration values (format: key.path=value)
    Set {
        /// Configuration keys to set (e.g., "github.apiToken=abc123")
        keys: Vec<String>,
    },
    /// Get one or more configuration values (format: key.path)
    Get {
        /// Configuration keys to retrieve (e.g., "github.apiToken")
        keys: Vec<String>,
    },
    /// List all configuration keys and values
    List,
    /// Show the entire configuration as JSON
    Show,
    /// Open configuration file in editor
    Edit,
    /// Reset configuration to defaults
    Reset,
}
