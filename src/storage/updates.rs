//! Disposable snapshots of known available updates. Readers never need a database.
use std::{collections::BTreeMap, fs, path::PathBuf};

use anyhow::{Context, Result, bail};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::{
    application::operations::upgrade_op::{UpdateCheckRow, UpdateCheckStatus},
    models::upstream::Package,
    utils::{filesystem::atomic_ops::write_atomic, static_paths::UpstreamPaths},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct CachedUpdate {
    pub current: String,
    pub latest: String,
    pub provider: String,
    pub channel: String,
    pub checked_at: Timestamp,
    // The complete installed record makes changes to selection policy invalidate results too.
    pub installed_state: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateSnapshot {
    pub schema_version: u32,
    pub updates: BTreeMap<String, CachedUpdate>,
}

impl Default for UpdateSnapshot {
    fn default() -> Self {
        Self {
            schema_version: 1,
            updates: BTreeMap::new(),
        }
    }
}

pub fn path(paths: &UpstreamPaths) -> PathBuf {
    paths.dirs.cache_dir.join("updates/available.json")
}

pub fn load(paths: &UpstreamPaths) -> Result<Option<UpdateSnapshot>> {
    let path = path(paths);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).context(
                "Could not read update cache; run upstream upgrade --check to refresh it",
            );
        }
    };

    let snapshot: UpdateSnapshot = serde_json::from_slice(&bytes)
        .context("Invalid update cache; run upstream upgrade --check to refresh it")?;

    if snapshot.schema_version != 1 {
        bail!("Unsupported update cache version; run upstream upgrade --check to refresh it");
    }

    Ok(Some(snapshot))
}

fn save(paths: &UpstreamPaths, snapshot: &UpdateSnapshot) -> Result<()> {
    write_atomic(&path(paths), &serde_json::to_vec(snapshot)?)
}

fn retain_installed(snapshot: &mut UpdateSnapshot, packages: &[Package]) {
    snapshot.updates.retain(|id, entry| {
        packages
            .iter()
            .find(|package| package.id == *id)
            .and_then(|package| serde_json::to_value(package).ok())
            .is_some_and(|state| state == entry.installed_state)
    });
}

pub fn refresh(
    paths: &UpstreamPaths,
    packages: &[Package],
    rows: &[UpdateCheckRow],
    now: Timestamp,
) -> Result<()> {
    // A fresh check can repair a damaged or obsolete disposable cache.
    let mut snapshot = load(paths).unwrap_or_default().unwrap_or_default();
    retain_installed(&mut snapshot, packages);
    for row in rows {
        match &row.status {
            UpdateCheckStatus::UpdateAvailable { current, latest } => {
                let Some(package) = packages.iter().find(|package| package.id == row.name) else {
                    continue;
                };

                snapshot.updates.insert(
                    package.id.clone(),
                    CachedUpdate {
                        current: current.clone(),
                        latest: latest.clone(),
                        provider: package.provider.to_string(),
                        channel: package.channel.to_string(),
                        checked_at: now,
                        installed_state: serde_json::to_value(package)?,
                    },
                );
            }
            UpdateCheckStatus::UpToDate { .. } | UpdateCheckStatus::NotInstalled => {
                snapshot.updates.remove(&row.name);
            }
            UpdateCheckStatus::Failed { .. } => {}
        }
    }

    save(paths, &snapshot)
}

/// Invalidate a package immediately after its persistent state changes.
/// Callers hold the package operation lock, as they do for update snapshot refreshes.
pub fn invalidate(paths: &UpstreamPaths, package_id: &str) {
    if let Err(error) = invalidate_entry(paths, package_id) {
        tracing::warn!(package = package_id, error = %format!("{error:#}"), "Could not invalidate update cache");
        eprintln!("Warning: could not invalidate update cache for '{package_id}': {error:#}");
    }
}

fn invalidate_entry(paths: &UpstreamPaths, package_id: &str) -> Result<()> {
    let Some(mut snapshot) = load(paths)? else {
        return Ok(());
    };

    if snapshot.updates.remove(package_id).is_some() {
        save(paths, &snapshot)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{cli::arguments::Cli, commands::updates::notification_message},
        models::common::enums::{Channel, Filetype, Provider},
        storage::database::PackageDatabase,
        utils::test_support,
    };
    use clap::Parser;

    struct Fixture(UpstreamPaths);

    impl Fixture {
        fn new(name: &str) -> Self {
            Self(test_support::upstream_paths(&test_support::temp_root(
                "update-cache",
                name,
            )))
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0.dirs.user_dir);
        }
    }

    fn package(id: &str) -> Package {
        Package::with_defaults(
            id.into(),
            format!("owner/{id}"),
            Filetype::Archive,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        )
    }

    fn row(id: &str, status: UpdateCheckStatus) -> UpdateCheckRow {
        UpdateCheckRow {
            name: id.into(),
            provider: Some(Provider::Github),
            channel: Some(Channel::Stable),
            status,
        }
    }

    fn available(id: &str) -> UpdateCheckRow {
        row(
            id,
            UpdateCheckStatus::UpdateAvailable {
                current: "0.0.0".into(),
                latest: "1.0.0".into(),
            },
        )
    }

    #[test]
    fn named_checks_merge_deduplicate_and_preserve_failed_results_and_age() {
        let fixture = Fixture::new("merge");
        let paths = &fixture.0;
        let packages = [package("a"), package("b"), package("c")];
        let first = Timestamp::now() - jiff::SignedDuration::from_hours(48);
        refresh(paths, &packages, &[available("a"), available("b")], first).unwrap();
        refresh(
            paths,
            &packages,
            &[
                available("c"),
                available("c"),
                row(
                    "b",
                    UpdateCheckStatus::Failed {
                        error: "offline".into(),
                    },
                ),
            ],
            Timestamp::now(),
        )
        .unwrap();
        let snapshot = load(paths).unwrap().unwrap();
        assert_eq!(snapshot.updates.len(), 3);
        assert_eq!(snapshot.updates["b"].checked_at, first);
        assert!(
            notification_message(&snapshot, Timestamp::now())
                .unwrap()
                .contains("3 pending software updates (last checked 0s ago)")
        );

        refresh(
            paths,
            &packages,
            &[row(
                "a",
                UpdateCheckStatus::UpToDate {
                    current: "1.0.0".into(),
                },
            )],
            Timestamp::now(),
        )
        .unwrap();
        assert_eq!(
            load(paths)
                .unwrap()
                .unwrap()
                .updates
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            ["b", "c"]
        );
    }

    #[test]
    fn full_checks_prune_removed_packages_and_empty_installations() {
        let fixture = Fixture::new("prune");
        let packages = [package("a"), package("b")];
        refresh(
            &fixture.0,
            &packages,
            &[available("a"), available("b")],
            Timestamp::now(),
        )
        .unwrap();
        refresh(
            &fixture.0,
            &packages[..1],
            &[available("a")],
            Timestamp::now(),
        )
        .unwrap();
        assert_eq!(load(&fixture.0).unwrap().unwrap().updates.len(), 1);
        refresh(&fixture.0, &[], &[], Timestamp::now()).unwrap();
        assert!(load(&fixture.0).unwrap().unwrap().updates.is_empty());
    }

    #[tokio::test]
    async fn package_mutations_invalidate_even_after_partial_removal_failure() {
        let fixture = Fixture::new("pin");
        let paths = &fixture.0;
        let mut database = PackageDatabase::open(&paths.metadata.packages_database_file).unwrap();
        for id in ["a", "b", "c"] {
            database.upsert_package(&package(id)).unwrap();
        }

        let packages = database.list_packages().unwrap();
        refresh(
            paths,
            &packages,
            &[available("a"), available("b"), available("c")],
            Timestamp::now(),
        )
        .unwrap();
        Cli::try_parse_from(["upstream", "package", "pin", "a"])
            .unwrap()
            .run(paths, &Default::default())
            .await
            .unwrap();
        let snapshot = load(paths).unwrap().unwrap();
        assert!(!snapshot.updates.contains_key("a"));
        assert!(snapshot.updates.contains_key("b"));
        assert!(snapshot.updates.contains_key("c"));
        // Reject the second removal after the first commits, on every platform.
        rusqlite::Connection::open(&paths.metadata.packages_database_file)
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER reject_c_removal BEFORE DELETE ON packages
                 WHEN OLD.id = 'c' BEGIN
                     SELECT RAISE(FAIL, 'injected removal failure');
                 END;",
            )
            .unwrap();
        let result = Cli::try_parse_from(["upstream", "--yes", "remove", "b", "c", "--force"])
            .unwrap()
            .run(paths, &Default::default())
            .await;

        crate::output::set_assume_yes(false);
        assert!(result.is_err());
        assert!(database.get_package("b").unwrap().is_none());
        assert!(database.get_package("c").unwrap().is_some());
        assert_eq!(
            load(paths)
                .unwrap()
                .unwrap()
                .updates
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            ["c"],
        );
    }

    fn cache_installed(paths: &UpstreamPaths, database: &PackageDatabase) {
        let packages = database.list_packages().unwrap();
        let rows = packages
            .iter()
            .map(|package| available(&package.id))
            .collect::<Vec<_>>();

        refresh(paths, &packages, &rows, Timestamp::now()).unwrap();
    }

    fn cached_ids(paths: &UpstreamPaths) -> Vec<String> {
        load(paths).unwrap().unwrap().updates.into_keys().collect()
    }

    #[test]
    fn invalidation_is_selective_and_does_not_create_a_missing_cache() {
        let fixture = Fixture::new("selective-invalidation");
        let paths = &fixture.0;
        invalidate(paths, "a");
        assert!(!paths.dirs.user_dir.exists());
        refresh(
            paths,
            &[package("a"), package("b")],
            &[available("a"), available("b")],
            Timestamp::now(),
        )
        .unwrap();
        let before = fs::read(path(paths)).unwrap();
        invalidate(paths, "not-cached");
        assert_eq!(fs::read(path(paths)).unwrap(), before);
        let untouched = serde_json::to_value(&load(paths).unwrap().unwrap().updates["b"]).unwrap();
        invalidate(paths, "a");
        assert_eq!(cached_ids(paths), ["b"]);
        assert_eq!(
            serde_json::to_value(&load(paths).unwrap().unwrap().updates["b"]).unwrap(),
            untouched
        );
    }

    #[test]
    fn settings_pins_and_shadowed_alias_renames_invalidate_the_actual_package() {
        use crate::{
            application::operations::{metadata_op::MetadataManager, package_op, rename_op},
            models::upstream::PackageExecutable,
        };
        let fixture = Fixture::new("metadata-invalidation");
        let paths = &fixture.0;
        let mut database = PackageDatabase::open(&paths.metadata.packages_database_file).unwrap();
        let mut a = package("a");
        // This command alias deliberately shadows another package's canonical ID.
        a.executables = vec![PackageExecutable {
            path: paths.install.binaries_dir.join("a"),
            name: "b".into(),
        }];
        database.upsert_package(&a).unwrap();
        database.upsert_package(&package("b")).unwrap();
        cache_installed(paths, &database);
        rename_op::rename_alias(&mut database, paths, "b", "renamed").unwrap();
        assert_eq!(cached_ids(paths), ["b"]);

        cache_installed(paths, &database);
        package_op::set("renamed", vec!["trust_mode=none".into()], paths).unwrap();
        assert_eq!(cached_ids(paths), ["b"]);
        cache_installed(paths, &database);
        package_op::unset(
            "renamed",
            vec![crate::application::cli::arguments::PackageSettingKey::TrustMode],
            paths,
        )
        .unwrap();
        assert_eq!(cached_ids(paths), ["b"]);

        cache_installed(paths, &database);
        MetadataManager::new(&mut database, paths)
            .pin_package("renamed")
            .unwrap();
        assert_eq!(cached_ids(paths), ["b"]);
        cache_installed(paths, &database);
        let before = fs::read(path(paths)).unwrap();
        MetadataManager::new(&mut database, paths)
            .pin_package("renamed")
            .unwrap();
        assert_eq!(fs::read(path(paths)).unwrap(), before); // No persistent change.
        MetadataManager::new(&mut database, paths)
            .unpin_package("renamed")
            .unwrap();
        assert_eq!(cached_ids(paths), ["b"]);
    }

    #[test]
    fn failed_mutations_keep_cached_updates_and_cache_errors_do_not_fail_commits() {
        use crate::application::operations::metadata_op::MetadataManager;
        let fixture = Fixture::new("failed-mutation");
        let paths = &fixture.0;
        let mut database = PackageDatabase::open(&paths.metadata.packages_database_file).unwrap();
        database.upsert_package(&package("a")).unwrap();
        cache_installed(paths, &database);
        let before = fs::read(path(paths)).unwrap();
        let connection =
            rusqlite::Connection::open(&paths.metadata.packages_database_file).unwrap();

        connection.execute_batch("CREATE TRIGGER reject_pin BEFORE UPDATE ON packages BEGIN SELECT RAISE(FAIL, 'injected persistence failure'); END;").unwrap();
        assert!(
            MetadataManager::new(&mut database, paths)
                .pin_package("a")
                .is_err()
        );

        assert_eq!(fs::read(path(paths)).unwrap(), before);
        assert!(!database.get_package("a").unwrap().unwrap().is_pinned);
        connection
            .execute_batch("DROP TRIGGER reject_pin;")
            .unwrap();
        fs::write(path(paths), b"invalid cache").unwrap();
        MetadataManager::new(&mut database, paths)
            .pin_package("a")
            .unwrap();
        assert!(database.get_package("a").unwrap().unwrap().is_pinned);
    }

    #[cfg(unix)]
    #[test]
    fn removal_invalidates_before_later_path_regeneration_failure() {
        use crate::services::packaging::{PackageProgressEvent, PackageRemover};
        let fixture = Fixture::new("post-removal-failure");
        let paths = &fixture.0;
        let mut database = PackageDatabase::open(&paths.metadata.packages_database_file).unwrap();
        for id in ["a", "b"] {
            database.upsert_package(&package(id)).unwrap();
        }

        cache_installed(paths, &database);
        fs::write(&paths.dirs.generated_dir, b"blocks PATH generation").unwrap();
        let result = PackageRemover::new(paths).remove(
            &mut database,
            "a",
            false,
            true,
            &mut None::<fn(&str)>,
            &mut None::<fn(&str, PackageProgressEvent)>,
        );

        assert!(result.unwrap_err().to_string().contains("PATH"));
        assert!(database.get_package("a").unwrap().is_none());
        assert_eq!(cached_ids(paths), ["b"]);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn activation_commits_invalidate_but_failed_replacement_persistence_preserves_cache() {
        use crate::{
            models::{common::Version, upstream::PackageExecutable},
            services::packaging::{
                InstallWorkspace, PackageActivator, PackageProgressEvent, PreparedInstall,
            },
        };
        fn prepare(paths: &UpstreamPaths, version: u32) -> (Package, InstallWorkspace) {
            let workspace = InstallWorkspace::new(paths, "a").unwrap();
            let binary = workspace.binaries_dir.join("a");
            fs::write(&binary, format!("version {version}")).unwrap();
            let mut package = package("a");
            package.filetype = Filetype::Binary;
            package.version = Version::new(version, 0, 0, false);
            package.install_path = Some(binary.clone());
            package.executables = vec![PackageExecutable {
                path: binary,
                name: "a".into(),
            }];
            (package, workspace)
        }
        let fixture = Fixture::new("activation-invalidation");
        let paths = &fixture.0;
        fs::create_dir_all(&paths.state.symlinks_dir).unwrap();
        fs::create_dir_all(&paths.dirs.config_dir).unwrap();
        let mut database = PackageDatabase::open(&paths.metadata.packages_database_file).unwrap();
        database.upsert_package(&package("b")).unwrap();
        let activator = PackageActivator::new(paths);
        let (candidate, workspace) = prepare(paths, 1);
        refresh(
            paths,
            &[candidate.clone(), package("b")],
            &[available("a"), available("b")],
            Timestamp::now(),
        )
        .unwrap();
        let installed = activator
            .install_new(
                &mut database,
                PreparedInstall::new(candidate, workspace),
                Some(false),
                &mut None::<fn(&str)>,
                &mut None::<fn(PackageProgressEvent)>,
            )
            .await
            .unwrap();

        assert_eq!(cached_ids(paths), ["b"]);

        cache_installed(paths, &database);
        let (candidate, workspace) = prepare(paths, 2);
        let replacement = activator
            .replace(
                &installed,
                PreparedInstall::new(candidate, workspace),
                false,
                &mut None::<fn(&str)>,
                &mut None::<fn(PackageProgressEvent)>,
            )
            .await
            .unwrap();

        assert_eq!(cached_ids(paths), ["a", "b"]); // Activation has not committed metadata yet.
        let installed = activator.persist(&mut database, replacement).unwrap();
        assert_eq!(cached_ids(paths), ["b"]);

        cache_installed(paths, &database);
        let before = fs::read(path(paths)).unwrap();
        let (candidate, workspace) = prepare(paths, 3);
        let replacement = activator
            .replace(
                &installed,
                PreparedInstall::new(candidate, workspace),
                false,
                &mut None::<fn(&str)>,
                &mut None::<fn(PackageProgressEvent)>,
            )
            .await
            .unwrap();

        rusqlite::Connection::open(&paths.metadata.packages_database_file).unwrap().execute_batch(
            "CREATE TRIGGER reject_replacement BEFORE UPDATE ON packages BEGIN SELECT RAISE(FAIL, 'injected persistence failure'); END;",
        ).unwrap();
        assert!(activator.persist(&mut database, replacement).is_err());
        assert_eq!(
            database.get_package("a").unwrap().unwrap().version,
            Version::new(2, 0, 0, false)
        );

        assert_eq!(fs::read(path(paths)).unwrap(), before);
        assert_eq!(
            fs::read(installed.install_path.unwrap()).unwrap(),
            b"version 2"
        );
    }

    #[test]
    fn changed_versions_and_branch_commits_are_invalidated() {
        let fixture = Fixture::new("changes");
        let mut packages = [package("release"), package("branch")];
        packages[1].build_branch = Some("main".into());
        packages[1].build_commit = Some("old".into());
        refresh(
            &fixture.0,
            &packages,
            &[available("release"), available("branch")],
            Timestamp::now(),
        )
        .unwrap();
        packages[0].version = crate::models::common::Version::new(1, 0, 0, false);
        packages[1].build_commit = Some("new".into());
        refresh(&fixture.0, &packages, &[], Timestamp::now()).unwrap();
        assert!(load(&fixture.0).unwrap().unwrap().updates.is_empty());
    }

    #[test]
    fn missing_corrupt_and_unsupported_snapshots_and_repair() {
        let fixture = Fixture::new("invalid");
        assert!(load(&fixture.0).unwrap().is_none());
        assert!(!fixture.0.dirs.user_dir.exists());
        fs::create_dir_all(path(&fixture.0).parent().unwrap()).unwrap();
        for bytes in [
            b"invalid".as_slice(),
            br#"{"schema_version":2,"updates":{}}"#,
        ] {
            fs::write(path(&fixture.0), bytes).unwrap();
            assert!(load(&fixture.0).is_err());
            refresh(&fixture.0, &[], &[], Timestamp::now()).unwrap();
            assert!(load(&fixture.0).unwrap().unwrap().updates.is_empty());
        }
    }

    #[test]
    fn notification_handles_empty_and_singular_updates() {
        let fixture = Fixture::new("message");
        assert!(notification_message(&UpdateSnapshot::default(), Timestamp::now()).is_none());
        let now = Timestamp::now();
        refresh(&fixture.0, &[package("a")], &[available("a")], now).unwrap();
        let snapshot = load(&fixture.0).unwrap().unwrap();
        assert_eq!(
            notification_message(&snapshot, now).unwrap(),
            "upstream: 1 pending software update (last checked 0s ago).\nRun 'upstream updates' to view them.\n"
        );
    }
}
