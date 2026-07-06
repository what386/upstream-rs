mod v2_0_0;
#[path = "v2.11.0.rs"]
mod v2_11_0;
mod v2_3_0;
mod v2_6_0;

use anyhow::Result;

use crate::{routines::migrate::MigrationReport, utils::static_paths::UpstreamPaths};

pub(super) fn run(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
    v2_0_0::run(paths, report)?;
    v2_3_0::run(paths, report)?;
    v2_6_0::run(paths, report)?;
    v2_11_0::run(paths, report)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::run;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::routines::migrate::MigrationReport;
    use crate::storage::database::PackageDatabase;
    use crate::utils::test_support;
    use crate::utils::static_paths::UpstreamPaths;
    use std::path::{Path, PathBuf};
    use std::{fs, io};

    fn state_symlink_path(paths: &UpstreamPaths, name: &str) -> PathBuf {
        let link = paths.state.symlinks_dir.join(name);
        #[cfg(windows)]
        {
            if link.extension().is_none() {
                return link.with_extension("exe");
            }
        }

        link
    }

    fn temp_root(name: &str) -> PathBuf {
        test_support::temp_root("upstream-migrate-steps-test", name)
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    #[test]
    fn run_moves_legacy_symlinks_before_final_refresh() {
        let root = temp_root("legacy-symlinks-first-run");
        let paths = test_support::upstream_paths(&root);
        let binary = paths.install.binaries_dir.join("tool");
        fs::create_dir_all(binary.parent().expect("binary parent")).expect("create binary parent");
        fs::write(&binary, b"tool").expect("write binary");

        let old_symlinks_dir = paths.dirs.data_dir.join("symlinks");
        fs::create_dir_all(&old_symlinks_dir).expect("create old symlinks");
        fs::write(old_symlinks_dir.join("tool"), b"stale link placeholder")
            .expect("write old symlink placeholder");

        let mut package = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        package.install_path = Some(binary.clone());
        package.exec_path = Some(binary.clone());
        let mut database = PackageDatabase::open(&paths.config.packages_database_file)
            .expect("create package database");
        database.upsert_package(&package).expect("seed package");

        let mut report = MigrationReport::default();
        run(&paths, &mut report).expect("run migration steps");

        assert!(!old_symlinks_dir.exists());
        assert!(state_symlink_path(&paths, "tool").exists());
        #[cfg(unix)]
        assert_eq!(
            fs::read_link(state_symlink_path(&paths, "tool")).expect("read refreshed symlink"),
            binary
        );

        cleanup(&root).expect("cleanup");
    }
}
