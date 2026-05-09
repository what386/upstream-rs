use crate::models::provider::{Asset, Release};
use std::path::Path;

pub(crate) fn find_signature_assets<'r>(release: &'r Release, asset_name: &str) -> Vec<&'r Asset> {
    let mut out: Vec<&Asset> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut push_unique = |asset: &'r Asset| {
        let lowered = asset.name.to_ascii_lowercase();
        if seen.iter().any(|n| n == &lowered) {
            return;
        }
        seen.push(lowered);
        out.push(asset);
    };

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
            push_unique(asset);
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
            push_unique(asset);
        }
    }

    for asset in &release.assets {
        if is_signature_filename(&asset.name) {
            push_unique(asset);
        }
    }

    out
}

pub(crate) fn signature_target_name(signature_asset_name: &str) -> Option<&str> {
    let lowered = signature_asset_name.to_ascii_lowercase();
    for suffix in [".cosign.sig", ".minisig", ".sig"] {
        if lowered.ends_with(suffix) {
            let target_len = signature_asset_name.len().saturating_sub(suffix.len());
            let target = &signature_asset_name[..target_len];
            if !target.is_empty() {
                return Some(target);
            }
        }
    }
    None
}

fn is_signature_filename(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    lowered.ends_with(".minisig")
        || lowered.ends_with(".cosign.sig")
        || lowered.ends_with(".sig")
        || lowered.contains("signature")
}
