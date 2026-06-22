use anyhow::Result;

use crate::application::cli::arguments::{
    Cli, Commands, ConfigAction, HooksAction, ImportAs, PackageAction,
};
use crate::application::commands;
use crate::output;
use crate::storage::system::lock::LockStorage;
use crate::utils::static_paths::UpstreamPaths;

impl Cli {
    pub async fn run(self) -> Result<()> {
        output::set_assume_yes(self.yes);
        let command = self.command;
        let paths = UpstreamPaths::new()?;
        let _lock = if command.requires_lock() {
            let operation = command.to_string();
            Some(LockStorage::acquire(&paths, &operation)?)
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
                list,
                prune,
                dry_run,
            } => commands::rollback::run(names, list, prune, dry_run),

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
                json,
                trust_mode,
                dry_run,
            } => {
                commands::upgrade::run(
                    names,
                    force,
                    check,
                    machine_readable,
                    json,
                    trust_mode,
                    dry_run,
                )
                .await
            }

            Commands::List { name, json } => commands::list::run(name, json),

            Commands::Changelog {
                name,
                from_tag,
                to_tag,
            } => commands::changelog::run(name, from_tag, to_tag).await,

            Commands::Docs {
                name,
                offline,
                fetch,
                keywords,
            } => commands::docs::run(name, keywords, offline, fetch).await,

            Commands::Probe {
                repo_slug,
                name,
                provider,
                base_url,
                channel,
                limit,
                tag,
                kind,
                verbose,
                include_incompatible,
                json,
                desktop,
                trust_mode,
                dry_run,
            } => {
                commands::probe::run(
                    repo_slug,
                    name,
                    provider,
                    base_url,
                    channel,
                    limit,
                    tag,
                    kind,
                    verbose,
                    include_incompatible,
                    json,
                    desktop,
                    trust_mode,
                    dry_run,
                )
                .await
            }
            Commands::Search {
                query_words,
                provider,
                base_url,
                limit,
                language,
                topic,
                min_stars,
                max_stars,
                pushed_after,
                include_forks,
                include_archived,
                json,
            } => {
                commands::search::run(
                    query_words,
                    provider,
                    base_url,
                    limit,
                    language,
                    topic,
                    min_stars,
                    max_stars,
                    pushed_after,
                    include_forks,
                    include_archived,
                    json,
                )
                .await
            }
            Commands::Find {
                query_words,
                provider,
                base_url,
                limit,
                language,
                topic,
                min_stars,
                max_stars,
                pushed_after,
                include_forks,
                include_archived,
                name,
                kind,
                channel,
                match_pattern,
                exclude_pattern,
                desktop,
                trust_mode,
                dry_run,
            } => {
                commands::find::run(
                    query_words,
                    provider,
                    base_url,
                    limit,
                    language,
                    topic,
                    min_stars,
                    max_stars,
                    pushed_after,
                    include_forks,
                    include_archived,
                    name,
                    kind,
                    channel,
                    match_pattern,
                    exclude_pattern,
                    desktop,
                    trust_mode,
                    dry_run,
                )
                .await
            }

            Commands::Config { action } => match action {
                ConfigAction::Set { keys } => commands::config::run_set(keys),
                ConfigAction::Get { keys } => commands::config::run_get(keys),
                ConfigAction::List => commands::config::run_list(),
                ConfigAction::Edit => commands::config::run_edit(),
                ConfigAction::Reset => commands::config::run_reset(),
            },

            Commands::Package { action } => match action {
                PackageAction::Pin { name } => commands::package::run_pin(name),
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
                migrate,
                json,
            } => commands::doctor::run(names, verbose, fix, migrate, json).await,
        }
    }
}
