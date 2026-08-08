mod checker;
pub mod disk_impact;
mod installer;
pub mod progress;
mod remover;
mod replacement;
pub mod rollback;
mod upgrader;
mod staging;

use std::path::PathBuf;

use crate::{
    models::{
        common::{enums::TrustMode, version::Version},
        provider::{Asset, Release},
        upstream::Package,
    },
    services::packaging::disk_impact::DiskImpact,
};

/// The caller's requested source for a normal package installation.
#[derive(Clone)]
pub enum InstallSource {
    Release {
        version: Option<String>,
        semver: Option<String>,
    },
    SelectedRelease {
        release: Release,
        asset: Asset,
    },
    LocalArtifact {
        artifact_path: PathBuf,
        version: Version,
    },
}

#[derive(Clone)]
pub struct InstallRequest {
    pub package: Package,
    pub source: InstallSource,
    pub add_entry: bool,
    pub trust_mode: TrustMode,
}

/// An immutable install decision. Release plans execute their selected asset
/// directly and never resolve a different latest release.
#[derive(Clone)]
pub struct InstallPlan {
    pub package: Package,
    pub source: PlannedInstallSource,
    pub add_entry: bool,
    pub trust_mode: TrustMode,
    pub disk_impact: DiskImpact,
}

#[derive(Clone)]
pub enum PlannedInstallSource {
    Release {
        release: Release,
        asset: Asset,
    },
    LocalArtifact {
        artifact_path: PathBuf,
        version: Version,
    },
}

impl InstallPlan {
    pub fn package(&self) -> &Package {
        &self.package
    }

    pub fn resolved_asset(&self) -> Option<ResolvedAssetInstall> {
        let PlannedInstallSource::Release { release, asset } = &self.source else {
            return None;
        };
        let resolved_filetype =
            if self.package.filetype == crate::models::common::enums::Filetype::Auto {
                asset.filetype
            } else {
                self.package.filetype
            };
        Some(ResolvedAssetInstall {
            release_name: release.name.clone(),
            release_tag: release.tag.clone(),
            asset_name: asset.name.clone(),
            resolved_filetype,
            disk_impact: self.disk_impact.clone(),
            release: release.clone(),
            asset: asset.clone(),
        })
    }
}

pub use checker::PackageChecker;
pub use installer::PackageInstaller;
pub use installer::ResolvedAssetInstall;
pub use progress::{OperationPhase, OperationProgressEvent, PackagePhase, PackageProgressEvent};
pub use remover::PackageRemover;
pub use replacement::{PackageReplacer, PreparedInstall};
pub use rollback::RollbackManager;
pub use upgrader::PackageUpgrader;
pub use upgrader::ResolvedUpgradeTarget;
pub use staging::InstallWorkspace;
