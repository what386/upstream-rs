use crate::{
    models::{
        common::enums::Provider,
        provider::{Asset, Release},
    },
    providers::provider_manager::ProviderManager,
};
use anyhow::{Result, anyhow};
use minisign_verify::{PublicKey, Signature};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct MinisignPublicKey {
    pub id: Option<String>,
    pub key: String,
}

pub enum SignatureVerificationStatus {
    Verified {
        key_id: Option<String>,
        signature_asset: String,
    },
    MissingSignature,
    InvalidSignature,
    NoTrustedKeyMatched,
}

struct DownloadedSignatureAsset {
    name: String,
    path: PathBuf,
}

pub struct SignatureVerifier<'a> {
    provider_manager: &'a ProviderManager,
    download_cache: &'a Path,
}

impl<'a> SignatureVerifier<'a> {
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
        trusted_keys: &[MinisignPublicKey],
        dl_progress: &mut Option<F>,
    ) -> Result<SignatureVerificationStatus>
    where
        F: FnMut(u64, u64),
    {
        let asset_filename = asset_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow!("Invalid asset filename"))?;

        let signature_asset = match self
            .try_download_signature(release, asset_filename, provider, dl_progress)
            .await?
        {
            Some(path) => path,
            None => return Ok(SignatureVerificationStatus::MissingSignature),
        };

        let signature_contents = fs::read_to_string(signature_asset.path)?;
        let status =
            Self::verify_minisign_signature(asset_path, &signature_contents, trusted_keys)?;

        Ok(match status {
            SignatureVerificationStatus::Verified { key_id, .. } => {
                SignatureVerificationStatus::Verified {
                    key_id,
                    signature_asset: signature_asset.name,
                }
            }
            SignatureVerificationStatus::InvalidSignature => {
                SignatureVerificationStatus::InvalidSignature
            }
            SignatureVerificationStatus::NoTrustedKeyMatched => {
                SignatureVerificationStatus::NoTrustedKeyMatched
            }
            SignatureVerificationStatus::MissingSignature => {
                SignatureVerificationStatus::MissingSignature
            }
        })
    }

    async fn try_download_signature<F>(
        &self,
        release: &Release,
        asset_name: &str,
        provider: &Provider,
        dl_progress: &mut Option<F>,
    ) -> Result<Option<DownloadedSignatureAsset>>
    where
        F: FnMut(u64, u64),
    {
        let signature_asset = Self::find_signature_asset(release, asset_name);
        let Some(asset) = signature_asset else {
            return Ok(None);
        };

        let path = self
            .provider_manager
            .download_asset(asset, provider, self.download_cache, dl_progress)
            .await?;

        Ok(Some(DownloadedSignatureAsset {
            name: asset.name.clone(),
            path,
        }))
    }

    fn find_signature_asset<'r>(release: &'r Release, asset_name: &str) -> Option<&'r Asset> {
        let basename = Path::new(asset_name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(asset_name);

        let specific_candidates = [
            format!("{asset_name}.minisig"),
            format!("{basename}.minisig"),
            format!("{asset_name}.sig"),
            format!("{basename}.sig"),
        ];

        for candidate in &specific_candidates {
            if let Some(asset) = release.get_asset_by_name_invariant(candidate) {
                return Some(asset);
            }
        }

        const COMMON_NAMES: &[&str] = &[
            "minisig",
            "signature.minisig",
            "signature.sig",
            "signatures.txt",
        ];
        for name in COMMON_NAMES {
            if let Some(asset) = release.get_asset_by_name_invariant(name) {
                return Some(asset);
            }
        }

        release
            .assets
            .iter()
            .find(|asset| Self::is_signature_filename(&asset.name))
    }

    fn is_signature_filename(name: &str) -> bool {
        let lowered = name.to_ascii_lowercase();
        lowered.ends_with(".minisig") || lowered.ends_with(".sig") || lowered.contains("signature")
    }

    fn verify_minisign_signature(
        asset_path: &Path,
        signature_contents: &str,
        trusted_keys: &[MinisignPublicKey],
    ) -> Result<SignatureVerificationStatus> {
        if trusted_keys.is_empty() {
            return Ok(SignatureVerificationStatus::NoTrustedKeyMatched);
        }

        let signature = match Signature::decode(signature_contents) {
            Ok(sig) => sig,
            Err(_) => return Ok(SignatureVerificationStatus::InvalidSignature),
        };

        let file_bytes = fs::read(asset_path).map_err(|e| {
            anyhow!(
                "Failed to read asset '{}' for signature verification: {}",
                asset_path.display(),
                e
            )
        })?;

        for key in trusted_keys {
            let public_key = match PublicKey::from_base64(&key.key) {
                Ok(k) => k,
                Err(_) => continue,
            };

            if public_key.verify(&file_bytes, &signature, false).is_ok() {
                return Ok(SignatureVerificationStatus::Verified {
                    key_id: key.id.clone(),
                    signature_asset: String::new(),
                });
            }
        }

        Ok(SignatureVerificationStatus::NoTrustedKeyMatched)
    }
}

#[cfg(test)]
mod tests {
    use super::{MinisignPublicKey, SignatureVerificationStatus, SignatureVerifier};
    use crate::models::common::{enums::Provider, version::Version};
    use crate::models::provider::{Asset, Release};
    use chrono::Utc;
    use std::{fs, path::PathBuf, time::SystemTime};

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

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-signature-test-{name}-{nanos}"))
    }

    #[test]
    fn find_signature_asset_prefers_asset_specific_names() {
        let assets = vec![
            Asset::new(
                "https://example.invalid/signature.minisig".to_string(),
                1,
                "signature.minisig".to_string(),
                10,
                Utc::now(),
            ),
            Asset::new(
                "https://example.invalid/tool.tar.gz.minisig".to_string(),
                2,
                "tool.tar.gz.minisig".to_string(),
                10,
                Utc::now(),
            ),
        ];
        let release = release_with_assets(assets);

        let selected = SignatureVerifier::find_signature_asset(&release, "tool.tar.gz")
            .expect("must select signature asset");
        assert_eq!(selected.name, "tool.tar.gz.minisig");
    }

    #[test]
    fn verify_minisign_signature_returns_invalid_for_malformed_signature() {
        let root = temp_root("invalid-signature");
        fs::create_dir_all(&root).expect("create root");
        let asset_path = root.join("tool.tar.gz");
        fs::write(&asset_path, b"payload").expect("write asset");

        let keys = vec![MinisignPublicKey {
            id: Some("k1".to_string()),
            key: "RWQx2345invalidbase64".to_string(),
        }];
        let status =
            SignatureVerifier::verify_minisign_signature(&asset_path, "not-minisign", &keys)
                .expect("invalid signature must return status");
        assert!(matches!(
            status,
            SignatureVerificationStatus::InvalidSignature
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn verify_minisign_signature_returns_no_matching_key() {
        let root = temp_root("no-key-match");
        fs::create_dir_all(&root).expect("create root");
        let asset_path = root.join("tool.tar.gz");
        fs::write(&asset_path, b"test").expect("write asset");

        let status = SignatureVerifier::verify_minisign_signature(
            &asset_path,
            "untrusted comment: signature from minisign secret key\n\
RUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\n\
trusted comment: timestamp:1633700835\tfile:test\tprehashed\n\
wLMDjy9FLAuxZ3q4NlEvkgtyhrr0gtTu6KC4KBJdITbbOeAi1zBIYo0v4iTgt8jJpIidRJnp94ABQkJAgAooBQ==",
            &[MinisignPublicKey {
                id: Some("k1".to_string()),
                key: "RWQx2345invalidbase64".to_string(),
            }],
        )
        .expect("status");
        assert!(matches!(
            status,
            SignatureVerificationStatus::NoTrustedKeyMatched
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn verify_minisign_signature_returns_verified_with_matching_key() {
        let root = temp_root("verified");
        fs::create_dir_all(&root).expect("create root");
        let asset_path = root.join("tool.tar.gz");
        fs::write(&asset_path, b"test").expect("write asset");

        let status = SignatureVerifier::verify_minisign_signature(
            &asset_path,
            "untrusted comment: signature from minisign secret key\n\
RUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\n\
trusted comment: timestamp:1633700835\tfile:test\tprehashed\n\
wLMDjy9FLAuxZ3q4NlEvkgtyhrr0gtTu6KC4KBJdITbbOeAi1zBIYo0v4iTgt8jJpIidRJnp94ABQkJAgAooBQ==",
            &[MinisignPublicKey {
                id: Some("k-good".to_string()),
                key: "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3".to_string(),
            }],
        )
        .expect("status");
        assert!(matches!(
            status,
            SignatureVerificationStatus::Verified {
                key_id: Some(ref id),
                ..
            } if id == "k-good"
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn try_verify_file_returns_missing_signature_when_release_has_no_signature_asset() {
        let root = temp_root("missing-signature");
        fs::create_dir_all(&root).expect("create root");
        let asset_path = root.join("tool.tar.gz");
        fs::write(&asset_path, b"payload").expect("write asset");

        let manager =
            crate::providers::provider_manager::ProviderManager::new(None, None, None).expect("pm");
        let verifier = SignatureVerifier::new(&manager, &root);
        let mut progress: Option<fn(u64, u64)> = None;

        let status = verifier
            .try_verify_file(
                &asset_path,
                &release_with_assets(vec![]),
                &Provider::Github,
                &[MinisignPublicKey {
                    id: Some("k1".to_string()),
                    key: "RWQx2345invalidbase64".to_string(),
                }],
                &mut progress,
            )
            .await
            .expect("status");
        assert!(matches!(
            status,
            SignatureVerificationStatus::MissingSignature
        ));

        let _ = fs::remove_dir_all(root);
    }
}
