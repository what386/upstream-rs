use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeConfidence {
    Exact,
    Estimated,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteEstimate {
    pub bytes: Option<u64>,
    pub confidence: SizeConfidence,
}

impl ByteEstimate {
    pub fn exact(bytes: u64) -> Self {
        Self {
            bytes: Some(bytes),
            confidence: SizeConfidence::Exact,
        }
    }

    pub fn estimated(bytes: u64) -> Self {
        Self {
            bytes: Some(bytes),
            confidence: SizeConfidence::Estimated,
        }
    }

    pub fn unknown() -> Self {
        Self {
            bytes: None,
            confidence: SizeConfidence::Unknown,
        }
    }

    pub fn is_unknown(self) -> bool {
        self.bytes.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignedByteEstimate {
    pub bytes: Option<i128>,
    pub confidence: SizeConfidence,
}

impl SignedByteEstimate {
    pub fn exact(bytes: i128) -> Self {
        Self {
            bytes: Some(bytes),
            confidence: SizeConfidence::Exact,
        }
    }

    pub fn estimated(bytes: i128) -> Self {
        Self {
            bytes: Some(bytes),
            confidence: SizeConfidence::Estimated,
        }
    }

    pub fn unknown() -> Self {
        Self {
            bytes: None,
            confidence: SizeConfidence::Unknown,
        }
    }

    pub fn is_unknown(self) -> bool {
        self.bytes.is_none()
    }
}

impl std::ops::Add for SignedByteEstimate {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        match (self.bytes, other.bytes) {
            (Some(left), Some(right)) => {
                let confidence = combine_confidence(self.confidence, other.confidence);
                Self {
                    bytes: Some(left.saturating_add(right)),
                    confidence,
                }
            }
            _ => Self::unknown(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskImpact {
    pub download: ByteEstimate,
    pub net: SignedByteEstimate,
}

impl DiskImpact {
    pub fn empty() -> Self {
        Self {
            download: ByteEstimate::exact(0),
            net: SignedByteEstimate::exact(0),
        }
    }

    pub fn unknown() -> Self {
        Self {
            download: ByteEstimate::unknown(),
            net: SignedByteEstimate::unknown(),
        }
    }
}

impl std::ops::Add for DiskImpact {
    type Output = Self;

    fn add(mut self, other: Self) -> Self {
        self.download = add_unsigned(self.download, other.download);
        self.net = self.net + other.net;
        self
    }
}

pub fn estimate_path_size(path: &Path) -> Result<u64> {
    if !path.exists() {
        return Ok(0);
    }

    if path.is_file() || path.is_symlink() {
        return fs::symlink_metadata(path)
            .map(|metadata| metadata.len())
            .with_context(|| format!("Failed to read metadata for '{}'", path.display()));
    }

    let mut total = 0_u64;
    for entry in WalkDir::new(path).follow_links(false) {
        let entry = entry.with_context(|| format!("Failed to scan '{}'", path.display()))?;
        if entry.file_type().is_file() || entry.file_type().is_symlink() {
            let metadata = entry.metadata().with_context(|| {
                format!("Failed to read metadata for '{}'", entry.path().display())
            })?;
            total = total.saturating_add(metadata.len());
        }
    }
    Ok(total)
}

pub fn estimate_existing_paths(paths: impl IntoIterator<Item = impl AsRef<Path>>) -> Result<u64> {
    let mut total = 0_u64;
    for path in paths {
        total = total.saturating_add(estimate_path_size(path.as_ref())?);
    }
    Ok(total)
}

pub fn asset_size_estimate(bytes: u64) -> ByteEstimate {
    if bytes == 0 {
        ByteEstimate::unknown()
    } else {
        ByteEstimate::estimated(bytes)
    }
}

pub fn install_impact_from_download(download: ByteEstimate) -> DiskImpact {
    let net = match download.bytes {
        Some(bytes) => SignedByteEstimate {
            bytes: Some(i128::from(bytes)),
            confidence: download.confidence,
        },
        None => SignedByteEstimate::unknown(),
    };
    DiskImpact { download, net }
}

fn add_unsigned(left: ByteEstimate, right: ByteEstimate) -> ByteEstimate {
    match (left.bytes, right.bytes) {
        (Some(a), Some(b)) => ByteEstimate {
            bytes: Some(a.saturating_add(b)),
            confidence: combine_confidence(left.confidence, right.confidence),
        },
        (Some(bytes), None) | (None, Some(bytes)) => ByteEstimate {
            bytes: Some(bytes),
            confidence: SizeConfidence::Estimated,
        },
        (None, None) => ByteEstimate::unknown(),
    }
}

fn combine_confidence(left: SizeConfidence, right: SizeConfidence) -> SizeConfidence {
    match (left, right) {
        (SizeConfidence::Unknown, _) | (_, SizeConfidence::Unknown) => SizeConfidence::Unknown,
        (SizeConfidence::Estimated, _) | (_, SizeConfidence::Estimated) => {
            SizeConfidence::Estimated
        }
        (SizeConfidence::Exact, SizeConfidence::Exact) => SizeConfidence::Exact,
    }
}

#[cfg(test)]
mod tests {
    use super::{ByteEstimate, DiskImpact, SignedByteEstimate, estimate_path_size};
    use std::fs;

    #[test]
    fn signed_estimates_add_and_preserve_estimated_confidence() {
        let total = SignedByteEstimate::exact(10) + SignedByteEstimate::estimated(-3);
        assert_eq!(total.bytes, Some(7));
        assert_eq!(format!("{:?}", total.confidence), "Estimated");
    }

    #[test]
    fn path_size_counts_nested_files() {
        let root =
            std::env::temp_dir().join(format!("upstream-disk-impact-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("nested")).expect("create dir");
        fs::write(root.join("a"), b"abc").expect("write a");
        fs::write(root.join("nested").join("b"), b"defg").expect("write b");

        assert_eq!(estimate_path_size(&root).expect("size"), 7);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn download_aggregation_keeps_known_sizes_when_some_are_unknown() {
        let known = DiskImpact {
            download: ByteEstimate::exact(1024),
            net: SignedByteEstimate::exact(0),
        };
        let unknown = DiskImpact {
            download: ByteEstimate::unknown(),
            net: SignedByteEstimate::exact(0),
        };

        let total = known + unknown;

        assert_eq!(total.download.bytes, Some(1024));
        assert_eq!(format!("{:?}", total.download.confidence), "Estimated");
    }

    #[test]
    fn download_aggregation_stays_unknown_when_everything_is_unknown() {
        let total = DiskImpact::unknown() + DiskImpact::unknown();

        assert_eq!(total.download.bytes, None);
        assert_eq!(format!("{:?}", total.download.confidence), "Unknown");
    }
}
