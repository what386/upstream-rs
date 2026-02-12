use anyhow::Result;

use crate::application::cli::arguments::{Cli, Commands, ConfigAction, PackageAction};
use crate::application::features;

impl Cli {
    pub async fn run(self) -> Result<()> {
        match self.command {
            Commands::Init { clean } => features::init::run(clean),
            Commands::Install {
                name,
                repo_slug,
                kind,
                tag,
                provider,
                base_url,
                channel,
                match_pattern,
                exclude_pattern,
                desktop,
            } => {
                features::install::run(
                    name,
                    repo_slug,
                    kind,
                    tag,
                    provider,
                    base_url,
                    channel,
                    match_pattern,
                    exclude_pattern,
                    desktop,
                )
                .await
            }

            Commands::Remove {
                names,
                purge: purge_option,
            } => features::remove::run(names, purge_option),

            Commands::Upgrade {
                names,
                force,
                check,
            } => features::upgrade::run(names, force, check).await,

            Commands::List { name } => features::list::run(name),

            Commands::Config { action } => match action {
                ConfigAction::Set { keys } => features::config::run_set(keys),
                ConfigAction::Get { keys } => features::config::run_get(keys),
                ConfigAction::List => features::config::run_list(),
                ConfigAction::Show => features::config::run_show(),
                ConfigAction::Edit => features::config::run_edit(),
                ConfigAction::Reset => features::config::run_reset(),
            },

            Commands::Package { action } => match action {
                PackageAction::Pin { name } => features::package::run_pin(name),
                PackageAction::Unpin { name } => features::package::run_unpin(name),
                PackageAction::SetKey { name, keys } => features::package::run_set_key(name, keys),
                PackageAction::GetKey { name, keys } => features::package::run_get_key(name, keys),
                PackageAction::Metadata { name } => features::package::run_metadata(name),
            },

            Commands::Export { path, full } => features::export::run_export(path, full).await,
            Commands::Import { path } => features::import::run_import(path).await,
        }
    }
}
