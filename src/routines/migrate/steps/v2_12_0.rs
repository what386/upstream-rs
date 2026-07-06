use std::fs;

use anyhow::{Context, Result};

use crate::models::upstream::AuthenticationConfig;
use crate::routines::migrate::MigrationReport;
use crate::storage::system::auth::AuthStorage;
use crate::utils::filesystem::atomic_ops::write_atomic;
use crate::utils::static_paths::UpstreamPaths;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub(super) fn run(paths: &UpstreamPaths, _report: &mut MigrationReport) -> Result<()> {
    migrate_legacy_auth_config(paths)
}

fn migrate_legacy_auth_config(paths: &UpstreamPaths) -> Result<()> {
    if !paths.config.config_file.exists() || paths.config.auth_file.exists() {
        return Ok(());
    }

    let raw_config = fs::read_to_string(&paths.config.config_file).with_context(|| {
        format!(
            "Failed to read config '{}'",
            paths.config.config_file.display()
        )
    })?;
    if raw_config.trim().is_empty() {
        return Ok(());
    }

    let mut config_value: toml::Value = toml::from_str(&raw_config).with_context(|| {
        format!(
            "Failed to parse config '{}'",
            paths.config.config_file.display()
        )
    })?;
    let Some(config_table) = config_value.as_table_mut() else {
        return Ok(());
    };

    let mut auth = AuthenticationConfig::default();
    let mut migrated = false;

    for provider in ["github", "gitlab", "gitea"] {
        let mut remove_provider = false;
        let Some(provider_value) = config_table.get_mut(provider) else {
            continue;
        };
        {
            let Some(provider_table) = provider_value.as_table_mut() else {
                continue;
            };

            if let Some(token_value) = provider_table.remove("api_token") {
                let token: Option<String> = token_value
                    .try_into()
                    .context("Failed to parse legacy provider auth token")?;
                match provider {
                    "github" => auth.github.api_token = token,
                    "gitlab" => auth.gitlab.api_token = token,
                    "gitea" => auth.gitea.api_token = token,
                    _ => unreachable!(),
                }
                migrated = true;
            }

            if provider_table.is_empty() {
                remove_provider = true;
            }
        }

        if remove_provider {
            config_table.remove(provider);
        }
    }

    if !migrated {
        return Ok(());
    }

    let mut auth_storage = AuthStorage::new(&paths.config.auth_file)?;
    auth_storage.replace_auth(auth)?;

    let rendered = toml::to_string_pretty(&config_value).context("Failed to serialize config")?;
    write_atomic(&paths.config.config_file, rendered.as_bytes()).with_context(|| {
        format!(
            "Failed to save config to '{}'",
            paths.config.config_file.display()
        )
    })?;

    #[cfg(unix)]
    fs::set_permissions(&paths.config.config_file, fs::Permissions::from_mode(0o600))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::run;
    use crate::routines::migrate::MigrationReport;
    use crate::storage::system::auth::AuthStorage;
    use crate::utils::test_support;
    use std::path::{Path, PathBuf};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        test_support::temp_root("upstream-migrate-v2-12-test", name)
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    #[test]
    fn migrate_splits_legacy_auth_keys_into_auth_toml() {
        let root = temp_root("legacy-auth");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.dirs.config_dir).expect("create config dir");

        fs::write(
            &paths.config.config_file,
            "[github]\napi_token = \"ghp_abc\"\n\n[download]\nlow_threads = 3\n",
        )
        .expect("write legacy config");
        let mut report = MigrationReport::default();

        run(&paths, &mut report).expect("run migration");

        let migrated_config = fs::read_to_string(&paths.config.config_file).expect("read config");
        assert!(!migrated_config.contains("api_token"));
        assert!(migrated_config.contains("[download]"));

        let auth = AuthStorage::new(&paths.config.auth_file).expect("load auth");
        assert_eq!(auth.get_auth().github.api_token.as_deref(), Some("ghp_abc"));

        cleanup(&root).expect("cleanup");
    }
}
