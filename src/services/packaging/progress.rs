#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackagePhase {
    CreatingSnapshot,
    RemovingRuntimeLinks,
    DownloadingPackage,
    ChecksummingPackage,
    VerifyingSignature,
    InstallingPackage,
    ExtractingPackage,
    CreatingRuntimeLinks,
    InstallingCompletions,
    RebuildingFromSource,
    RollingBack,
    RestoringSnapshot,
}

impl PackagePhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::CreatingSnapshot => "Creating snapshot ...",
            Self::RemovingRuntimeLinks => "Removing runtime links ...",
            Self::DownloadingPackage => "Downloading package ...",
            Self::ChecksummingPackage => "Checksumming package ...",
            Self::VerifyingSignature => "Verifying signature ...",
            Self::InstallingPackage => "Installing package ...",
            Self::ExtractingPackage => "Extracting package ...",
            Self::CreatingRuntimeLinks => "Creating runtime links ...",
            Self::InstallingCompletions => "Installing completions ...",
            Self::RebuildingFromSource => "Rebuilding from source ...",
            Self::RollingBack => "Rolling back ...",
            Self::RestoringSnapshot => "Restoring snapshot ...",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageProgressEvent {
    Phase(PackagePhase),
    Warning(String),
}
