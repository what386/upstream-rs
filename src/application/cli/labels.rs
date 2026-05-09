use std::fmt;

use crate::application::cli::arguments::{Commands, ConfigAction, HooksAction, PackageAction};

impl fmt::Display for Commands {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Commands::Install { .. } => write!(f, "install"),
            Commands::Build { .. } => write!(f, "build"),
            Commands::Remove { .. } => write!(f, "remove"),
            Commands::Rollback { .. } => write!(f, "rollback"),
            Commands::Reinstall { .. } => write!(f, "reinstall"),
            Commands::Upgrade { .. } => write!(f, "upgrade"),
            Commands::List { .. } => write!(f, "list"),
            Commands::Probe { .. } => write!(f, "probe"),
            Commands::Search { .. } => write!(f, "search"),
            Commands::Config { action } => write!(f, "{action}"),
            Commands::Package { action } => write!(f, "{action}"),
            Commands::Hooks { action } => write!(f, "{action}"),
            Commands::Import { .. } => write!(f, "import"),
            Commands::Export { .. } => write!(f, "export"),
            Commands::Doctor { .. } => write!(f, "doctor"),
        }
    }
}

impl fmt::Display for ConfigAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigAction::Set { .. } => write!(f, "config set"),
            ConfigAction::Get { .. } => write!(f, "config get"),
            ConfigAction::List => write!(f, "config list"),
            ConfigAction::Edit => write!(f, "config edit"),
            ConfigAction::Reset => write!(f, "config reset"),
        }
    }
}

impl fmt::Display for HooksAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HooksAction::Init => write!(f, "hooks init"),
            HooksAction::Check => write!(f, "hooks check"),
            HooksAction::Clean => write!(f, "hooks clean"),
            HooksAction::Purge { .. } => write!(f, "hooks purge"),
        }
    }
}

impl fmt::Display for PackageAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackageAction::Pin { .. } => write!(f, "package pin"),
            PackageAction::Unpin { .. } => write!(f, "package unpin"),
            PackageAction::Remove { .. } => write!(f, "package remove"),
            PackageAction::GetKey { .. } => write!(f, "package get-key"),
            PackageAction::SetKey { .. } => write!(f, "package set-key"),
            PackageAction::Rename { .. } => write!(f, "package rename"),
            PackageAction::Metadata { .. } => write!(f, "package metadata"),
        }
    }
}
