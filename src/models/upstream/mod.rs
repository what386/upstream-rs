pub mod authentication;
pub mod config;
pub mod install_plan;
pub mod package;
pub mod package_ref;

pub use authentication::{AuthenticationConfig, ProviderAuthentication};
pub use install_plan::{
    BuildInstallSource, BuildSelector, HttpInstallSource, InstallPlan, InstallSource,
    ReleaseInstallSource, ReleaseSelector,
};
pub use package::{InstallType, Package};
pub use package_ref::PackageReference;
