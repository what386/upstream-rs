use anyhow::{Context, Result, anyhow};

use crate::{
    models::upstream::Package,
    services::{integration::ShellManager, packaging::RollbackManager},
    storage::{database::PackageDatabase, rollback::RollbackStorage},
    utils::static_paths::UpstreamPaths,
};

pub struct PackageReplacer<'a> {
    paths: &'a UpstreamPaths,
}

impl<'a> PackageReplacer<'a> {
    pub fn new(paths: &'a UpstreamPaths) -> Self {
        Self { paths }
    }

    pub fn commit(
        &self,
        package_database: &mut PackageDatabase,
        replacement: &Package,
    ) -> Result<()> {
        if let Err(persistence_error) = package_database.upsert_package(replacement) {
            return self.rollback_failed_database_commit(
                package_database,
                replacement,
                persistence_error,
            );
        }

        ShellManager::new(&self.paths.config.paths_file)
            .regenerate_paths(package_database, self.paths)
            .context(format!(
                "Replacement for '{}' was persisted, but shell PATH files could not be refreshed",
                replacement.name
            ))
    }

    fn rollback_failed_database_commit(
        &self,
        package_database: &mut PackageDatabase,
        replacement: &Package,
        persistence_error: anyhow::Error,
    ) -> Result<()> {
        let rollback_result = (|| {
            let rollback_file = RollbackManager::rollback_file_path(self.paths);
            let mut rollback_storage = RollbackStorage::new(&rollback_file)?;
            RollbackManager::new(self.paths, package_database, &mut rollback_storage)
                .restore_replaced_package(&replacement.name, replacement, &mut None::<fn(&str)>)
        })();

        match rollback_result {
            Ok(()) => Err(persistence_error).context(format!(
                "Failed to persist replacement for '{}' (previous version restored)",
                replacement.name
            )),
            Err(rollback_error) => Err(anyhow!(
                "Failed to persist replacement for '{}': {}. Rollback also failed: {}",
                replacement.name,
                persistence_error,
                rollback_error
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PackageReplacer;
    use crate::{
        models::{
            common::{
                Version,
                enums::{Channel, Filetype, Provider, TrustMode},
            },
            upstream::Package,
        },
        services::{integration::SymlinkManager, packaging::RollbackManager},
        storage::{
            database::{PackageDatabase, PackageSettings},
            rollback::{RollbackSource, RollbackStorage},
        },
        utils::test_support,
    };
    use rusqlite::Connection;
    use std::fs;

    #[test]
    fn failed_database_commit_restores_previous_files_and_metadata() {
        let root = test_support::temp_root("upstream-package-replacement-test", "rollback");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.install.binaries_dir).expect("create binaries");
        fs::create_dir_all(&paths.install.tmp_dir).expect("create tmp");
        fs::create_dir_all(&paths.state.symlinks_dir).expect("create symlinks");
        fs::create_dir_all(&paths.dirs.metadata_dir).expect("create metadata");

        let install_path = paths.install.binaries_dir.join("tool");
        let backup_path = paths.install.tmp_dir.join("tool.old");
        fs::write(&backup_path, b"old binary").expect("write backup");

        let mut previous = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        previous.version = Version::new(1, 0, 0, false);
        previous.install_path = Some(install_path.clone());
        previous.exec_path = Some(install_path.clone());

        let mut database =
            PackageDatabase::open(&paths.config.packages_database_file).expect("open database");
        let mut settings = PackageSettings::new("tool");
        settings.trust_mode = Some(TrustMode::Signature);
        database
            .upsert_package_with_settings(&previous, &settings)
            .expect("store previous package and settings");
        let rollback_file = RollbackManager::rollback_file_path(&paths);
        let mut rollback_storage =
            RollbackStorage::new(&rollback_file).expect("open rollback storage");
        RollbackManager::capture_backup_path(
            &paths,
            &mut rollback_storage,
            &previous,
            &backup_path,
            RollbackSource::Upgrade,
            &mut None::<fn(&str)>,
        )
        .expect("capture rollback");

        fs::write(&install_path, b"new binary").expect("write replacement");
        SymlinkManager::new(&paths.state.symlinks_dir)
            .add_link(&install_path, "tool")
            .expect("create replacement link");
        let mut replacement = previous.clone();
        replacement.version = Version::new(2, 0, 0, false);

        let database_path =
            PackageDatabase::database_path_for(&paths.config.packages_database_file);
        Connection::open(&database_path)
            .expect("open trigger connection")
            .execute_batch(
                "
                CREATE TRIGGER reject_v2
                BEFORE UPDATE ON packages
                WHEN NEW.version_major = 2
                BEGIN
                    SELECT RAISE(ABORT, 'reject test replacement');
                END;
                ",
            )
            .expect("create trigger");

        let error = PackageReplacer::new(&paths)
            .commit(&mut database, &replacement)
            .expect_err("replacement persistence should fail");

        assert!(error.to_string().contains("previous version restored"));
        assert_eq!(
            fs::read(&install_path).expect("read restored binary"),
            b"old binary"
        );
        assert_eq!(
            database
                .get_package("tool")
                .expect("load package")
                .expect("stored package")
                .version,
            Version::new(1, 0, 0, false)
        );
        assert_eq!(
            database
                .get_package_settings("tool")
                .expect("load settings")
                .expect("stored settings")
                .trust_mode,
            Some(TrustMode::Signature)
        );
        assert!(
            RollbackStorage::new(&rollback_file)
                .expect("reload rollback storage")
                .get_record("tool")
                .is_none()
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn shell_refresh_failure_keeps_persisted_replacement_consistent() {
        let root = test_support::temp_root("upstream-package-replacement-test", "shell-failure");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.install.binaries_dir).expect("create binaries");
        fs::create_dir_all(&paths.dirs.data_dir).expect("create data");
        fs::write(&paths.dirs.generated_dir, b"blocks generated directory")
            .expect("create blocking file");

        let install_path = paths.install.binaries_dir.join("tool");
        fs::write(&install_path, b"new binary").expect("write replacement");
        let mut previous = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        previous.version = Version::new(1, 0, 0, false);
        previous.install_path = Some(install_path.clone());
        previous.exec_path = Some(install_path.clone());
        let mut replacement = previous.clone();
        replacement.version = Version::new(2, 0, 0, false);

        let mut database =
            PackageDatabase::open(&paths.config.packages_database_file).expect("open database");
        database
            .upsert_package(&previous)
            .expect("store previous package");

        let error = PackageReplacer::new(&paths)
            .commit(&mut database, &replacement)
            .expect_err("shell refresh should fail");

        assert!(error.to_string().contains("was persisted"));
        assert_eq!(
            database
                .get_package("tool")
                .expect("load package")
                .expect("stored package")
                .version,
            Version::new(2, 0, 0, false)
        );
        assert_eq!(
            fs::read(&install_path).expect("read replacement"),
            b"new binary"
        );

        fs::remove_dir_all(root).expect("cleanup");
    }
}
