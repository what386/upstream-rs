use anyhow::Result;

use crate::application::cli::arguments::{Cli, Commands};
use crate::application::operations;

impl Cli {
    pub async fn run(self) -> Result<()> {
        match self.command {
            Commands::Install {
                repo_slug,
                provider,
                kind,
                name,
                update_channel,
                create_entry,
            } => {
                operations::install::run(
                    repo_slug,
                    provider,
                    kind,
                    name,
                    update_channel,
                    create_entry,
                )
                .await
            }

            Commands::Remove {
                names,
                purge: purge_option,
            } => operations::remove::run(names, purge_option),

            Commands::Upgrade {
                names,
                force,
                check,
            } => operations::upgrade::run(names, force, check).await,

            Commands::List { name } => operations::list::run(name),

            Commands::Init { clean } => operations::init::run(clean),
        }
    }
}
