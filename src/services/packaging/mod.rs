pub mod bundles;
pub mod checker;
pub mod disk_impact;
pub mod installer;
pub mod progress;
pub mod remover;
pub mod rollback;
pub mod upgrader;

pub use checker::PackageChecker;
pub use installer::{InstallPlan, PackageInstaller};
pub use progress::{OperationPhase, OperationProgressEvent, PackagePhase, PackageProgressEvent};
pub use remover::PackageRemover;
pub use rollback::RollbackManager;
pub use upgrader::{PackageUpgrader, ResolvedUpgradeTarget};
