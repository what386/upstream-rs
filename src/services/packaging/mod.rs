pub mod bundles;
pub mod disk_impact;
pub mod installer;
pub mod package_installer;
pub mod progress;
pub mod remover;
pub mod rollback;
pub mod upgrader;

pub use installer::PackageChecker;
pub use package_installer::{InstallPreview, PackageInstaller};
pub use progress::{OperationPhase, OperationProgressEvent, PackagePhase, PackageProgressEvent};
pub use remover::PackageRemover;
pub use rollback::RollbackManager;
pub use upgrader::{PackageUpgrader, ResolvedUpgradeTarget};
