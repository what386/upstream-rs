use anyhow::Result;

use crate::{
    application::cancellation::Cancellation,
    models::upstream::config::AppConfig,
    providers::provider_manager::ProviderManager,
    services::trust::TrustedSignatureKeys,
    storage::{
        database::PackageDatabase,
        system::{auth::AuthStorage, trust::TrustStorage},
    },
    utils::static_paths::UpstreamPaths,
};

pub struct CommandContext<'a> {
    pub paths: &'a UpstreamPaths,
    pub provider_manager: ProviderManager,
    pub app_config: &'a AppConfig,
    pub cancellation: Cancellation,
}

impl<'a> CommandContext<'a> {
    pub fn new(paths: &'a UpstreamPaths, app_config: &'a AppConfig) -> Result<Self> {
        let auth = AuthStorage::new(&paths.metadata.auth_file)?;
        let provider_manager = ProviderManager::new(
            auth.get_auth().github.api_token.as_deref(),
            auth.get_auth().gitlab.api_token.as_deref(),
            auth.get_auth().gitea.api_token.as_deref(),
            app_config.download,
        )?;

        Ok(Self {
            paths,
            provider_manager,
            app_config,
            cancellation: crate::application::cancellation::current(),
        })
    }

    pub fn package_database(&self) -> Result<PackageDatabase> {
        PackageDatabase::open(&self.paths.metadata.packages_database_file)
    }

    pub fn trust_storage(&self) -> Result<TrustStorage> {
        TrustStorage::new(&self.paths.metadata.trust_file)
    }

    pub fn trusted_keys(&self) -> Result<TrustedSignatureKeys> {
        Ok(self.trust_storage()?.trusted_signature_keys())
    }
}
