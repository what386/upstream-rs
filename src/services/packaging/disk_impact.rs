use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use walkdir::WalkDir;

#[cfg(unix)]
use std::collections::HashSet;

use crate::models::common::enums::Filetype;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeConfidence {
    Exact,
    Estimated,
    AtLeast,
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

    pub fn at_least(bytes: u64) -> Self {
        Self {
            bytes: Some(bytes),
            confidence: SizeConfidence::AtLeast,
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

impl std::ops::Add for ByteEstimate {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        add_unsigned(self, other)
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
            (Some(left), Some(right)) => Self {
                bytes: left.checked_add(right),
                confidence: combine_confidence(self.confidence, other.confidence),
            },

            // Unlike unsigned sizes, an unknown signed component could move
            // the total in either direction, so there is no valid bound.
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
        self.download = self.download + other.download;
        self.net = self.net + other.net;
        self
    }
}

/// Return the approximate allocated disk space occupied by `path`.
///
/// On Unix this uses allocated filesystem blocks and avoids counting the
/// same regular-file inode more than once, so hardlinks are handled correctly.
///
/// On non-Unix platforms this falls back to logical metadata length.
pub fn estimate_path_size(path: &Path) -> Result<u64> {
    let root_metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(0),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("Failed to read metadata for '{}'", path.display()));
        }
    };

    #[cfg(unix)]
    let mut seen_files = HashSet::new();

    // WalkDir includes the root directory itself, so handling the root
    // separately is only necessary for non-directories.
    if !root_metadata.is_dir() {
        #[cfg(unix)]
        return metadata_disk_usage(&root_metadata, &mut seen_files);

        #[cfg(not(unix))]
        return metadata_disk_usage(&root_metadata);
    }

    let mut total = 0_u64;

    for entry in WalkDir::new(path).follow_links(false) {
        crate::application::cancellation::check()?;
        let entry = entry.with_context(|| format!("Failed to scan '{}'", path.display()))?;

        let metadata = fs::symlink_metadata(entry.path())
            .with_context(|| format!("Failed to read metadata for '{}'", entry.path().display()))?;

        #[cfg(unix)]
        let size = metadata_disk_usage(&metadata, &mut seen_files)?;

        #[cfg(not(unix))]
        let size = metadata_disk_usage(&metadata)?;

        total = total
            .checked_add(size)
            .ok_or_else(|| anyhow!("Disk usage overflow while scanning '{}'", path.display()))?;
    }

    Ok(total)
}

#[cfg(unix)]
fn metadata_disk_usage(
    metadata: &fs::Metadata,
    seen_files: &mut HashSet<(u64, u64)>,
) -> Result<u64> {
    use std::os::unix::fs::MetadataExt;

    if metadata.file_type().is_file() {
        let identity = (metadata.dev(), metadata.ino());

        // Multiple hardlinks refer to the same allocated data.
        if !seen_files.insert(identity) {
            return Ok(0);
        }
    }

    metadata
        .blocks()
        .checked_mul(512)
        .ok_or_else(|| anyhow!("Allocated disk size overflow"))
}

#[cfg(not(unix))]
fn metadata_disk_usage(metadata: &fs::Metadata) -> Result<u64> {
    Ok(metadata.len())
}

pub fn estimate_existing_paths(paths: impl IntoIterator<Item = impl AsRef<Path>>) -> Result<u64> {
    let mut total = 0_u64;

    for path in paths {
        total = total
            .checked_add(estimate_path_size(path.as_ref())?)
            .ok_or_else(|| anyhow!("Disk usage overflow"))?;
    }

    Ok(total)
}

/// Use this when the asset metadata definitely supplied a size.
///
/// Zero is a legitimate size and is no longer treated as "unknown".
pub fn asset_size_estimate(bytes: u64) -> ByteEstimate {
    ByteEstimate::estimated(bytes)
}

/// Use this at boundaries where the upstream API may omit the size.
pub fn optional_asset_size_estimate(bytes: Option<u64>) -> ByteEstimate {
    match bytes {
        Some(bytes) => ByteEstimate::estimated(bytes),
        None => ByteEstimate::unknown(),
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
        (Some(a), Some(b)) => {
            let Some(bytes) = a.checked_add(b) else {
                return ByteEstimate::unknown();
            };

            ByteEstimate {
                bytes: Some(bytes),
                confidence: combine_confidence(left.confidence, right.confidence),
            }
        }

        // An unsigned unknown can only add >= 0 bytes, so the known subtotal
        // remains a valid lower bound.
        (Some(bytes), None) => ByteEstimate::at_least(bytes),
        (None, Some(bytes)) => ByteEstimate::at_least(bytes),

        (None, None) => ByteEstimate::unknown(),
    }
}

fn combine_confidence(left: SizeConfidence, right: SizeConfidence) -> SizeConfidence {
    use SizeConfidence::*;

    match (left, right) {
        (Unknown, _) | (_, Unknown) => Unknown,

        (Exact, Exact) => Exact,

        // A lower bound plus an exact/lower-bound value remains a lower bound.
        (AtLeast, Exact) | (Exact, AtLeast) | (AtLeast, AtLeast) => AtLeast,

        // Once an ordinary estimate participates, we no longer have a
        // mathematically meaningful lower bound.
        (Estimated, _) | (_, Estimated) => Estimated,
    }
}

pub fn estimate_fresh_install(filetype: Filetype, download: ByteEstimate) -> DiskImpact {
    let net = match (filetype, download.bytes) {
        (Filetype::Archive | Filetype::Compressed, _) => SignedByteEstimate::unknown(),

        (_, Some(bytes)) => SignedByteEstimate {
            bytes: Some(i128::from(bytes)),
            confidence: download.confidence,
        },

        (_, None) => SignedByteEstimate::unknown(),
    };

    DiskImpact { download, net }
}

pub fn estimate_upgrade(
    filetype: Filetype,
    download: ByteEstimate,
    current_installed: u64,
) -> DiskImpact {
    let net = match (filetype, download.bytes) {
        // Cheap heuristic: assume archive upgrades install to roughly
        // the same size as the current version.
        (Filetype::Archive | Filetype::Compressed, _) => SignedByteEstimate::estimated(0),

        (_, Some(new_size)) => {
            SignedByteEstimate::estimated(i128::from(new_size) - i128::from(current_installed))
        }

        (_, None) => SignedByteEstimate::unknown(),
    };

    DiskImpact { download, net }
}

#[cfg(test)]
mod tests {
    use super::{ByteEstimate, SignedByteEstimate, SizeConfidence, estimate_path_size};
    use std::fs;

    #[test]
    fn exact_unsigned_values_add_exactly() {
        let total = ByteEstimate::exact(10) + ByteEstimate::exact(20);

        assert_eq!(total.bytes, Some(30));
        assert_eq!(total.confidence, SizeConfidence::Exact);
    }

    #[test]
    fn estimated_unsigned_value_makes_total_estimated() {
        let total = ByteEstimate::exact(10) + ByteEstimate::estimated(20);

        assert_eq!(total.bytes, Some(30));
        assert_eq!(total.confidence, SizeConfidence::Estimated);
    }

    #[test]
    fn known_unsigned_plus_unknown_becomes_lower_bound() {
        let total = ByteEstimate::exact(10) + ByteEstimate::unknown();

        assert_eq!(total.bytes, Some(10));
        assert_eq!(total.confidence, SizeConfidence::AtLeast);
    }

    #[test]
    fn two_unknown_unsigned_values_remain_unknown() {
        let total = ByteEstimate::unknown() + ByteEstimate::unknown();

        assert_eq!(total.bytes, None);
        assert_eq!(total.confidence, SizeConfidence::Unknown);
    }

    #[test]
    fn signed_estimates_add_and_preserve_estimated_confidence() {
        let total = SignedByteEstimate::exact(10) + SignedByteEstimate::estimated(-3);

        assert_eq!(total.bytes, Some(7));
        assert_eq!(total.confidence, SizeConfidence::Estimated);
    }

    #[test]
    fn signed_known_plus_unknown_is_unknown() {
        let total = SignedByteEstimate::exact(-10) + SignedByteEstimate::unknown();

        assert_eq!(total.bytes, None);
        assert_eq!(total.confidence, SizeConfidence::Unknown);
    }

    #[test]
    fn zero_asset_size_is_not_unknown() {
        let estimate = super::asset_size_estimate(0);

        assert_eq!(estimate.bytes, Some(0));
        assert_eq!(estimate.confidence, SizeConfidence::Estimated);
    }

    #[test]
    fn path_size_counts_nested_files() {
        let root =
            std::env::temp_dir().join(format!("upstream-disk-impact-test-{}", std::process::id()));

        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("nested")).expect("create dir");
        fs::write(root.join("a"), b"abc").expect("write a");
        fs::write(root.join("nested").join("b"), b"defg").expect("write b");

        // Allocated disk usage is intentionally filesystem-dependent, so
        // don't assert that it equals the seven logical payload bytes.
        assert!(estimate_path_size(&root).expect("size") > 0);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn hardlinks_are_not_double_counted() {
        let root = std::env::temp_dir().join(format!(
            "upstream-disk-impact-hardlink-test-{}",
            std::process::id()
        ));

        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create dir");

        let original = root.join("original");
        let link = root.join("link");

        fs::write(&original, vec![0_u8; 8192]).expect("write original");
        fs::hard_link(&original, &link).expect("create hardlink");

        let one = estimate_path_size(&original).expect("single file size");
        let tree = estimate_path_size(&root).expect("tree size");

        // The directory itself consumes some space, but the payload should
        // not suddenly double merely because a second hardlink exists.
        assert!(tree < one.saturating_mul(2).saturating_add(8192));

        fs::remove_dir_all(root).expect("cleanup");
    }
}
