use super::UpstreamPaths;

#[test]
fn upstream_paths_are_composed_from_base_directories() {
    let paths = UpstreamPaths::new();

    assert_eq!(
        paths.config.config_file,
        paths.dirs.config_dir.join("config.toml")
    );
    assert_eq!(
        paths.config.packages_file,
        paths.dirs.metadata_dir.join("packages.json")
    );
    assert_eq!(
        paths.install.binaries_dir,
        paths.dirs.data_dir.join("binaries")
    );
    assert_eq!(
        paths.integration.symlinks_dir,
        paths.dirs.data_dir.join("symlinks")
    );
}
