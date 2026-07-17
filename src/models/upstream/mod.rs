pub mod config;
pub mod authentication;
pub mod package;
pub mod package_ref;

pub use authentication::{AuthenticationConfig, ProviderAuthentication};
pub use package::{InstallType, Package};
pub use package_ref::PackageReference;
