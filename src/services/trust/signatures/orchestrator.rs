use crate::{
    models::{
        common::enums::Provider,
        provider::Release,
    },
    providers::provider_manager::ProviderManager,
};
use anyhow::{Result, anyhow};
use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{
    TrustedSignatureKeys,
    asset_selector::find_signature_asset,
    cosign::verify_cosign_signature,
    minisign::verify_minisign_signature,
    SignatureScheme,
    SignatureVerificationStatus,
};

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
        trusted_keys: &TrustedSignatureKeys,
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

        let minisign_status = verify_minisign_signature(
            asset_path,
            &signature_contents,
            &trusted_keys.minisign_public_keys,
        )?;
        if let SignatureVerificationStatus::Verified { key_id, .. } = minisign_status {
            return Ok(SignatureVerificationStatus::Verified {
                scheme: SignatureScheme::Minisign,
                key_id,
                signature_asset: signature_asset.name,
            });
        }

        let cosign_status = verify_cosign_signature(
            asset_path,
            &signature_contents,
            &trusted_keys.cosign_public_keys,
        )
        .await?;
        if let SignatureVerificationStatus::Verified { key_id, .. } = cosign_status {
            return Ok(SignatureVerificationStatus::Verified {
                scheme: SignatureScheme::Cosign,
                key_id,
                signature_asset: signature_asset.name,
            });
        }

        if matches!(minisign_status, SignatureVerificationStatus::NoTrustedKeyMatched)
            || matches!(cosign_status, SignatureVerificationStatus::NoTrustedKeyMatched)
        {
            return Ok(SignatureVerificationStatus::NoTrustedKeyMatched);
        }

        if matches!(minisign_status, SignatureVerificationStatus::InvalidSignature)
            || matches!(cosign_status, SignatureVerificationStatus::InvalidSignature)
        {
            return Ok(SignatureVerificationStatus::InvalidSignature);
        }

        Ok(SignatureVerificationStatus::NoTrustedKeyMatched)
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
        let signature_asset = find_signature_asset(release, asset_name);
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
}

#[cfg(test)]
mod tests {
    use super::{
        SignatureVerificationStatus,
        SignatureVerifier,
        find_signature_asset,
        verify_minisign_signature,
    };
    use crate::models::common::{enums::Provider, version::Version};
    use crate::models::provider::{Asset, Release};
    use crate::services::trust::{MinisignPublicKey, TrustedSignatureKeys};
    use chrono::Utc;
    use serde::Deserialize;
    use std::{fs, path::PathBuf, time::SystemTime};

    #[derive(Deserialize)]
    struct MinisignKeyFixture {
        id: Option<String>,
        key: String,
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

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-signature-test-{name}-{nanos}"))
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

    fn trusted_key_fixtures(relative: &str) -> Vec<MinisignPublicKey> {
        serde_json::from_str::<Vec<MinisignKeyFixture>>(&fixture_string(relative))
            .expect("parse minisign key fixture")
            .into_iter()
            .map(|key| MinisignPublicKey {
                id: key.id,
                key: key.key,
            })
            .collect()
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

        let selected = find_signature_asset(&release, "tool.tar.gz")
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
        let signature = fixture_string("trust/signatures/malformed.minisig");
        let status = verify_minisign_signature(&asset_path, &signature, &keys)
            .expect("invalid signature must return status");
        assert!(matches!(
            status,
            SignatureVerificationStatus::InvalidSignature
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn verify_minisign_signature_returns_verified_with_matching_key() {
        let root = temp_root("verified");
        fs::create_dir_all(&root).expect("create root");
        let asset_path = root.join("tool.tar.gz");
        fs::copy(
            fixture_path("trust/signatures/valid-asset.bin"),
            &asset_path,
        )
        .expect("copy asset fixture");

        let signature = fixture_string("trust/signatures/valid-asset.bin.minisig");
        let keys = trusted_key_fixtures("trust/signatures/valid-keys.json");
        let status = verify_minisign_signature(&asset_path, &signature, &keys)
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
                &TrustedSignatureKeys {
                    minisign_public_keys: vec![MinisignPublicKey {
                        id: Some("k1".to_string()),
                        key: "RWQx2345invalidbase64".to_string(),
                    }],
                    cosign_public_keys: Vec::new(),
                },
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
