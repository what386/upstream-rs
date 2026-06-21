use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::utils::static_paths::{
    AppDirs, ConfigPaths, InstallPaths, IntegrationPaths, UpstreamPaths,
};

pub fn temp_root(prefix: &str, name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{prefix}-{name}-{nanos}"))
}

pub fn upstream_paths(root: &Path) -> UpstreamPaths {
    let dirs = AppDirs {
        user_dir: root.to_path_buf(),
        config_dir: root.join("config"),
        data_dir: root.join("data"),
        packages_dir: root.join("data/packages"),
        cache_dir: root.join("data/cache"),
        metadata_dir: root.join("data/metadata"),
    };

    UpstreamPaths {
        config: ConfigPaths {
            config_file: dirs.config_dir.join("config.toml"),
            packages_file: dirs.metadata_dir.join("packages.json"),
            trust_file: dirs.metadata_dir.join("trust.json"),
            paths_file: dirs.metadata_dir.join("paths.sh"),
            paths_nu_file: dirs.metadata_dir.join("paths.nu"),
        },
        install: InstallPaths {
            appimages_dir: dirs.packages_dir.join("appimages"),
            binaries_dir: dirs.packages_dir.join("binaries"),
            archives_dir: dirs.packages_dir.join("archives"),
            rollback_dir: dirs.data_dir.join("rollback"),
            tmp_dir: dirs.data_dir.join("tmp"),
        },
        integration: IntegrationPaths {
            symlinks_dir: dirs.data_dir.join("symlinks"),
            xdg_applications_dir: dirs.user_dir.join(".local/share/applications"),
            icons_dir: dirs.data_dir.join("icons"),
            bash_completions_dir: dirs
                .user_dir
                .join(".local/share/bash-completion/completions"),
            fish_completions_dir: dirs.user_dir.join(".config/fish/completions"),
            zsh_completions_dir: dirs.user_dir.join(".local/share/zsh/site-functions"),
        },
        dirs,
    }
}
