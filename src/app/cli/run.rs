use anyhow::Result;

use crate::app::operations;
use crate::app::cli::args::{Cli, Commands};

impl Cli {
    pub async fn run(self) -> Result<()> {
        match self.command {
            Commands::Install { repo_slug, provider, kind, name, channel } =>
                operations::install::run(repo_slug, provider, kind, name, channel ).await,

            Commands::Remove { names, purge_option } =>
                operations::remove::run(names, purge_option),

            Commands::Upgrade { names, force_option } =>
                operations::upgrade::run(names, force_option).await,

            Commands::List { name } =>
                operations::list::run(name),
        }
    }
}
