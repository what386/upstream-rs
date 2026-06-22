use anyhow::Result;

use crate::{
    providers::provider_manager::ProviderManager,
    services::trust::TrustedSignatureKeys,
    storage::{
        package_storage::PackageStorage,
        system::{config::ConfigStorage, trust::TrustStorage},
    },
    utils::static_paths::UpstreamPaths,
};

pub struct CommandContext {
    pub paths: UpstreamPaths,
    pub provider_manager: ProviderManager,
}

impl CommandContext {
    pub fn new() -> Result<Self> {
        let paths = UpstreamPaths::new()?;
        let config = ConfigStorage::new(&paths.config.config_file)?;
        let app_config = config.get_config();
        let provider_manager = ProviderManager::new(
            app_config.github.api_token.as_deref(),
            app_config.gitlab.api_token.as_deref(),
            app_config.gitea.api_token.as_deref(),
            app_config.download,
        )?;

        Ok(Self {
            paths,
            provider_manager,
        })
    }

    pub fn package_storage(&self) -> Result<PackageStorage> {
        PackageStorage::new(&self.paths.config.packages_file)
    }

    pub fn trust_storage(&self) -> Result<TrustStorage> {
        TrustStorage::new(&self.paths.config.trust_file)
    }

    pub fn trusted_keys(&self) -> Result<TrustedSignatureKeys> {
        Ok(self.trust_storage()?.trusted_signature_keys())
    }
}
