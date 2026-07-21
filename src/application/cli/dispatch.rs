use anyhow::Result;

use crate::application::cli::arguments::{
    AuthAction, Cli, Commands, ConfigAction, ExportAction, HooksAction, ImportAction, PackageAction,
};
use crate::application::commands;
use crate::output;
use crate::storage::system::lock::LockStorage;
use crate::utils::static_paths::UpstreamPaths;

impl Cli {
    pub async fn run(self, paths: UpstreamPaths) -> Result<()> {
        output::set_assume_yes(self.yes);
        output::set_no_pager(self.no_pager);
        let command = self.command;
        let operation = command.to_string();
        let _lock = if command.requires_lock() {
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
                semver,
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
                    semver,
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
                semver,
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
                    semver,
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

            Commands::List { filter, json } => commands::list::run(filter, json),

            Commands::Info { query, json } => commands::info::run(query, json),

            Commands::History {
                package,
                action,
                status,
                limit,
                since,
                today,
                json,
            } => commands::history::run(package, action, status, limit, since, today, json),

            Commands::Changelog {
                name,
                from_tag,
                to_tag,
                for_tag,
            } => commands::changelog::run(name, from_tag, to_tag, for_tag).await,

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

            Commands::Auth { action } => match action {
                AuthAction::Set { keys } => commands::auth::run_set(keys),
                AuthAction::Get { keys } => commands::auth::run_get(keys),
                AuthAction::List => commands::auth::run_list(),
                AuthAction::Edit => commands::auth::run_edit(),
                AuthAction::Reset => commands::auth::run_reset(),
            },

            Commands::Package { action } => match action {
                PackageAction::Pin { name } => commands::package::run_pin(name),
                PackageAction::Unpin { name } => commands::package::run_unpin(name),
                PackageAction::Rename { old_name, new_name } => {
                    commands::package::run_rename(old_name, new_name)
                }
                PackageAction::AddEntry { name } => commands::package::run_add_entry(name).await,
                PackageAction::RmEntry { name } => commands::package::run_rm_entry(name).await,
            },

            Commands::Export { action } => match action {
                ExportAction::Config { path } => commands::export::run_export_config(path),
                ExportAction::Keys { path } => commands::export::run_export_keys(path),
                ExportAction::Packages { path } => commands::export::run_export_packages(path),
                ExportAction::Profile { path } => commands::export::run_export_profile(path),
            },
            Commands::Import { action } => match action {
                ImportAction::Config { path } => commands::import::run_import_config(path),
                ImportAction::Keys { path } => commands::import::run_import_keys(path),
                ImportAction::Packages {
                    path,
                    skip_failed,
                    latest,
                } => commands::import::run_import_packages(path, skip_failed, latest).await,
                ImportAction::Profile {
                    path,
                    skip_failed,
                    latest,
                } => commands::import::run_import_profile(path, skip_failed, latest).await,
            },
            Commands::Doctor {
                names,
                verbose,
                fix,
                json,
            } => commands::doctor::run(names, verbose, fix, json).await,
        }
    }
}
