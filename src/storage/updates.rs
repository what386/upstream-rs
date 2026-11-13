//! Disposable snapshots of known available updates. Readers never need a database.
use std::{collections::BTreeMap, fs, path::PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    application::operations::upgrade_op::{UpdateCheckRow, UpdateCheckStatus},
    models::upstream::Package,
    storage::database::PackageDatabase,
    utils::{filesystem::atomic_ops::write_atomic, static_paths::UpstreamPaths},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct CachedUpdate {
    pub current: String,
    pub latest: String,
    pub provider: String,
    pub channel: String,
    pub checked_at: DateTime<Utc>,
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
    now: DateTime<Utc>,
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

/// Called while the command lock is held, including after partial failures.
pub fn reconcile(paths: &UpstreamPaths) -> Result<()> {
    let Some(mut snapshot) = load(paths)? else {
        return Ok(());
    };

    if snapshot.updates.is_empty() {
        return Ok(());
    }

    let packages = if paths.metadata.packages_database_file.is_file() {
        PackageDatabase::open(&paths.metadata.packages_database_file)?.list_packages()?
    } else {
        Vec::new()
    };

    let previous = snapshot.updates.len();
    retain_installed(&mut snapshot, &packages);
    if previous != snapshot.updates.len() {
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
        let first = Utc::now() - chrono::Duration::days(2);
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
            Utc::now(),
        )
        .unwrap();
        let snapshot = load(paths).unwrap().unwrap();
        assert_eq!(snapshot.updates.len(), 3);
        assert_eq!(snapshot.updates["b"].checked_at, first);
        assert!(
            notification_message(&snapshot, Utc::now())
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
            Utc::now(),
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
            Utc::now(),
        )
        .unwrap();
        refresh(&fixture.0, &packages[..1], &[available("a")], Utc::now()).unwrap();
        assert_eq!(load(&fixture.0).unwrap().unwrap().updates.len(), 1);
        refresh(&fixture.0, &[], &[], Utc::now()).unwrap();
        assert!(load(&fixture.0).unwrap().unwrap().updates.is_empty());
    }

    #[tokio::test]
    async fn dispatch_reconciles_pin_and_partial_removal_failure() {
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
            Utc::now(),
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
            Utc::now(),
        )
        .unwrap();
        packages[0].version = crate::models::common::Version::new(1, 0, 0, false);
        packages[1].build_commit = Some("new".into());
        refresh(&fixture.0, &packages, &[], Utc::now()).unwrap();
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
            refresh(&fixture.0, &[], &[], Utc::now()).unwrap();
            assert!(load(&fixture.0).unwrap().unwrap().updates.is_empty());
        }
    }

    #[test]
    fn notification_handles_empty_and_singular_updates() {
        let fixture = Fixture::new("message");
        assert!(notification_message(&UpdateSnapshot::default(), Utc::now()).is_none());
        let now = Utc::now();
        refresh(&fixture.0, &[package("a")], &[available("a")], now).unwrap();
        let snapshot = load(&fixture.0).unwrap().unwrap();
        assert_eq!(
            notification_message(&snapshot, now).unwrap(),
            "upstream: 1 pending software update (last checked 0s ago).\nRun 'upstream updates' to view them.\n"
        );
    }
}
