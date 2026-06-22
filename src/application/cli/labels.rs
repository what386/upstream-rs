use std::fmt;

use crate::application::cli::arguments::{Commands, ConfigAction, HooksAction, PackageAction};

impl fmt::Display for Commands {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Commands::Install { .. } => write!(f, "install"),
            Commands::Build { .. } => write!(f, "build"),
            Commands::Remove { .. } => write!(f, "remove"),
            Commands::Rollback { list, prune, .. } => {
                if *list {
                    write!(f, "rollback --list")
                } else if prune.is_some() {
                    write!(f, "rollback --prune")
                } else {
                    write!(f, "rollback")
                }
            }
            Commands::Reinstall { .. } => write!(f, "reinstall"),
            Commands::Upgrade { .. } => write!(f, "upgrade"),
            Commands::List { .. } => write!(f, "list"),
            Commands::Changelog { .. } => write!(f, "changelog"),
            Commands::Docs { .. } => write!(f, "docs"),
            Commands::Probe { .. } => write!(f, "probe"),
            Commands::Search { .. } => write!(f, "search"),
            Commands::Find { .. } => write!(f, "find"),
            Commands::Config { action } => write!(f, "{action}"),
            Commands::Package { action } => write!(f, "{action}"),
            Commands::Hooks { action } => write!(f, "{action}"),
            Commands::Import { .. } => write!(f, "import"),
            Commands::Export { .. } => write!(f, "export"),
            Commands::Doctor { migrate, .. } if *migrate => write!(f, "doctor --migrate"),
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
