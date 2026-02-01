use crate::{
    models::{common::enums::Provider, provider::Release},
    providers::provider_manager::ProviderManager,
};
use anyhow::{Result, anyhow};
use std::{
    fs,
    path::{Path, PathBuf},
};

enum HashAlgo {
    Sha256,
    Sha512,
}

struct ChecksumEntry {
    algo: HashAlgo,
    filename: String,
    digest: String,
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
    ) -> Result<bool>
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
            None => return Ok(false), // No checksum available, that's ok
        };

        // Read and parse the checksum file
        let contents = fs::read_to_string(&checksum_path)?;
        let entries = Self::parse_checksums(&contents);

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

        // Verify the checksum
        Self::verify_checksum(asset_path, checksum_entry)
    }

    async fn try_download_checksum<F>(
        &self,
        release: &Release,
        asset_name: &str,
        provider: &Provider,
        dl_progress: &mut Option<F>,
    ) -> Result<Option<PathBuf>>
    where
        F: FnMut(u64, u64),
    {
        let checksum_asset = release
            .get_asset_by_name_invariant("checksums.txt")
            .or_else(|| release.get_asset_by_name_invariant("sha256sums.txt"))
            .or_else(|| release.get_asset_by_name_invariant("sha256sum.txt"))
            .or_else(|| {
                let name = format!("{asset_name}.sha256");
                release.get_asset_by_name_invariant(&name)
            });

        let Some(asset) = checksum_asset else {
            return Ok(None); // no checksum advertised
        };

        // If this fails, it's a real error
        let path = self
            .provider_manager
            .download_asset(asset, provider, self.download_cache, dl_progress)
            .await?;

        Ok(Some(path))
    }

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
            } else if let Some(entry) = Self::parse_bare_hash(line) {
                entries.push(entry);
            }
        }

        entries
    }

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

        // Determine algorithm based on digest length
        let algo = match digest.len() {
            64 => HashAlgo::Sha256,
            128 => HashAlgo::Sha512,
            _ => return None, // Unknown hash length
        };

        Some(ChecksumEntry {
            algo,
            filename: filename.to_string(),
            digest: digest.to_lowercase(),
        })
    }

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

        // Determine algorithm based on digest length
        let algo = match digest.len() {
            64 => HashAlgo::Sha256,
            128 => HashAlgo::Sha512,
            _ => return None,
        };

        Some(ChecksumEntry {
            algo,
            filename: filename.to_string(),
            digest: digest.to_lowercase(),
        })
    }

    fn parse_bare_hash(line: &str) -> Option<ChecksumEntry> {
        // Handle bare hash format (just the digest, no filename)
        let digest = line.trim();

        if digest.is_empty() {
            return None;
        }

        // Determine algorithm based on digest length
        let algo = match digest.len() {
            64 => HashAlgo::Sha256,
            128 => HashAlgo::Sha512,
            _ => return None, // Unknown hash length
        };

        // Use empty filename - indicates bare hash file
        Some(ChecksumEntry {
            algo,
            filename: String::new(),
            digest: digest.to_lowercase(),
        })
    }

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
                format!("{:x}", hasher.finalize())
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
                format!("{:x}", hasher.finalize())
            }
        };

        Ok(computed_digest.to_lowercase() == checksum.digest.to_lowercase())
    }
}
