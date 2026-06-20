use super::{MinisignPublicKey, SignatureScheme, SignatureVerificationStatus};
use anyhow::{Result, anyhow};
use minisign_verify::{PublicKey, Signature};
use std::{fs, path::Path};

pub fn verify_minisign_signature(
    asset_path: &Path,
    signature_contents: &str,
    trusted_keys: &[MinisignPublicKey],
) -> Result<SignatureVerificationStatus> {
    if trusted_keys.is_empty() {
        return Ok(SignatureVerificationStatus::NoTrustedKeyMatched);
    }

    let Ok(signature) = Signature::decode(signature_contents) else {
        return Ok(SignatureVerificationStatus::InvalidSignature);
    };

    let file_bytes = fs::read(asset_path).map_err(|e| {
        anyhow!(
            "Failed to read asset '{}' for signature verification: {}",
            asset_path.display(),
            e
        )
    })?;

    for key in trusted_keys {
        let Ok(public_key) = PublicKey::from_base64(&key.key) else {
            continue;
        };

        if public_key.verify(&file_bytes, &signature, false).is_ok() {
            return Ok(SignatureVerificationStatus::Verified {
                scheme: SignatureScheme::Minisign,
                key_id: key.id.clone(),
                signature_asset: String::new(),
            });
        }
    }

    Ok(SignatureVerificationStatus::NoTrustedKeyMatched)
}
