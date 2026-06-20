use super::{CosignPublicKey, SignatureScheme, SignatureVerificationStatus};
use anyhow::{Context, Result};
use sigstore::crypto::{CosignVerificationKey, Signature};
use std::{fs, path::Path};

pub async fn verify_cosign_signature(
    asset_path: &Path,
    signature_contents: &str,
    trusted_keys: &[CosignPublicKey],
) -> Result<SignatureVerificationStatus> {
    if trusted_keys.is_empty() {
        return Ok(SignatureVerificationStatus::NoTrustedKeyMatched);
    }

    let blob = fs::read(asset_path).with_context(|| {
        format!(
            "Failed to read asset '{}' for cosign signature verification",
            asset_path.display()
        )
    })?;
    let signature = signature_contents.trim();
    let mut saw_valid_key = false;
    let mut saw_parse_error = false;

    for key in trusted_keys {
        let Ok(verification_key) = CosignVerificationKey::try_from_pem(key.key.as_bytes()) else {
            saw_parse_error = true;
            continue;
        };
        saw_valid_key = true;

        if verification_key
            .verify_signature(Signature::Base64Encoded(signature.as_bytes()), &blob)
            .is_ok()
        {
            return Ok(SignatureVerificationStatus::Verified {
                scheme: SignatureScheme::Cosign,
                key_id: key.id.clone(),
                signature_asset: String::new(),
            });
        }
    }

    if saw_valid_key {
        Ok(SignatureVerificationStatus::NoTrustedKeyMatched)
    } else if saw_parse_error {
        Ok(SignatureVerificationStatus::InvalidSignature)
    } else {
        Ok(SignatureVerificationStatus::NoTrustedKeyMatched)
    }
}

#[cfg(test)]
mod tests {
    use super::verify_cosign_signature;
    use crate::services::trust::{CosignPublicKey, SignatureScheme, SignatureVerificationStatus};
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
    use p256::ecdsa::{DerSignature, SigningKey, signature::Signer};
    use p256::pkcs8::{EncodePublicKey, LineEnding};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-cosign-test-{name}-{nanos}"))
    }

    #[tokio::test]
    async fn verify_cosign_signature_verifies_blob_bytes_with_pem_key() {
        let root = temp_root("blob-bytes");
        fs::create_dir_all(&root).expect("create root");
        let artifact_path = root.join("checksums.txt");
        fs::write(&artifact_path, b"payload bytes").expect("write artifact");

        let signing_key = SigningKey::from_bytes((&[7_u8; 32]).into()).expect("create signing key");
        let public_key = signing_key
            .verifying_key()
            .to_public_key_pem(LineEnding::LF)
            .expect("encode public key");
        let signature: DerSignature = signing_key.sign(b"payload bytes");
        let signature = BASE64_STANDARD.encode(signature.to_bytes());

        let status = verify_cosign_signature(
            &artifact_path,
            &signature,
            &[CosignPublicKey {
                id: Some("fixture".to_string()),
                key: public_key,
            }],
        )
        .await
        .expect("verify cosign signature");

        assert!(matches!(
            status,
            SignatureVerificationStatus::Verified {
                scheme: SignatureScheme::Cosign,
                key_id: Some(ref id),
                ..
            } if id == "fixture"
        ));

        let _ = fs::remove_dir_all(root);
    }
}
