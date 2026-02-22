use super::ChecksumVerifier;
use crate::models::common::enums::Provider;
use crate::models::common::version::Version;
use crate::models::provider::{Asset, Release};
use crate::providers::provider_manager::ProviderManager;
use chrono::Utc;
use sha2::Digest;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("upstream-checksum-test-{name}-{nanos}"))
}

fn empty_release() -> Release {
    Release {
        id: 1,
        tag: "v1.0.0".to_string(),
        name: "v1.0.0".to_string(),
        body: String::new(),
        is_draft: false,
        is_prerelease: false,
        assets: Vec::new(),
        version: Version::new(1, 0, 0, false),
        published_at: Utc::now(),
    }
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

fn cleanup(path: &PathBuf) -> io::Result<()> {
    fs::remove_dir_all(path)
}

#[test]
fn parse_checksums_supports_standard_colon_and_bare_formats() {
    let digest = "a".repeat(64);
    let contents = format!(
        "{}  tool.tar.gz\n\
         tool2.tar.gz: {}\n\
         {}\n\
         #comment\n",
        digest, digest, digest
    );

    let entries = ChecksumVerifier::parse_checksums(&contents);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].filename, "tool.tar.gz");
    assert_eq!(entries[1].filename, "tool2.tar.gz");
    assert_eq!(entries[2].filename, "");
}

#[test]
fn parse_checksums_supports_openssl_style_lines() {
    let digest = "b".repeat(64);
    let contents = format!("SHA256(tool.tar.gz)= {digest}");

    let entries = ChecksumVerifier::parse_checksums(&contents);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].filename, "tool.tar.gz");
    assert_eq!(entries[0].digest, digest);
}

#[test]
fn parse_checksums_normalizes_uppercase_prefixed_digest_tokens() {
    let digest = "A".repeat(64);
    let contents = format!("sha256={digest}  tool.tar.gz");

    let entries = ChecksumVerifier::parse_checksums(&contents);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].filename, "tool.tar.gz");
    assert_eq!(entries[0].digest, digest.to_ascii_lowercase());
}

#[test]
fn verify_checksum_validates_sha256_digest() {
    let root = temp_root("verify");
    fs::create_dir_all(&root).expect("create root");
    let asset_path = root.join("asset.bin");
    fs::write(&asset_path, b"checksum-data").expect("write asset");

    let digest = format!("{:x}", sha2::Sha256::digest(b"checksum-data"));
    let entry = ChecksumVerifier::parse_standard_format(&format!("{digest}  asset.bin"))
        .expect("parse checksum entry");

    assert!(ChecksumVerifier::verify_checksum(&asset_path, &entry).expect("verify checksum"));

    cleanup(&root).expect("cleanup");
}

#[tokio::test]
async fn try_verify_file_returns_false_when_release_has_no_checksum_asset() {
    let root = temp_root("no-checksum");
    fs::create_dir_all(&root).expect("create root");
    let asset_path = root.join("tool.tar.gz");
    fs::write(&asset_path, b"payload").expect("write asset");

    let manager = ProviderManager::new(None, None, None, None).expect("provider manager");
    let verifier = ChecksumVerifier::new(&manager, &root);
    let mut progress: Option<fn(u64, u64)> = None;

    let verified = verifier
        .try_verify_file(
            &asset_path,
            &empty_release(),
            &Provider::Github,
            &mut progress,
        )
        .await
        .expect("verify without checksum");
    assert!(!verified);

    cleanup(&root).expect("cleanup");
}

#[test]
fn find_checksum_asset_prefers_asset_specific_files_then_common_names() {
    let assets = vec![
        Asset::new(
            "https://example.invalid/checksums.txt".to_string(),
            1,
            "checksums.txt".to_string(),
            10,
            Utc::now(),
        ),
        Asset::new(
            "https://example.invalid/tool.tar.gz.sha256".to_string(),
            2,
            "tool.tar.gz.sha256".to_string(),
            10,
            Utc::now(),
        ),
    ];
    let release = release_with_assets(assets);

    let selected = ChecksumVerifier::find_checksum_asset(&release, "tool.tar.gz")
        .expect("must select checksum asset");
    assert_eq!(selected.name, "tool.tar.gz.sha256");
}
