use anyhow::Result;

use crate::app::operations;
use crate::app::cli::args::{Cli, Commands};

impl Cli {
    pub async fn run(self) -> Result<()> {
        match self.command {
            Commands::Install {
                channel,
                provider,
                package_kind,
                repo_slug,
                name,
            } => operations::install::run(
                channel,
                provider,
                package_kind,
                repo_slug,
                name,
            ).await,

            Commands::Remove { name } => {
                operations::remove::run(name)
            }
        }
    }
}
