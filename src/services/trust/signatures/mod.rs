mod asset_selector;
mod cosign;
mod minisign;
mod orchestrator;

pub use orchestrator::SignatureVerifier;

#[derive(Debug, Clone)]
pub struct MinisignPublicKey {
    pub id: Option<String>,
    pub key: String,
}

#[derive(Debug, Clone)]
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
    Verified {
        scheme: SignatureScheme,
        key_id: Option<String>,
        signature_asset: String,
    },
    MissingSignature,
    InvalidSignature,
    NoTrustedKeyMatched,
}
