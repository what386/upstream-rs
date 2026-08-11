mod asset_selector;
mod cosign;
mod minisign;
mod orchestrator;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::Path};

pub use orchestrator::SignatureVerifier;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinisignPublicKey {
    pub id: Option<String>,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CosignPublicKey {
    pub id: Option<String>,
    pub key: String,
}

#[derive(Debug, Clone, Default)]
pub struct TrustedSignatureKeys {
    pub minisign_public_keys: Vec<MinisignPublicKey>,
    pub cosign_public_keys: Vec<CosignPublicKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureScheme {
    Minisign,
    Cosign,
}

pub enum SignatureVerificationStatus {
    NotChecked,
    Verified {
        scheme: SignatureScheme,
        key_id: Option<String>,
        signature_asset: String,
    },
    MissingSignature,
    InvalidSignature,
    NoTrustedKeyMatched,
}

/// Reads a file fully into memory in fixed-size chunks, checking for
/// cancellation between reads so verification of large assets stays
/// responsive to CTRL-C / cancellation requests.
pub(crate) fn read_asset_bytes(asset_path: &Path) -> Result<Vec<u8>> {
    let mut file = fs::File::open(asset_path).with_context(|| {
        format!(
            "Failed to read asset '{}' for signature verification",
            asset_path.display()
        )
    })?;
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        crate::application::cancellation::check()?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    Ok(bytes)
}
