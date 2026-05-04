use anyhow::Result;

use crate::application::cli::arguments::{
    Cli, Commands, ConfigAction, HooksAction, ImportAs, PackageAction,
};
use crate::application::commands;
use crate::services::storage::lock_storage::LockStorage;
use crate::utils::static_paths::UpstreamPaths;

impl Cli {
    pub async fn run(self) -> Result<()> {
        let command = self.command;
        let paths = UpstreamPaths::new()?;
        let _lock = if command.requires_lock() {
            Some(LockStorage::acquire(&paths, &command)?)
        } else {
            None
        };

        match command {
            Commands::Hooks { action } => match action {
                HooksAction::Init => commands::hooks::run_hooks_init(),
                HooksAction::Check => commands::hooks::run_hooks_check(),
                HooksAction::Clean => commands::hooks::run_hooks_clean(),
                HooksAction::Purge { yes } => commands::hooks::run_hooks_purge(yes),
            },
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
                trust_mode,
                yes,
                dry_run,
            } => {
                commands::install::run(
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
                    trust_mode,
                    yes,
                    dry_run,
                )
                .await
            }
            Commands::Build {
                name,
                repo_slug,
                tag,
                branch,
                provider,
                base_url,
                channel,
                desktop,
                yes,
                build_profile,
                build_output,
                dry_run,
            } => {
                commands::build::run(
                    name,
                    repo_slug,
                    tag,
                    branch,
                    provider,
                    base_url,
                    channel,
                    desktop,
                    yes,
                    build_profile,
                    build_output,
                    dry_run,
                )
                .await
            }

            Commands::Remove {
                names,
                purge: purge_option,
            } => commands::remove::run(names, purge_option),

            Commands::Reinstall { names, trust_mode } => {
                commands::reinstall::run(names, trust_mode).await
            }

            Commands::Upgrade {
                names,
                force,
                check,
                machine_readable,
                trust_mode,
            } => commands::upgrade::run(names, force, check, machine_readable, trust_mode).await,

            Commands::List { name, json } => commands::list::run(name, json),

            Commands::Probe {
                repo_slug,
                provider,
                base_url,
                channel,
                limit,
                verbose,
            } => commands::probe::run(repo_slug, provider, base_url, channel, limit, verbose).await,

            Commands::Config { action } => match action {
                ConfigAction::Set { keys } => commands::config::run_set(keys),
                ConfigAction::Get { keys } => commands::config::run_get(keys),
                ConfigAction::List => commands::config::run_list(),
                ConfigAction::Edit => commands::config::run_edit(),
                ConfigAction::Reset => commands::config::run_reset(),
            },

            Commands::Package { action } => match action {
                PackageAction::Pin { name, reason } => commands::package::run_pin(name, reason),
                PackageAction::Unpin { name } => commands::package::run_unpin(name),
                PackageAction::Remove { name } => commands::package::run_remove(name),
                PackageAction::SetKey { name, keys } => commands::package::run_set_key(name, keys),
                PackageAction::Rename { old_name, new_name } => {
                    commands::package::run_rename(old_name, new_name)
                }
                PackageAction::GetKey { name, keys } => commands::package::run_get_key(name, keys),
                PackageAction::Metadata { name } => commands::package::run_metadata(name),
            },

            Commands::Export { path, full } => commands::export::run_export(path, full).await,
            Commands::Import {
                path,
                skip_failed,
                import_as,
                yes,
            } => {
                let forced_kind = import_as.map(|value| match value {
                    ImportAs::Keys => commands::import::ImportKindArg::Keys,
                    ImportAs::Manifest => commands::import::ImportKindArg::Manifest,
                    ImportAs::Snapshot => commands::import::ImportKindArg::Snapshot,
                });
                commands::import::run_import(path, skip_failed, forced_kind, yes).await
            }
            Commands::Doctor {
                names,
                verbose,
                fix,
            } => commands::doctor::run(names, verbose, fix),
        }
    }
}
