use std::fmt;

use crate::application::cli::arguments::{
    Commands, ConfigAction, HooksAction, PackageAction, RollbackAction,
};

impl fmt::Display for Commands {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Commands::Install { .. } => write!(f, "install"),
            Commands::Build { .. } => write!(f, "build"),
            Commands::Remove { .. } => write!(f, "remove"),
            Commands::Rollback { action } => write!(f, "{action}"),
            Commands::Reinstall { .. } => write!(f, "reinstall"),
            Commands::Upgrade { .. } => write!(f, "upgrade"),
            Commands::List { .. } => write!(f, "list"),
            Commands::Changelog { .. } => write!(f, "changelog"),
            Commands::Probe { .. } => write!(f, "probe"),
            Commands::Search { .. } => write!(f, "search"),
            Commands::Find { .. } => write!(f, "find"),
            Commands::Config { action } => write!(f, "{action}"),
            Commands::Package { action } => write!(f, "{action}"),
            Commands::Hooks { action } => write!(f, "{action}"),
            Commands::Import { .. } => write!(f, "import"),
            Commands::Export { .. } => write!(f, "export"),
            Commands::Migrate => write!(f, "migrate"),
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
            HooksAction::Purge => write!(f, "hooks purge"),
        }
    }
}

impl fmt::Display for PackageAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackageAction::Pin { .. } => write!(f, "package pin"),
            PackageAction::Unpin { .. } => write!(f, "package unpin"),
            PackageAction::Rename { .. } => write!(f, "package rename"),
        }
    }
}

impl fmt::Display for RollbackAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RollbackAction::Restore { .. } => write!(f, "rollback restore"),
            RollbackAction::Prune { .. } => write!(f, "rollback prune"),
            RollbackAction::List => write!(f, "rollback list"),
        }
    }
}
