use anyhow::{Context, Result, bail};

use crate::{
    services::integration::SymlinkManager, storage::database::PackageDatabase,
    utils::static_paths::UpstreamPaths,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameOutcome {
    Renamed,
    Unchanged,
}

pub fn rename_alias(
    package_database: &mut PackageDatabase,
    paths: &UpstreamPaths,
    old_name: &str,
    new_name: &str,
) -> Result<RenameOutcome> {
    if !package_database.executable_alias_exists(old_name)? {
        bail!("Executable alias '{}' not found", old_name);
    }

    if old_name == new_name {
        return Ok(RenameOutcome::Unchanged);
    }

    if package_database.executable_alias_exists(new_name)? {
        bail!("Executable alias '{}' already exists", new_name);
    }

    let symlink_manager = SymlinkManager::new(&paths.state.symlinks_dir);
    let renamed_link = symlink_manager.rename_link(old_name, new_name)?;
    if let Err(error) = package_database.rename_alias(old_name, new_name) {
        if renamed_link {
            let _ = symlink_manager.rename_link(new_name, old_name);
        }
        return Err(error).context("Executable alias rename failed; runtime link was reverted");
    }

    Ok(RenameOutcome::Renamed)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{RenameOutcome, rename_alias};
    use crate::{
        models::{
            common::enums::{Channel, Filetype, Provider},
            upstream::Package,
        },
        services::{integration::SymlinkManager, packaging::RollbackManager},
        storage::{database::PackageDatabase, rollback::RollbackSource},
        utils::test_support,
    };
    use std::fs;

    fn seed_package(paths: &crate::utils::static_paths::UpstreamPaths, name: &str) -> Package {
        fs::create_dir_all(&paths.install.binaries_dir).expect("create binaries");
        fs::create_dir_all(&paths.state.symlinks_dir).expect("create symlinks");
        let install_path = paths.install.binaries_dir.join("tool-bin");
        fs::write(&install_path, b"tool").expect("write installed binary");
        let mut package = Package::with_defaults(
            name.to_string(),
            "owner/tool".to_string(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );

        package.install_path = Some(install_path.clone());
        package.executables = vec![crate::models::upstream::PackageExecutable {
            path: install_path.clone(),
            name: name.to_string(),
        }];
        let mut database =
            PackageDatabase::open(&paths.metadata.packages_database_file).expect("open database");

        database.upsert_package(&package).expect("store package");
        SymlinkManager::new(&paths.state.symlinks_dir)
            .add_link(&install_path, name)
            .expect("add runtime link");
        package
    }

    #[test]
    fn rename_moves_package_owned_integrations_and_rollback_data() {
        let root = test_support::temp_root("upstream-rename-op-test", "rename");
        let paths = test_support::upstream_paths(&root);
        let package = seed_package(&paths, "old");
        fs::create_dir_all(&paths.integration.bash_completions_dir).expect("create completion dir");
        fs::create_dir_all(&paths.integration.xdg_applications_dir)
            .expect("create applications dir");
        fs::write(
            paths.integration.bash_completions_dir.join("old"),
            b"completion",
        )
        .expect("write completion");
        fs::write(
            paths.integration.xdg_applications_dir.join("old.desktop"),
            b"desktop",
        )
        .expect("write desktop");

        fs::create_dir_all(&paths.install.tmp_dir).expect("create tmp");
        fs::write(paths.install.tmp_dir.join("tool.old"), b"rollback")
            .expect("write rollback source");
        let mut rollback_manager = RollbackManager::new(&paths).expect("open rollback manager");
        rollback_manager
            .capture_backup_path(
                &package,
                &paths.install.tmp_dir.join("tool.old"),
                RollbackSource::Upgrade,
                &mut None::<fn(&str)>,
            )
            .expect("capture rollback");

        let mut database =
            PackageDatabase::open(&paths.metadata.packages_database_file).expect("open database");

        assert_eq!(
            rename_alias(&mut database, &paths, "old", "new").expect("rename alias"),
            RenameOutcome::Renamed
        );

        assert!(database.get_package("old").expect("load old").is_some());
        assert!(
            database
                .executable_alias_exists("new")
                .expect("load new alias")
        );
        assert!(paths.state.symlinks_dir.join("new").exists());
        assert!(!paths.state.symlinks_dir.join("old").exists());
        assert!(paths.integration.bash_completions_dir.join("old").exists());
        assert!(
            paths
                .integration
                .xdg_applications_dir
                .join("old.desktop")
                .exists()
        );

        let rollback_manager = RollbackManager::new(&paths).expect("reload rollback manager");
        assert!(rollback_manager.rollback_record("old").is_some());
        assert!(rollback_manager.rollback_record("new").is_none());
        assert_eq!(
            rollback_manager
                .rollback_record("old")
                .expect("existing rollback")
                .package_snapshot
                .id,
            "old"
        );

        assert!(paths.state.rollback_dir.join("old").exists());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn rename_only_changes_the_executable_alias() {
        let root = test_support::temp_root("upstream-rename-op-test", "rename-collision");
        let paths = test_support::upstream_paths(&root);
        seed_package(&paths, "old");
        fs::create_dir_all(&paths.integration.bash_completions_dir).expect("create completion dir");
        fs::create_dir_all(&paths.integration.xdg_applications_dir)
            .expect("create applications dir");
        fs::write(
            paths.integration.bash_completions_dir.join("old"),
            b"old completion",
        )
        .expect("write old completion");
        let mut database =
            PackageDatabase::open(&paths.metadata.packages_database_file).expect("open database");

        rename_alias(&mut database, &paths, "old", "new").expect("rename alias");

        assert!(database.get_package("old").expect("load old").is_some());
        assert!(
            database
                .executable_alias_exists("new")
                .expect("load new alias")
        );
        assert!(!paths.state.symlinks_dir.join("old").exists());
        assert!(paths.state.symlinks_dir.join("new").exists());
        assert!(paths.integration.bash_completions_dir.join("old").exists());

        fs::remove_dir_all(root).expect("cleanup");
    }
}
