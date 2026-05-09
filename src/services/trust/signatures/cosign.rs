use super::{CosignPublicKey, SignatureScheme, SignatureVerificationStatus};
use anyhow::Result;
use sigstore_verification::verify_cosign_signature_with_key;
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) async fn verify_cosign_signature(
    asset_path: &Path,
    signature_contents: &str,
    trusted_keys: &[CosignPublicKey],
) -> Result<SignatureVerificationStatus> {
    if trusted_keys.is_empty() {
        return Ok(SignatureVerificationStatus::NoTrustedKeyMatched);
    }

    for key in trusted_keys {
        let temp_root = std::env::temp_dir().join(format!(
            "upstream-cosign-verify-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&temp_root)?;
        let signature_path = temp_root.join("signature.sig");
        let key_path = temp_root.join("key.pub");
        fs::write(&signature_path, signature_contents)?;
        fs::write(&key_path, &key.key)?;

        let verified = match verify_cosign_signature_with_key(asset_path, &signature_path, &key_path).await {
            Ok(v) => v,
            Err(_) => {
                let _ = fs::remove_dir_all(&temp_root);
                continue;
            }
        };

        let _ = fs::remove_dir_all(&temp_root);
        if verified {
            return Ok(SignatureVerificationStatus::Verified {
                scheme: SignatureScheme::Cosign,
                key_id: key.id.clone(),
                signature_asset: String::new(),
            });
        }
    }

    Ok(SignatureVerificationStatus::NoTrustedKeyMatched)
}
