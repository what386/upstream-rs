pub mod config;
pub mod authentication;
pub mod package;
pub mod package_reference;

pub use authentication::{AuthenticationConfig, ProviderAuthentication};
pub use package::{InstallType, Package};
pub use package_reference::PackageReference;
