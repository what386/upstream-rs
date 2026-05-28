use crate::{
    models::{
        common::enums::{Provider, TrustMode},
        provider::Release,
    },
    providers::provider_manager::ProviderManager,
    services::packaging::{PackagePhase, PackageProgressEvent},
};
use anyhow::{Result, anyhow};
use std::path::Path;

use super::{
    checksum_verifier::{ChecksumVerificationResult, ChecksumVerifier},
    signatures::{SignatureVerificationStatus, SignatureVerifier, TrustedSignatureKeys},
};

pub enum ChecksumVerificationStatus {
    NotChecked,
    Verified,
    Missing,
}

pub enum TrustVerificationStatus {
    Skipped,
    Verified {
        checksum: ChecksumVerificationStatus,
        signature: SignatureVerificationStatus,
    },
}

pub struct TrustVerifier<'a> {
    checksum_verifier: ChecksumVerifier<'a>,
    signature_verifier: SignatureVerifier<'a>,
    trust_mode: TrustMode,
    trusted_keys: &'a TrustedSignatureKeys,
}

impl<'a> TrustVerifier<'a> {
    pub fn new(
        provider_manager: &'a ProviderManager,
        download_cache: &'a Path,
        trust_mode: TrustMode,
        trusted_keys: &'a TrustedSignatureKeys,
    ) -> Self {
        Self {
            checksum_verifier: ChecksumVerifier::new(provider_manager, download_cache),
            signature_verifier: SignatureVerifier::new(provider_manager, download_cache),
            trust_mode,
            trusted_keys,
        }
    }

    pub async fn verify_file<F, H, P>(
        &self,
        asset_path: &Path,
        release: &Release,
        provider: &Provider,
        dl_progress: &mut Option<F>,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<TrustVerificationStatus>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        if self.trust_mode == TrustMode::None {
            return Ok(TrustVerificationStatus::Skipped);
        }

        let checksum_status = if self.should_verify_checksum() {
            let asset_filename = asset_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("downloaded asset");
            let _ = asset_filename;
            if let Some(cb) = progress_callback.as_mut() {
                cb(PackageProgressEvent::Phase(
                    PackagePhase::ChecksummingPackage,
                ));
            }

            let checksum_result = self
                .checksum_verifier
                .try_verify_file(asset_path, release, provider, dl_progress)
                .await?;
            match checksum_result {
                ChecksumVerificationResult::Verified(info) => {
                    let _ = info;
                    ChecksumVerificationStatus::Verified
                }
                ChecksumVerificationResult::Missing => ChecksumVerificationStatus::Missing,
            }
        } else {
            ChecksumVerificationStatus::NotChecked
        };

        let signature_status = if self.should_verify_signature() {
            self.signature_verifier
                .try_verify_file(
                    asset_path,
                    release,
                    provider,
                    self.trusted_keys,
                    dl_progress,
                    message_callback,
                    progress_callback,
                )
                .await?
        } else {
            SignatureVerificationStatus::NotChecked
        };

        self.enforce_policy(&checksum_status, &signature_status)?;

        Ok(TrustVerificationStatus::Verified {
            checksum: checksum_status,
            signature: signature_status,
        })
    }

    fn should_verify_checksum(&self) -> bool {
        matches!(
            self.trust_mode,
            TrustMode::BestEffort | TrustMode::Checksum | TrustMode::All
        )
    }

    fn should_verify_signature(&self) -> bool {
        matches!(
            self.trust_mode,
            TrustMode::BestEffort | TrustMode::Signature | TrustMode::All
        )
    }

    fn enforce_policy(
        &self,
        checksum: &ChecksumVerificationStatus,
        signature: &SignatureVerificationStatus,
    ) -> Result<()> {
        match self.trust_mode {
            TrustMode::None => Ok(()),
            TrustMode::BestEffort => {
                self.enforce_signature_attempt_result(signature)?;
                Ok(())
            }
            TrustMode::Checksum => {
                if !matches!(checksum, ChecksumVerificationStatus::Verified) {
                    return Err(anyhow!(
                        "Checksum is required but no checksum asset was found"
                    ));
                }
                Ok(())
            }
            TrustMode::Signature => {
                if matches!(
                    signature,
                    SignatureVerificationStatus::MissingSignature
                        | SignatureVerificationStatus::NotChecked
                ) {
                    return Err(anyhow!(
                        "Signature is required but no signature asset was found"
                    ));
                }
                self.enforce_signature_attempt_result(signature)?;
                Ok(())
            }
            TrustMode::All => {
                if !matches!(checksum, ChecksumVerificationStatus::Verified) {
                    return Err(anyhow!(
                        "Checksum is required but no checksum asset was found"
                    ));
                }
                if matches!(
                    signature,
                    SignatureVerificationStatus::MissingSignature
                        | SignatureVerificationStatus::NotChecked
                ) {
                    return Err(anyhow!(
                        "Signature is required but no signature asset was found"
                    ));
                }
                self.enforce_signature_attempt_result(signature)?;
                Ok(())
            }
        }
    }

    fn enforce_signature_attempt_result(
        &self,
        signature: &SignatureVerificationStatus,
    ) -> Result<()> {
        match signature {
            SignatureVerificationStatus::Verified { .. }
            | SignatureVerificationStatus::NotChecked
            | SignatureVerificationStatus::MissingSignature => Ok(()),
            SignatureVerificationStatus::InvalidSignature => Err(anyhow!(
                "Signature verification failed: signature is invalid"
            )),
            SignatureVerificationStatus::NoTrustedKeyMatched => Err(anyhow!(
                "Signature verification failed: no configured trusted key matched"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TrustVerifier;
    use crate::{
        models::common::enums::TrustMode, providers::provider_manager::ProviderManager,
        services::trust::TrustedSignatureKeys,
    };
    use std::path::Path;

    fn with_verifier(mode: TrustMode, assert: impl FnOnce(TrustVerifier<'_>)) {
        let provider_manager = ProviderManager::new(None, None, None).expect("provider manager");
        let trusted_keys = TrustedSignatureKeys::default();
        assert(TrustVerifier::new(
            &provider_manager,
            Path::new("/tmp"),
            mode,
            &trusted_keys,
        ));
    }

    #[test]
    fn checksum_mode_only_attempts_checksum_verification() {
        with_verifier(TrustMode::Checksum, |verifier| {
            assert!(verifier.should_verify_checksum());
            assert!(!verifier.should_verify_signature());
        });
    }

    #[test]
    fn signature_mode_only_attempts_signature_verification() {
        with_verifier(TrustMode::Signature, |verifier| {
            assert!(!verifier.should_verify_checksum());
            assert!(verifier.should_verify_signature());
        });
    }

    #[test]
    fn all_and_best_effort_attempt_both_verifiers() {
        with_verifier(TrustMode::All, |verifier| {
            assert!(verifier.should_verify_checksum());
            assert!(verifier.should_verify_signature());
        });
        with_verifier(TrustMode::BestEffort, |verifier| {
            assert!(verifier.should_verify_checksum());
            assert!(verifier.should_verify_signature());
        });
    }
}
