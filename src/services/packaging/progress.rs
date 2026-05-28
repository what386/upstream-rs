#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackagePhase {
    CreatingSnapshot,
    RemovingRuntimeLinks,
    ResolvingRelease,
    DownloadingPackage,
    ChecksummingPackage,
    VerifyingSignature,
    InstallingPackage,
    ExtractingPackage,
    CreatingRuntimeLinks,
    CreatingDesktopEntry,
    InstallingCompletions,
    SavingMetadata,
    RebuildingFromSource,
    RemovingPackage,
    RemovingMetadata,
    PurgingPackageData,
    RollingBack,
    RestoringSnapshot,
}

impl PackagePhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::CreatingSnapshot => "Creating snapshot ...",
            Self::RemovingRuntimeLinks => "Removing runtime links ...",
            Self::ResolvingRelease => "Resolving release ...",
            Self::DownloadingPackage => "Downloading package ...",
            Self::ChecksummingPackage => "Checksumming package ...",
            Self::VerifyingSignature => "Verifying signature ...",
            Self::InstallingPackage => "Installing package ...",
            Self::ExtractingPackage => "Extracting package ...",
            Self::CreatingRuntimeLinks => "Creating runtime links ...",
            Self::CreatingDesktopEntry => "Creating desktop entry ...",
            Self::InstallingCompletions => "Installing completions ...",
            Self::SavingMetadata => "Saving metadata ...",
            Self::RebuildingFromSource => "Rebuilding from source ...",
            Self::RemovingPackage => "Removing package ...",
            Self::RemovingMetadata => "Removing metadata ...",
            Self::PurgingPackageData => "Purging package data ...",
            Self::RollingBack => "Rolling back ...",
            Self::RestoringSnapshot => "Restoring snapshot ...",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageProgressEvent {
    Phase(PackagePhase),
    Download { downloaded: u64, total: u64 },
    Warning(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationPhase {
    SerializingManifest,
    WritingManifest,
    ScanningFiles,
    ArchivingFiles,
    FinalizingArchive,
    ImportingManifest,
    ImportingKeys,
    ExtractingSnapshot,
    CreatingSnapshotBackup,
    RestoringSnapshot,
    LoadingMetadata,
}

impl OperationPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::SerializingManifest => "Serializing manifest ...",
            Self::WritingManifest => "Writing manifest ...",
            Self::ScanningFiles => "Scanning files ...",
            Self::ArchivingFiles => "Archiving files ...",
            Self::FinalizingArchive => "Finalizing archive ...",
            Self::ImportingManifest => "Importing manifest ...",
            Self::ImportingKeys => "Importing keys ...",
            Self::ExtractingSnapshot => "Extracting snapshot ...",
            Self::CreatingSnapshotBackup => "Creating snapshot backup ...",
            Self::RestoringSnapshot => "Restoring snapshot ...",
            Self::LoadingMetadata => "Loading metadata ...",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationProgressEvent {
    Phase(OperationPhase),
    Count { done: u64, total: u64 },
    Warning(String),
    Detail(String),
}
