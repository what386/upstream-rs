use anyhow::Result;

use crate::application::cli::arguments::{
    Cli, Commands, ConfigAction, HooksAction, ImportAs, PackageAction,
};
use crate::application::commands;
use crate::output;
use crate::services::storage::lock_storage::LockStorage;
use crate::utils::static_paths::UpstreamPaths;

impl Cli {
    pub async fn run(self) -> Result<()> {
        output::set_assume_yes(self.yes);
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
                HooksAction::Purge => commands::hooks::run_hooks_purge(),
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
                    build_profile,
                    build_output,
                    dry_run,
                )
                .await
            }

            Commands::Remove {
                names,
                purge: purge_option,
                force,
                dry_run,
            } => commands::remove::run(names, purge_option, force, dry_run),

            Commands::Rollback {
                names,
                prune,
                dry_run,
            } => commands::rollback::run(names, prune, dry_run),

            Commands::Reinstall {
                names,
                trust_mode,
                force,
                dry_run,
            } => commands::reinstall::run(names, trust_mode, force, dry_run).await,

            Commands::Upgrade {
                names,
                force,
                check,
                machine_readable,
                trust_mode,
                dry_run,
            } => {
                commands::upgrade::run(names, force, check, machine_readable, trust_mode, dry_run)
                    .await
            }

            Commands::List { name, json } => commands::list::run(name, json),

            Commands::Changelog {
                name,
                from_tag,
                to_tag,
            } => commands::changelog::run(name, from_tag, to_tag).await,

            Commands::Probe {
                repo_slug,
                provider,
                base_url,
                channel,
                limit,
                verbose,
            } => commands::probe::run(repo_slug, provider, base_url, channel, limit, verbose).await,
            Commands::Search {
                query_words,
                provider,
                base_url,
                limit,
            } => commands::search::run(query_words, provider, base_url, limit).await,

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
                PackageAction::Rename { old_name, new_name } => {
                    commands::package::run_rename(old_name, new_name)
                }
            },

            Commands::Export { path, full } => commands::export::run_export(path, full).await,
            Commands::Import {
                path,
                skip_failed,
                import_as,
            } => {
                let forced_kind = import_as.map(|value| match value {
                    ImportAs::Keys => commands::import::ImportKindArg::Keys,
                    ImportAs::Manifest => commands::import::ImportKindArg::Manifest,
                    ImportAs::Snapshot => commands::import::ImportKindArg::Snapshot,
                });
                commands::import::run_import(path, skip_failed, forced_kind).await
            }
            Commands::Doctor {
                names,
                verbose,
                fix,
            } => commands::doctor::run(names, verbose, fix),
        }
    }
}
