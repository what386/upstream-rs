use anyhow::{Context, Result, anyhow, bail};

use crate::{
    services::{
        integration::{CompletionManager, DesktopManager, SymlinkManager},
        packaging::RollbackManager,
    },
    storage::database::PackageDatabase,
    utils::static_paths::UpstreamPaths,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameOutcome {
    Renamed,
    Unchanged,
}

#[derive(Default)]
struct RenameIntegrationSteps {
    runtime_link: bool,
    completions: bool,
    desktop_entry: bool,
    rollback: bool,
}

pub fn rename_package(
    package_database: &mut PackageDatabase,
    paths: &UpstreamPaths,
    old_name: &str,
    new_name: &str,
) -> Result<RenameOutcome> {
    package_database
        .get_package(old_name)?
        .ok_or_else(|| anyhow!("Package '{}' not found", old_name))?;

    if old_name == new_name {
        return Ok(RenameOutcome::Unchanged);
    }

    if package_database.get_package(new_name)?.is_some() {
        bail!("Package '{}' already exists", new_name);
    }

    let mut rollback_manager = RollbackManager::new(paths)?;
    let symlink_manager = SymlinkManager::new(&paths.state.symlinks_dir);
    let completion_manager = CompletionManager::new(paths);
    let mut steps = RenameIntegrationSteps::default();

    let rename_result = (|| {
        steps.runtime_link = symlink_manager.rename_link(old_name, new_name)?;
        steps.completions = completion_manager.rename_for_package(old_name, new_name)?;
        steps.desktop_entry = DesktopManager::rename_entry(paths, old_name, new_name)?;
        steps.rollback = rollback_manager.rename_package(old_name, new_name)?;
        package_database.rename_package(old_name, new_name)
    })();

    if let Err(error) = rename_result {
        let rollback_result =
            rollback_integrations(paths, &mut rollback_manager, old_name, new_name, steps);

        return match rollback_result {
            Ok(()) => {
                Err(error).context("Package rename failed; integration changes were reverted")
            }
            Err(rollback_error) => Err(anyhow!(
                "Package rename failed: {}. Integration rollback also failed: {}",
                error,
                rollback_error
            )),
        };
    }

    Ok(RenameOutcome::Renamed)
}

fn rollback_integrations(
    paths: &UpstreamPaths,
    rollback_manager: &mut RollbackManager<'_>,
    old_name: &str,
    new_name: &str,
    steps: RenameIntegrationSteps,
) -> Result<()> {
    let mut errors = Vec::new();
    if steps.rollback
        && let Err(error) = rollback_manager.rename_package(new_name, old_name)
    {
        errors.push(format!("rollback data: {error}"));
    }

    if steps.desktop_entry
        && let Err(error) = DesktopManager::rename_entry(paths, new_name, old_name)
    {
        errors.push(format!("desktop entry: {error}"));
    }

    if steps.completions
        && let Err(error) = CompletionManager::new(paths).rename_for_package(new_name, old_name)
    {
        errors.push(format!("completions: {error}"));
    }

    if steps.runtime_link
        && let Err(error) =
            SymlinkManager::new(&paths.state.symlinks_dir).rename_link(new_name, old_name)
    {
        errors.push(format!("runtime link: {error}"));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        bail!("{}", errors.join("; "))
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{RenameOutcome, rename_package};
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
        package.exec_path = Some(install_path.clone());
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
            rename_package(&mut database, &paths, "old", "new").expect("rename package"),
            RenameOutcome::Renamed
        );

        assert!(database.get_package("old").expect("load old").is_none());
        assert!(database.get_package("new").expect("load new").is_some());
        assert!(paths.state.symlinks_dir.join("new").exists());
        assert!(!paths.state.symlinks_dir.join("old").exists());
        assert!(paths.integration.bash_completions_dir.join("new").exists());
        assert!(
            paths
                .integration
                .xdg_applications_dir
                .join("new.desktop")
                .exists()
        );

        let rollback_manager = RollbackManager::new(&paths).expect("reload rollback manager");
        assert!(rollback_manager.rollback_record("old").is_none());
        assert_eq!(
            rollback_manager
                .rollback_record("new")
                .expect("renamed rollback")
                .package_snapshot
                .name,
            "new"
        );

        assert!(paths.state.rollback_dir.join("new").exists());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn rename_collision_reverts_integrations_and_preserves_database_name() {
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
        fs::write(
            paths.integration.xdg_applications_dir.join("new.desktop"),
            b"unrelated desktop",
        )
        .expect("write colliding desktop");

        let mut database =
            PackageDatabase::open(&paths.metadata.packages_database_file).expect("open database");

        let error = rename_package(&mut database, &paths, "old", "new")
            .expect_err("desktop collision should fail rename");

        assert!(
            error
                .to_string()
                .contains("integration changes were reverted")
        );

        assert!(database.get_package("old").expect("load old").is_some());
        assert!(database.get_package("new").expect("load new").is_none());
        assert!(paths.state.symlinks_dir.join("old").exists());
        assert!(!paths.state.symlinks_dir.join("new").exists());
        assert!(paths.integration.bash_completions_dir.join("old").exists());
        assert_eq!(
            fs::read(paths.integration.xdg_applications_dir.join("new.desktop"))
                .expect("read colliding desktop"),
            b"unrelated desktop"
        );

        fs::remove_dir_all(root).expect("cleanup");
    }
}
