use clap::{Parser, Subcommand};

use crate::models::common::enums::{Channel, Filetype, Provider};

#[derive(Parser)]
#[command(name = "upstream")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Install {
        repo_slug: String,

        #[arg(default_value_t = Provider::Github)]
        provider: Provider,

        #[arg(short, long, value_enum)]
        kind: Filetype,

        #[arg(short, long, value_enum)]
        name: String,

        #[arg(short, long, value_enum, default_value_t = Channel::Stable)]
        channel: Channel,
    },
    Remove {
        names: Vec<String>,

        #[arg(long, default_value_t = false)]
        purge_option: bool,
    },
    Upgrade {
        names: Option<Vec<String>>,

        #[arg(long, default_value_t = false)]
        force_option: bool,
    },
    List {
        name: Option<String>,
    }
}
