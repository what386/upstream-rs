use clap::{Parser, Subcommand, ValueEnum};

use crate::models::common::enums::{Channel, Filetype, Provider};

#[derive(Parser)]
#[command(name = "upstream")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Install {
        provider: Provider,

        repo_slug: String,

        #[arg(short, long, value_enum)]
        package_kind: Filetype,

        #[arg(short, long, value_enum)]
        name: String,

        #[arg(short, long, value_enum, default_value_t = Channel::Stable)]
        channel: Channel,
    },
    Remove {
        name: String,
    },
}

#[derive(ValueEnum, Clone, Debug)]
pub enum Source {
    Github,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum InstallType {
    Placeholder,
}
