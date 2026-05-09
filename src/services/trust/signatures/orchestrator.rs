use crate::{
    models::{
        common::enums::Provider,
        provider::{Asset, Release},
    },
    providers::provider_manager::ProviderManager,
};
use anyhow::{Result, anyhow};
use std::{fs, path::Path};

use super::{
    SignatureScheme, SignatureVerificationStatus, TrustedSignatureKeys,
    asset_selector::{find_signature_assets, signature_target_name},
    cosign::verify_cosign_signature,
    minisign::verify_minisign_signature,
};

enum SignatureTarget<'r> {
    InstallAsset,
    ReleaseAsset(&'r Asset),
}

struct ResolvedSignatureTarget {
    name: String,
    path: std::path::PathBuf,
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

    pub async fn try_verify_file<F, H>(
        &self,
        asset_path: &Path,
        release: &Release,
        provider: &Provider,
        trusted_keys: &TrustedSignatureKeys,
        dl_progress: &mut Option<F>,
        message_callback: &mut Option<H>,
    ) -> Result<SignatureVerificationStatus>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
    {
        let asset_filename = asset_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow!("Invalid asset filename"))?;

        let mut saw_targeted_signature = false;
        let mut saw_invalid_signature = false;
        let mut saw_no_trusted_key = false;

        for signature_asset in find_signature_assets(release, asset_filename) {
            let Some(target) = self
                .try_resolve_signature_target(
                    release,
                    signature_asset,
                    asset_filename,
                    asset_path,
                    provider,
                    dl_progress,
                )
                .await?
            else {
                continue;
            };
            saw_targeted_signature = true;

            if let Some(cb) = message_callback.as_mut() {
                cb(&format!(
                    "Checking signature for '{}' using '{}' ...",
                    target.name, signature_asset.name
                ));
            }

            let signature_path = self
                .provider_manager
                .download_asset(signature_asset, provider, self.download_cache, dl_progress)
                .await?;
            let signature_contents = fs::read_to_string(signature_path)?;

            let minisign_status = verify_minisign_signature(
                &target.path,
                &signature_contents,
                &trusted_keys.minisign_public_keys,
            )?;
            if let SignatureVerificationStatus::Verified { key_id, .. } = minisign_status {
                return Ok(SignatureVerificationStatus::Verified {
                    scheme: SignatureScheme::Minisign,
                    key_id,
                    signature_asset: signature_asset.name.clone(),
                });
            }

            let cosign_status = verify_cosign_signature(
                &target.path,
                &signature_contents,
                &trusted_keys.cosign_public_keys,
            )
            .await?;
            if let SignatureVerificationStatus::Verified { key_id, .. } = cosign_status {
                return Ok(SignatureVerificationStatus::Verified {
                    scheme: SignatureScheme::Cosign,
                    key_id,
                    signature_asset: signature_asset.name.clone(),
                });
            }

            let minisign_attempted = !trusted_keys.minisign_public_keys.is_empty();
            let cosign_attempted = !trusted_keys.cosign_public_keys.is_empty();

            saw_no_trusted_key |= (minisign_attempted
                && matches!(
                    minisign_status,
                    SignatureVerificationStatus::NoTrustedKeyMatched
                ))
                || (cosign_attempted
                    && matches!(
                        cosign_status,
                        SignatureVerificationStatus::NoTrustedKeyMatched
                    ));

            saw_invalid_signature |= (minisign_attempted
                && matches!(
                    minisign_status,
                    SignatureVerificationStatus::InvalidSignature
                ))
                || (cosign_attempted
                    && matches!(cosign_status, SignatureVerificationStatus::InvalidSignature));
        }

        if !saw_targeted_signature {
            return Ok(SignatureVerificationStatus::MissingSignature);
        }

        if saw_no_trusted_key {
            return Ok(SignatureVerificationStatus::NoTrustedKeyMatched);
        }

        if saw_invalid_signature {
            return Ok(SignatureVerificationStatus::InvalidSignature);
        }

        Ok(SignatureVerificationStatus::NoTrustedKeyMatched)
    }

    async fn try_resolve_signature_target<F>(
        &self,
        release: &Release,
        signature_asset: &Asset,
        install_asset_name: &str,
        install_asset_path: &Path,
        provider: &Provider,
        dl_progress: &mut Option<F>,
    ) -> Result<Option<ResolvedSignatureTarget>>
    where
        F: FnMut(u64, u64),
    {
        let Some(target_name) = signature_target_name(&signature_asset.name) else {
            return Ok(None);
        };

        match Self::resolve_signature_target_asset(release, target_name, install_asset_name) {
            Some(SignatureTarget::InstallAsset) => Ok(Some(ResolvedSignatureTarget {
                name: install_asset_name.to_string(),
                path: install_asset_path.to_path_buf(),
            })),
            Some(SignatureTarget::ReleaseAsset(target_asset)) => {
                let target_path = self
                    .provider_manager
                    .download_asset(target_asset, provider, self.download_cache, dl_progress)
                    .await?;
                Ok(Some(ResolvedSignatureTarget {
                    name: target_asset.name.clone(),
                    path: target_path,
                }))
            }
            None => Ok(None),
        }
    }

    fn resolve_signature_target_asset<'r>(
        release: &'r Release,
        target_name: &str,
        install_asset_name: &str,
    ) -> Option<SignatureTarget<'r>> {
        if target_name.eq_ignore_ascii_case(install_asset_name) {
            return Some(SignatureTarget::InstallAsset);
        }

        release
            .get_asset_by_name_invariant(target_name)
            .map(SignatureTarget::ReleaseAsset)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SignatureTarget, SignatureVerificationStatus, SignatureVerifier, find_signature_assets,
        signature_target_name, verify_minisign_signature,
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
    fn find_signature_assets_prefers_asset_specific_names() {
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

        let selected = find_signature_assets(&release, "tool.tar.gz");
        assert_eq!(selected[0].name, "tool.tar.gz.minisig");
    }

    #[test]
    fn resolve_signature_target_asset_uses_signature_filename_target() {
        let release = release_with_assets(vec![
            Asset::new(
                "https://example.invalid/chezmoi_2.70.3_linux_amd64.tar.gz".to_string(),
                1,
                "chezmoi_2.70.3_linux_amd64.tar.gz".to_string(),
                10,
                Utc::now(),
            ),
            Asset::new(
                "https://example.invalid/chezmoi_2.70.3_checksums.txt".to_string(),
                2,
                "chezmoi_2.70.3_checksums.txt".to_string(),
                10,
                Utc::now(),
            ),
        ]);

        let target_name =
            signature_target_name("chezmoi_2.70.3_checksums.txt.sig").expect("target name");
        let selected = SignatureVerifier::resolve_signature_target_asset(
            &release,
            target_name,
            "chezmoi_2.70.3_linux_amd64.tar.gz",
        );

        assert!(matches!(
            selected,
            Some(SignatureTarget::ReleaseAsset(asset))
                if asset.name == "chezmoi_2.70.3_checksums.txt"
        ));
    }

    #[test]
    fn resolve_signature_target_asset_uses_install_asset_for_binary_signature() {
        let release = release_with_assets(vec![]);
        let target_name = signature_target_name("tool.tar.gz.sig").expect("target name");
        let selected =
            SignatureVerifier::resolve_signature_target_asset(&release, target_name, "tool.tar.gz");

        assert!(matches!(selected, Some(SignatureTarget::InstallAsset)));
    }

    #[test]
    fn resolve_signature_target_asset_ignores_unrelated_signature_assets() {
        let release = release_with_assets(vec![]);
        let target_name = signature_target_name("other-tool.tar.gz.sig").expect("target name");
        let selected =
            SignatureVerifier::resolve_signature_target_asset(&release, target_name, "tool.tar.gz");

        assert!(selected.is_none());
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
        let status = verify_minisign_signature(&asset_path, &signature, &keys).expect("status");
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
        let mut messages: Option<fn(&str)> = None;

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
                &mut messages,
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
