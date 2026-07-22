use anyhow::Result;

use crate::application::cli::arguments::{
    AuthAction, CacheAction, Cli, Commands, ConfigAction, ExportAction, HooksAction, ImportAction,
    PackageAction,
};
use crate::application::commands;
use crate::models::upstream::config::AppConfig;
use crate::output;
use crate::storage::system::lock::LockStorage;
use crate::utils::static_paths::UpstreamPaths;

impl Cli {
    pub async fn run(self, paths: &UpstreamPaths, app_config: &AppConfig) -> Result<()> {
        output::set_assume_yes(self.yes);
        output::set_no_pager(self.no_pager);
        let command = self.command;
        let operation = command.to_string();
        let _lock = if command.requires_lock() {
            Some(LockStorage::acquire(paths, &operation)?)
        } else {
            None
        };

        match command {
            Commands::Add {
                name,
                fetch,
                dry_run,
            } => commands::add::run(name, fetch, dry_run, paths, app_config).await,
            Commands::Hooks { action } => match action {
                HooksAction::Init => commands::hooks::run_hooks_init(paths),
                HooksAction::Check => commands::hooks::run_hooks_check(paths),
                HooksAction::Clean => commands::hooks::run_hooks_clean(paths),
                HooksAction::Purge => commands::hooks::run_hooks_purge(paths),
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
                    paths,
                    app_config,
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
                    paths,
                    app_config,
                )
                .await
            }

            Commands::Remove {
                names,
                purge: purge_option,
                force,
                dry_run,
            } => commands::remove::run(names, purge_option, force, dry_run, paths),

            Commands::Rollback {
                names,
                list,
                prune,
                dry_run,
            } => commands::rollback::run(names, list, prune, dry_run, paths),

            Commands::Reinstall {
                names,
                trust_mode,
                force,
                dry_run,
            } => {
                commands::reinstall::run(names, trust_mode, force, dry_run, paths, app_config).await
            }

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
                    paths,
                    app_config,
                )
                .await
            }

            Commands::List { filter, json } => commands::list::run(filter, json, paths),

            Commands::Info { query, json } => commands::info::run(query, json, paths),

            Commands::History {
                package,
                action,
                status,
                limit,
                since,
                today,
                json,
            } => commands::history::run(package, action, status, limit, since, today, json, paths),

            Commands::Changelog {
                name,
                from_tag,
                to_tag,
                for_tag,
            } => commands::changelog::run(name, from_tag, to_tag, for_tag, paths, app_config).await,

            Commands::Docs {
                name,
                offline,
                fetch,
                keywords,
            } => commands::docs::run(name, keywords, offline, fetch, paths, app_config).await,

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
                    paths,
                    app_config,
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
                    paths,
                    app_config,
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
                    paths,
                    app_config,
                )
                .await
            }

            Commands::Config { action } => match action {
                ConfigAction::Set { keys } => commands::config::run_set(keys, paths),
                ConfigAction::Get { keys } => commands::config::run_get(keys, paths),
                ConfigAction::List => commands::config::run_list(paths),
                ConfigAction::Edit => commands::config::run_edit(paths),
                ConfigAction::Reset => commands::config::run_reset(paths),
            },

            Commands::Auth { action } => match action {
                AuthAction::Set { keys } => commands::auth::run_set(keys, paths),
                AuthAction::Get { keys } => commands::auth::run_get(keys, paths),
                AuthAction::List => commands::auth::run_list(paths),
                AuthAction::Edit => commands::auth::run_edit(paths),
                AuthAction::Reset => commands::auth::run_reset(paths),
            },

            Commands::Package { action } => match action {
                PackageAction::Set { name, settings } => {
                    commands::package::run_set(name, settings, paths)
                }
                PackageAction::Get { name, keys, json } => {
                    commands::package::run_get(name, keys, json, paths)
                }
                PackageAction::Unset { name, keys } => {
                    commands::package::run_unset(name, keys, paths)
                }
                PackageAction::Pin { name } => commands::package::run_pin(name, paths),
                PackageAction::Unpin { name } => commands::package::run_unpin(name, paths),
                PackageAction::Rename { old_name, new_name } => {
                    commands::package::run_rename(old_name, new_name, paths)
                }
                PackageAction::AddEntry { name } => {
                    commands::package::run_add_entry(name, paths).await
                }
                PackageAction::RmEntry { name } => {
                    commands::package::run_rm_entry(name, paths).await
                }
            },

            Commands::Cache { action } => match action {
                CacheAction::List { json } => commands::cache::run_list(json, paths),
                CacheAction::Clean {
                    categories,
                    dry_run,
                } => commands::cache::run_clean(categories, dry_run, paths),
            },

            Commands::Export { action } => match action {
                ExportAction::Config { path } => commands::export::run_export_config(path, paths),
                ExportAction::Keys { path } => commands::export::run_export_keys(path, paths),
                ExportAction::Packages { path } => {
                    commands::export::run_export_packages(path, paths)
                }
                ExportAction::Profile { path } => commands::export::run_export_profile(path, paths),
            },
            Commands::Import { action } => match action {
                ImportAction::Config { path } => commands::import::run_import_config(path, paths),
                ImportAction::Keys { path } => {
                    commands::import::run_import_keys(path, paths, app_config)
                }
                ImportAction::Packages {
                    path,
                    skip_failed,
                    latest,
                } => {
                    commands::import::run_import_packages(
                        path,
                        skip_failed,
                        latest,
                        paths,
                        app_config,
                    )
                    .await
                }
                ImportAction::Profile {
                    path,
                    skip_failed,
                    latest,
                } => commands::import::run_import_profile(path, skip_failed, latest, paths).await,
            },
            Commands::Doctor {
                names,
                verbose,
                fix,
                json,
            } => commands::doctor::run(names, verbose, fix, json, paths).await,
        }
    }
}
