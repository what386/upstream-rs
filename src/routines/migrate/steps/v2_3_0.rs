use std::fs;

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use crate::routines::migrate::MigrationReport;
use crate::routines::migrate::step::Step;
use crate::services::trust::{CosignPublicKey, MinisignPublicKey};
use crate::storage::system::trust::TrustStorage;
use crate::utils::static_paths::UpstreamPaths;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct LegacyTrustConfig {
    minisign_public_keys: Vec<MinisignPublicKey>,
    cosign_public_keys: Vec<CosignPublicKey>,
}

pub struct V2_3_0;

pub(super) fn run(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
    V2_3_0::run(paths, report)
}

impl Step for V2_3_0 {
    fn check(paths: &UpstreamPaths) -> Result<bool> {
        legacy_trust_needs_migration(paths)
    }

    fn apply(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
        let mut trust_storage = TrustStorage::new(&paths.config.trust_file)?;
        let Some(legacy_trust) = legacy_trust_config(paths)? else {
            trust_storage.ensure_exists()?;
            return Ok(());
        };

        let summary = trust_storage.merge_trusted_keys(
            &legacy_trust.minisign_public_keys,
            &legacy_trust.cosign_public_keys,
        )?;
        report.migrated_trusted_keys += summary.minisign.imported + summary.cosign.imported;
        report.deduped_trusted_keys += summary.minisign.deduped + summary.cosign.deduped;

        Ok(())
    }
}

fn legacy_trust_config(paths: &UpstreamPaths) -> Result<Option<LegacyTrustConfig>> {
    if !paths.config.config_file.exists() {
        return Ok(None);
    }

    let raw_config = fs::read_to_string(&paths.config.config_file).with_context(|| {
        format!(
            "Failed to read config '{}'",
            paths.config.config_file.display()
        )
    })?;
    if raw_config.trim().is_empty() {
        return Ok(None);
    }

    let config_value: toml::Value = toml::from_str(&raw_config).with_context(|| {
        format!(
            "Failed to parse config '{}'",
            paths.config.config_file.display()
        )
    })?;
    let config_table = config_value.as_table().ok_or_else(|| {
        anyhow!(
            "Config '{}' must be a TOML table",
            paths.config.config_file.display()
        )
    })?;

    let Some(trust_value) = config_table.get("trust") else {
        return Ok(None);
    };

    trust_value
        .clone()
        .try_into()
        .map(Some)
        .context("Failed to parse legacy config trust keys")
}

fn legacy_trust_needs_migration(paths: &UpstreamPaths) -> Result<bool> {
    let Some(legacy_trust) = legacy_trust_config(paths)? else {
        return Ok(false);
    };
    let trust_storage = TrustStorage::new(&paths.config.trust_file)?;
    let trusted_keys = trust_storage.trusted_signature_keys();

    let missing_minisign = legacy_trust.minisign_public_keys.iter().any(|legacy_key| {
        let normalized = legacy_key.key.trim();
        !normalized.is_empty()
            && !trusted_keys
                .minisign_public_keys
                .iter()
                .any(|existing| existing.key.trim().eq_ignore_ascii_case(normalized))
    });
    let missing_cosign = legacy_trust.cosign_public_keys.iter().any(|legacy_key| {
        let normalized = legacy_key.key.trim();
        !normalized.is_empty()
            && !trusted_keys
                .cosign_public_keys
                .iter()
                .any(|existing| existing.key.trim() == normalized)
    });

    Ok(missing_minisign || missing_cosign)
}

#[cfg(test)]
mod tests {
    use super::{V2_3_0, run};
    use crate::routines::migrate::MigrationReport;
    use crate::routines::migrate::step::Step;
    use crate::utils::test_support;
    use std::path::{Path, PathBuf};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        test_support::temp_root("upstream-migrate-v2-3-test", name)
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
        Ok(())
    }

    #[test]
    fn check_detects_legacy_config_trust_keys() {
        let root = temp_root("check-trust-config");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.dirs.config_dir).expect("create config");
        fs::write(
            &paths.config.config_file,
            include_str!("../../../../tests/fixtures/storage/legacy-config-with-trust.toml"),
        )
        .expect("write legacy config");

        assert!(V2_3_0::check(&paths).expect("check migration"));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn migrate_moves_legacy_config_trust_keys_to_trust_storage() {
        let root = temp_root("trust-config");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.dirs.config_dir).expect("create config");
        fs::create_dir_all(&paths.dirs.metadata_dir).expect("create metadata");
        fs::write(
            &paths.config.config_file,
            include_str!("../../../../tests/fixtures/storage/legacy-config-with-trust.toml"),
        )
        .expect("write legacy config");
        let mut report = MigrationReport::default();

        run(&paths, &mut report).expect("migrate trust config");

        assert_eq!(report.migrated_trusted_keys, 2);
        let trust_json: serde_json::Value = serde_json::from_slice(
            &fs::read(&paths.config.trust_file).expect("read trust storage"),
        )
        .expect("parse trust storage");
        assert_eq!(
            trust_json["minisign_public_keys"][0]["id"].as_str(),
            Some("mini")
        );
        assert_eq!(
            trust_json["cosign_public_keys"][0]["id"].as_str(),
            Some("cosign")
        );
        assert!(!V2_3_0::check(&paths).expect("check after migration"));

        cleanup(&root).expect("cleanup");
    }
}
