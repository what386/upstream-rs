use crate::models::provider::{Asset, Release};
use std::path::Path;

pub(crate) fn find_signature_asset<'r>(release: &'r Release, asset_name: &str) -> Option<&'r Asset> {
    let basename = Path::new(asset_name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(asset_name);

    let specific_candidates = [
        format!("{asset_name}.minisig"),
        format!("{basename}.minisig"),
        format!("{asset_name}.cosign.sig"),
        format!("{basename}.cosign.sig"),
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
        "cosign.sig",
        "signature.cosign.sig",
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
        .find(|asset| is_signature_filename(&asset.name))
}

fn is_signature_filename(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    lowered.ends_with(".minisig")
        || lowered.ends_with(".cosign.sig")
        || lowered.ends_with(".sig")
        || lowered.contains("signature")
}
