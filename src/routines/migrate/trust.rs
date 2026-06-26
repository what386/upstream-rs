use std::fs;

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use crate::routines::migrate::MigrationReport;
use crate::services::trust::{CosignPublicKey, MinisignPublicKey};
use crate::storage::system::trust::TrustStorage;
use crate::utils::static_paths::UpstreamPaths;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct LegacyTrustConfig {
    minisign_public_keys: Vec<MinisignPublicKey>,
    cosign_public_keys: Vec<CosignPublicKey>,
}

pub(in crate::routines::migrate) fn migrate_trust_config(
    paths: &UpstreamPaths,
    report: &mut MigrationReport,
) -> Result<()> {
    let mut trust_storage = TrustStorage::new(&paths.config.trust_file)?;

    if !paths.config.config_file.exists() {
        trust_storage.ensure_exists()?;
        return Ok(());
    }

    let raw_config = fs::read_to_string(&paths.config.config_file).with_context(|| {
        format!(
            "Failed to read config '{}'",
            paths.config.config_file.display()
        )
    })?;
    if raw_config.trim().is_empty() {
        trust_storage.ensure_exists()?;
        return Ok(());
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

    if let Some(trust_value) = config_table.get("trust") {
        let legacy_trust: LegacyTrustConfig = trust_value
            .clone()
            .try_into()
            .context("Failed to parse legacy config trust keys")?;
        let summary = trust_storage.merge_trusted_keys(
            &legacy_trust.minisign_public_keys,
            &legacy_trust.cosign_public_keys,
        )?;
        report.migrated_trusted_keys += summary.minisign.imported + summary.cosign.imported;
        report.deduped_trusted_keys += summary.minisign.deduped + summary.cosign.deduped;
    } else {
        trust_storage.ensure_exists()?;
    }

    Ok(())
}
