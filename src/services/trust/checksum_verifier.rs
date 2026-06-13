use crate::{
    models::{
        common::enums::Provider,
        provider::{Asset, Release},
    },
    providers::provider_manager::ProviderManager,
};
use anyhow::{Result, anyhow};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum HashAlgo {
    Sha256,
    Sha512,
}

struct ChecksumEntry {
    algo: HashAlgo,
    filename: String,
    digest: String,
}

struct DownloadedChecksumAsset {
    name: String,
    path: PathBuf,
}

const COMMON_CHECKSUM_NAMES: &[&str] = &[
    "checksums-bsd",
    "checksums-bsd.txt",
    "checksums.txt",
    "checksum.txt",
    "sha256sums.txt",
    "sha256sum.txt",
    "sha256sums",
    "sha256sum",
    "sha512sums.txt",
    "sha512sum.txt",
    "sha512sums",
    "sha512sum",
    "checksums",
];

pub fn is_checksum_asset_name(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    COMMON_CHECKSUM_NAMES
        .iter()
        .any(|common| lowered == *common)
        || lowered.ends_with(".sha256")
        || lowered.ends_with(".sha512")
        || lowered.ends_with(".sha256sum")
        || lowered.ends_with(".sha512sum")
        || lowered.ends_with(".sha256.txt")
        || lowered.ends_with(".sha512.txt")
        || lowered.ends_with(".sum")
        || lowered.contains("checksums")
}

#[derive(Debug, Clone)]
pub struct VerifiedChecksumAsset {
    pub name: String,
    pub path: PathBuf,
}

pub enum ChecksumVerificationResult {
    Verified(VerifiedChecksumAsset),
    Missing,
}

pub struct ChecksumVerifier<'a> {
    provider_manager: &'a ProviderManager,
    download_cache: &'a Path,
}

impl<'a> ChecksumVerifier<'a> {
    pub fn new(provider_manager: &'a ProviderManager, download_cache: &'a Path) -> Self {
        Self {
            provider_manager,
            download_cache,
        }
    }

    pub async fn try_verify_file<F>(
        &self,
        asset_path: &Path,
        release: &Release,
        provider: &Provider,
        dl_progress: &mut Option<F>,
    ) -> Result<ChecksumVerificationResult>
    where
        F: FnMut(u64, u64),
    {
        let asset_filename = asset_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow!("Invalid asset filename"))?;

        // Try to download the checksum file
        let checksum_path = match self
            .try_download_checksum(release, asset_filename, provider, dl_progress)
            .await?
        {
            Some(path) => path,
            None => return Ok(ChecksumVerificationResult::Missing), // No checksum available, that's ok
        };

        // Read and parse the checksum file
        let contents = fs::read_to_string(&checksum_path.path)?;
        let mut entries = Self::parse_checksums(&contents);

        if entries.is_empty() && Self::looks_like_matrix_manifest(&contents) {
            let order_path = self
                .try_download_checksum_order(release, provider, dl_progress)
                .await?
                .ok_or_else(|| {
                    anyhow!(
                        "Checksum file '{}' uses a matrix format but release does not expose 'checksums_hashes_order'",
                        checksum_path.name
                    )
                })?;

            let order_contents = fs::read_to_string(order_path)?;
            entries = Self::parse_matrix_checksums(&contents, &order_contents)?;
        }

        if entries.is_empty() {
            return Err(anyhow!("Checksum file is empty or invalid"));
        }

        let checksum_entry = if entries.len() == 1 && entries[0].filename.is_empty() {
            // Bare hash file - assume it's for the asset we're verifying
            &entries[0]
        } else {
            // Standard multi-entry checksum file
            entries
                .iter()
                .find(|entry| entry.filename == asset_filename)
                .or_else(|| {
                    // If exact match not found, try matching just the basename
                    entries.iter().find(|entry| {
                        Path::new(&entry.filename)
                            .file_name()
                            .and_then(|n| n.to_str())
                            == Some(asset_filename)
                    })
                })
                .ok_or_else(|| {
                    anyhow!(
                        "No checksum found for asset '{}' in checksum file",
                        asset_filename
                    )
                })?
        };

        if Self::verify_checksum(asset_path, checksum_entry)? {
            return Ok(ChecksumVerificationResult::Verified(
                VerifiedChecksumAsset {
                    name: checksum_path.name,
                    path: checksum_path.path,
                },
            ));
        }

        Err(anyhow!("Checksum mismatch for asset '{}'", asset_filename))
    }

    /// Locate and download a checksum asset, if the release exposes one.
    async fn try_download_checksum<F>(
        &self,
        release: &Release,
        asset_name: &str,
        provider: &Provider,
        dl_progress: &mut Option<F>,
    ) -> Result<Option<DownloadedChecksumAsset>>
    where
        F: FnMut(u64, u64),
    {
        let checksum_asset = Self::find_checksum_asset(release, asset_name);

        let Some(asset) = checksum_asset else {
            return Ok(None); // no checksum advertised
        };

        // If this fails, it's a real error
        let path = self
            .provider_manager
            .download_asset(asset, provider, self.download_cache, dl_progress)
            .await?;

        Ok(Some(DownloadedChecksumAsset {
            name: asset.name.clone(),
            path,
        }))
    }

    async fn try_download_checksum_order<F>(
        &self,
        release: &Release,
        provider: &Provider,
        dl_progress: &mut Option<F>,
    ) -> Result<Option<PathBuf>>
    where
        F: FnMut(u64, u64),
    {
        let Some(asset) = Self::find_checksum_order_asset(release) else {
            return Ok(None);
        };

        let path = self
            .provider_manager
            .download_asset(asset, provider, self.download_cache, dl_progress)
            .await?;

        Ok(Some(path))
    }

    /// Heuristically find the checksum file that most likely corresponds to
    /// `asset_name`, preferring exact per-asset checksum names first.
    fn find_checksum_asset<'r>(release: &'r Release, asset_name: &str) -> Option<&'r Asset> {
        let basename = Path::new(asset_name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(asset_name);

        let specific_candidates = [
            format!("{asset_name}.sha256"),
            format!("{asset_name}.sha512"),
            format!("{basename}.sha256"),
            format!("{basename}.sha512"),
            format!("{basename}.sha256sum"),
            format!("{basename}.sha512sum"),
        ];

        for candidate in &specific_candidates {
            if let Some(asset) = release.get_asset_by_name_invariant(candidate) {
                return Some(asset);
            }
        }

        for name in COMMON_CHECKSUM_NAMES {
            if let Some(asset) = release.get_asset_by_name_invariant(name) {
                return Some(asset);
            }
        }

        release
            .assets
            .iter()
            .find(|asset| is_checksum_asset_name(&asset.name))
    }

    fn find_checksum_order_asset(release: &Release) -> Option<&Asset> {
        release.get_asset_by_name_invariant("checksums_hashes_order")
    }

    /// Parse checksum text that may contain GNU/coreutils, colon, OpenSSL, or
    /// bare-hash formats.
    fn parse_checksums(contents: &str) -> Vec<ChecksumEntry> {
        let mut entries = Vec::new();

        for line in contents.lines() {
            let line = line.trim();
            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Try to parse different checksum formats:
            // 1. "digest  filename" (two spaces, standard format)
            // 2. "digest *filename" (asterisk before filename, binary mode)
            // 3. "digest filename" (single space)
            // 4. "filename: digest" (colon-separated)
            // 5. "digest" (bare hash, no filename)
            if let Some(entry) = Self::parse_standard_format(line) {
                entries.push(entry);
            } else if let Some(entry) = Self::parse_colon_format(line) {
                entries.push(entry);
            } else if let Some(entry) = Self::parse_openssl_format(line) {
                entries.push(entry);
            } else if let Some(entry) = Self::parse_bare_hash(line) {
                entries.push(entry);
            }
        }

        entries
    }

    fn looks_like_matrix_manifest(contents: &str) -> bool {
        contents.lines().any(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return false;
            }

            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 3 {
                return false;
            }

            Self::parse_digest(fields[0]).is_none()
                && fields[1..]
                    .iter()
                    .any(|field| Self::parse_digest(field).is_some())
        })
    }

    fn parse_matrix_checksums(contents: &str, order_contents: &str) -> Result<Vec<ChecksumEntry>> {
        let hash_order = Self::parse_hash_order(order_contents);
        if hash_order.is_empty() || hash_order.iter().all(Option::is_none) {
            return Err(anyhow!(
                "Checksum hash order file is empty or does not describe supported hashes"
            ));
        }

        let mut entries = Vec::new();

        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 2 {
                continue;
            }

            let filename = fields[0];
            for (index, field) in fields.iter().skip(1).enumerate() {
                let Some(algo) = hash_order.get(index).copied().flatten() else {
                    continue;
                };

                let Some((parsed_algo, normalized)) = Self::parse_digest(field) else {
                    continue;
                };

                if parsed_algo != algo {
                    continue;
                }

                entries.push(ChecksumEntry {
                    algo,
                    filename: filename.to_string(),
                    digest: normalized,
                });
            }
        }

        Ok(entries)
    }

    fn parse_hash_order(order_contents: &str) -> Vec<Option<HashAlgo>> {
        order_contents
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    return None;
                }

                Some(Self::parse_hash_order_entry(line))
            })
            .collect()
    }

    fn parse_hash_order_entry(label: &str) -> Option<HashAlgo> {
        let normalized: String = label
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .map(|ch| ch.to_ascii_lowercase())
            .collect();

        if normalized.contains("sha256") {
            Some(HashAlgo::Sha256)
        } else if normalized.contains("sha512") {
            Some(HashAlgo::Sha512)
        } else {
            None
        }
    }

    /// Parse and normalize a digest token, inferring algorithm from hash length.
    fn parse_digest(raw: &str) -> Option<(HashAlgo, String)> {
        let mut token = raw.trim();
        for prefix in ["sha256:", "sha256=", "sha512:", "sha512="] {
            if token.len() >= prefix.len() && token[..prefix.len()].eq_ignore_ascii_case(prefix) {
                token = &token[prefix.len()..];
                break;
            }
        }
        let token = token.trim();
        if !token.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return None;
        }
        let algo = match token.len() {
            64 => HashAlgo::Sha256,
            128 => HashAlgo::Sha512,
            _ => return None,
        };

        Some((algo, token.to_ascii_lowercase()))
    }

    /// Parse standard `digest filename` formats (including `*filename`).
    fn parse_standard_format(line: &str) -> Option<ChecksumEntry> {
        // Handle formats like:
        // "abc123  filename.tar.gz"
        // "abc123 *filename.tar.gz"
        // "abc123 filename.tar.gz"
        let parts: Vec<&str> = line.splitn(2, |c: char| c.is_whitespace()).collect();
        if parts.len() != 2 {
            return None;
        }

        let digest = parts[0].trim();
        let filename = parts[1].trim().trim_start_matches('*').trim();

        if digest.is_empty() || filename.is_empty() {
            return None;
        }

        let (algo, normalized) = Self::parse_digest(digest)?;

        Some(ChecksumEntry {
            algo,
            filename: filename.to_string(),
            digest: normalized,
        })
    }

    /// Parse `filename: digest` checksum lines.
    fn parse_colon_format(line: &str) -> Option<ChecksumEntry> {
        // Handle format like: "filename.tar.gz: abc123"
        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() != 2 {
            return None;
        }

        let filename = parts[0].trim();
        let digest = parts[1].trim();

        if digest.is_empty() || filename.is_empty() {
            return None;
        }

        let (algo, normalized) = Self::parse_digest(digest)?;

        Some(ChecksumEntry {
            algo,
            filename: filename.to_string(),
            digest: normalized,
        })
    }

    /// Parse OpenSSL lines, e.g. `SHA256(file)= digest`.
    fn parse_openssl_format(line: &str) -> Option<ChecksumEntry> {
        let (left, right) = line.split_once('=')?;
        let left = left.trim();
        let open = left.find('(')?;
        let close = left.rfind(')')?;
        if close <= open + 1 {
            return None;
        }

        let algo_name = left[..open].trim();
        let expected_algo = if algo_name.eq_ignore_ascii_case("sha256") {
            HashAlgo::Sha256
        } else if algo_name.eq_ignore_ascii_case("sha512") {
            HashAlgo::Sha512
        } else {
            return None;
        };

        let filename = left[open + 1..close].trim();
        if filename.is_empty() {
            return None;
        }

        let (algo, normalized) = Self::parse_digest(right.trim())?;
        if algo != expected_algo {
            return None;
        }

        Some(ChecksumEntry {
            algo,
            filename: filename.to_string(),
            digest: normalized,
        })
    }

    /// Parse a single digest line that does not include a filename.
    fn parse_bare_hash(line: &str) -> Option<ChecksumEntry> {
        // Handle bare hash format (just the digest, no filename)
        let (algo, normalized) = Self::parse_digest(line.trim())?;

        // Use empty filename - indicates bare hash file
        Some(ChecksumEntry {
            algo,
            filename: String::new(),
            digest: normalized,
        })
    }

    fn bytes_to_lower_hex(bytes: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(bytes.len() * 2);
        for &byte in bytes {
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
        out
    }

    /// Stream-hash an asset and compare it with the expected digest.
    fn verify_checksum(asset_path: &Path, checksum: &ChecksumEntry) -> Result<bool> {
        use std::io::{BufReader, Read};

        if !asset_path.exists() {
            return Err(anyhow!(
                "Asset file does not exist: {}",
                asset_path.display()
            ));
        }

        let file = fs::File::open(asset_path)?;
        let mut reader = BufReader::new(file);
        let mut buffer = [0u8; 8192]; // 8KB buffer

        let computed_digest = match checksum.algo {
            HashAlgo::Sha256 => {
                use sha2::Digest;
                let mut hasher = sha2::Sha256::new();
                loop {
                    let n = reader.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    hasher.update(&buffer[..n]);
                }
                let digest = hasher.finalize();
                Self::bytes_to_lower_hex(digest.as_ref())
            }
            HashAlgo::Sha512 => {
                use sha2::Digest;
                let mut hasher = sha2::Sha512::new();
                loop {
                    let n = reader.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    hasher.update(&buffer[..n]);
                }
                let digest = hasher.finalize();
                Self::bytes_to_lower_hex(digest.as_ref())
            }
        };

        Ok(computed_digest.to_lowercase() == checksum.digest.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::{ChecksumVerificationResult, ChecksumVerifier};
    use crate::models::common::enums::Provider;
    use crate::models::common::version::Version;
    use crate::models::provider::{Asset, Release};
    use crate::providers::provider_manager::ProviderManager;
    use chrono::Utc;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-checksum-test-{name}-{nanos}"))
    }

    fn empty_release() -> Release {
        Release {
            id: 1,
            tag: "v1.0.0".to_string(),
            name: "v1.0.0".to_string(),
            body: String::new(),
            is_draft: false,
            is_prerelease: false,
            assets: Vec::new(),
            version: Version::new(1, 0, 0, false),
            published_at: Utc::now(),
        }
    }

    fn release_with_assets(assets: Vec<Asset>) -> Release {
        Release {
            id: 1,
            tag: "v1.0.0".to_string(),
            name: "v1.0.0".to_string(),
            body: String::new(),
            is_draft: false,
            is_prerelease: false,
            assets,
            version: Version::new(1, 0, 0, false),
            published_at: Utc::now(),
        }
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    fn fixture_path(relative: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(relative)
    }

    fn fixture_string(relative: &str) -> String {
        fs::read_to_string(fixture_path(relative)).expect("read fixture")
    }

    #[test]
    fn parse_checksums_supports_standard_colon_and_bare_formats() {
        let digest = "a".repeat(64);
        let contents = format!(
            "{}  tool.tar.gz\n\
             tool2.tar.gz: {}\n\
             {}\n\
             #comment\n",
            digest, digest, digest
        );

        let entries = ChecksumVerifier::parse_checksums(&contents);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].filename, "tool.tar.gz");
        assert_eq!(entries[1].filename, "tool2.tar.gz");
        assert_eq!(entries[2].filename, "");
    }

    #[test]
    fn parse_checksums_supports_openssl_style_lines() {
        let digest = "b".repeat(64);
        let contents = format!("SHA256(tool.tar.gz)= {digest}");

        let entries = ChecksumVerifier::parse_checksums(&contents);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].filename, "tool.tar.gz");
        assert_eq!(entries[0].digest, digest);
    }

    #[test]
    fn parse_checksums_normalizes_uppercase_prefixed_digest_tokens() {
        let digest = "A".repeat(64);
        let contents = format!("sha256={digest}  tool.tar.gz");

        let entries = ChecksumVerifier::parse_checksums(&contents);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].filename, "tool.tar.gz");
        assert_eq!(entries[0].digest, digest.to_ascii_lowercase());
    }

    #[test]
    fn parse_matrix_checksums_supports_yq_style_manifests() {
        let sha256 = "a".repeat(64);
        let sha512 = "b".repeat(128);
        let contents = format!("tool.tar.gz deadbeef {sha256} ignored {sha512}");
        let order = "CRC-32\nSHA-256\nBLAKE2b-256\nSHA-512\n";

        let entries = ChecksumVerifier::parse_matrix_checksums(&contents, order)
            .expect("parse matrix checksums");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].filename, "tool.tar.gz");
        assert_eq!(entries[0].digest, sha256);
        assert_eq!(entries[1].filename, "tool.tar.gz");
        assert_eq!(entries[1].digest, sha512);
    }

    #[test]
    fn parse_matrix_checksums_requires_supported_hash_order_entries() {
        let err = ChecksumVerifier::parse_matrix_checksums("tool.tar.gz deadbeef", "CRC-32\n")
            .err()
            .expect("matrix manifest without supported hashes should fail");

        assert!(
            err.to_string()
                .contains("does not describe supported hashes")
        );
    }

    #[test]
    fn verify_checksum_validates_sha256_digest() {
        let asset_path = fixture_path("trust/checksums/valid-asset.bin");
        let checksum = fixture_string("trust/checksums/valid-checksums.txt");
        let entry =
            ChecksumVerifier::parse_standard_format(&checksum).expect("parse checksum entry");

        assert!(ChecksumVerifier::verify_checksum(&asset_path, &entry).expect("verify checksum"));
    }

    #[test]
    fn verify_checksum_rejects_sha256_mismatch() {
        let asset_path = fixture_path("trust/checksums/mismatch-asset.bin");
        let checksum = fixture_string("trust/checksums/mismatch-checksums.txt");
        let entry =
            ChecksumVerifier::parse_standard_format(&checksum).expect("parse checksum entry");

        assert!(!ChecksumVerifier::verify_checksum(&asset_path, &entry).expect("verify checksum"));
    }

    #[tokio::test]
    async fn try_verify_file_returns_missing_when_release_has_no_checksum_asset() {
        let root = temp_root("no-checksum");
        fs::create_dir_all(&root).expect("create root");
        let asset_path = root.join("tool.tar.gz");
        fs::write(&asset_path, b"payload").expect("write asset");

        let manager = ProviderManager::new(None, None, None).expect("provider manager");
        let verifier = ChecksumVerifier::new(&manager, &root);
        let mut progress: Option<fn(u64, u64)> = None;

        let verified = verifier
            .try_verify_file(
                &asset_path,
                &empty_release(),
                &Provider::Github,
                &mut progress,
            )
            .await
            .expect("verify without checksum");
        assert!(matches!(verified, ChecksumVerificationResult::Missing));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn find_checksum_asset_prefers_asset_specific_files_then_common_names() {
        let assets = vec![
            Asset::new(
                "https://example.invalid/checksums.txt".to_string(),
                1,
                "checksums.txt".to_string(),
                10,
                Utc::now(),
            ),
            Asset::new(
                "https://example.invalid/tool.tar.gz.sha256".to_string(),
                2,
                "tool.tar.gz.sha256".to_string(),
                10,
                Utc::now(),
            ),
        ];
        let release = release_with_assets(assets);

        let selected = ChecksumVerifier::find_checksum_asset(&release, "tool.tar.gz")
            .expect("must select checksum asset");
        assert_eq!(selected.name, "tool.tar.gz.sha256");
    }

    #[test]
    fn find_checksum_asset_prefers_checksums_bsd_over_generic_checksums() {
        let assets = vec![
            Asset::new(
                "https://example.invalid/checksums".to_string(),
                1,
                "checksums".to_string(),
                10,
                Utc::now(),
            ),
            Asset::new(
                "https://example.invalid/checksums-bsd".to_string(),
                2,
                "checksums-bsd".to_string(),
                10,
                Utc::now(),
            ),
        ];
        let release = release_with_assets(assets);

        let selected = ChecksumVerifier::find_checksum_asset(&release, "tool.tar.gz")
            .expect("must select checksum asset");
        assert_eq!(selected.name, "checksums-bsd");
    }
}
