use crate::{
    models::{
        common::enums::{Provider, TrustMode},
        provider::Release,
    },
    providers::provider_manager::ProviderManager,
};
use anyhow::{Result, anyhow};
use std::path::Path;

use super::{
    checksum_verifier::ChecksumVerifier,
    signatures::{SignatureVerificationStatus, SignatureVerifier, TrustedSignatureKeys},
};

pub enum ChecksumVerificationStatus {
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

    pub async fn verify_file<F>(
        &self,
        asset_path: &Path,
        release: &Release,
        provider: &Provider,
        dl_progress: &mut Option<F>,
    ) -> Result<TrustVerificationStatus>
    where
        F: FnMut(u64, u64),
    {
        if self.trust_mode == TrustMode::None {
            return Ok(TrustVerificationStatus::Skipped);
        }

        let checksum_status = match self
            .checksum_verifier
            .try_verify_file(asset_path, release, provider, dl_progress)
            .await?
        {
            true => ChecksumVerificationStatus::Verified,
            false => ChecksumVerificationStatus::Missing,
        };

        let signature_status = self
            .signature_verifier
            .try_verify_file(
                asset_path,
                release,
                provider,
                self.trusted_keys,
                dl_progress,
            )
            .await?;

        self.enforce_policy(&checksum_status, &signature_status)?;

        Ok(TrustVerificationStatus::Verified {
            checksum: checksum_status,
            signature: signature_status,
        })
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
                if matches!(checksum, ChecksumVerificationStatus::Missing) {
                    return Err(anyhow!(
                        "Checksum is required but no checksum asset was found"
                    ));
                }
                self.enforce_signature_attempt_result(signature)?;
                Ok(())
            }
            TrustMode::Signature => {
                if matches!(signature, SignatureVerificationStatus::MissingSignature) {
                    return Err(anyhow!(
                        "Signature is required but no signature asset was found"
                    ));
                }
                self.enforce_signature_attempt_result(signature)?;
                Ok(())
            }
            TrustMode::All => {
                if matches!(checksum, ChecksumVerificationStatus::Missing) {
                    return Err(anyhow!(
                        "Checksum is required but no checksum asset was found"
                    ));
                }
                if matches!(signature, SignatureVerificationStatus::MissingSignature) {
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
