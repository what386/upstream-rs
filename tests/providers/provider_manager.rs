use super::ProviderManager;
use crate::models::common::Version;
use crate::models::common::enums::{Channel, Filetype, Provider};
use crate::models::provider::{Asset, Release};
use crate::models::upstream::Package;
use chrono::Utc;

fn make_release(assets: Vec<Asset>, prerelease: bool, tag: &str) -> Release {
    Release {
        id: 1,
        tag: tag.to_string(),
        name: tag.to_string(),
        body: String::new(),
        is_draft: false,
        is_prerelease: prerelease,
        assets,
        version: Version::new(1, 0, 0, prerelease),
        published_at: Utc::now(),
    }
}

fn make_package(filetype: Filetype) -> Package {
    Package::with_defaults(
        "tool".to_string(),
        "owner/tool".to_string(),
        filetype,
        Some("static".to_string()),
        Some("debug".to_string()),
        Channel::Stable,
        Provider::Github,
        None,
    )
}

#[test]
fn nightly_release_detection_is_case_insensitive() {
    assert!(ProviderManager::is_nightly_release("Nightly-20260221"));
    assert!(!ProviderManager::is_nightly_release("v1.2.3"));
}

#[test]
fn preview_release_excludes_nightly_tags() {
    let preview = make_release(Vec::new(), true, "v1.2.3-rc1");
    let nightly = make_release(Vec::new(), true, "nightly-20260221");

    assert!(ProviderManager::is_preview_release(&preview));
    assert!(!ProviderManager::is_preview_release(&nightly));
}

#[cfg(target_os = "linux")]
#[test]
fn resolve_auto_filetype_prefers_appimage_then_archives_on_linux() {
    let release = make_release(
        vec![
            Asset::new(
                "https://example.invalid/tool.tar.gz".to_string(),
                1,
                "tool.tar.gz".to_string(),
                200_000,
                Utc::now(),
            ),
            Asset::new(
                "https://example.invalid/tool.AppImage".to_string(),
                2,
                "tool.AppImage".to_string(),
                200_000,
                Utc::now(),
            ),
        ],
        false,
        "v1.0.0",
    );

    assert_eq!(
        ProviderManager::resolve_auto_filetype(&release).expect("resolve"),
        Filetype::AppImage
    );
}

#[cfg(target_os = "macos")]
#[test]
fn resolve_auto_filetype_prefers_macapp_on_macos() {
    let release = make_release(
        vec![
            Asset::new(
                "https://example.invalid/tool.tar.gz".to_string(),
                1,
                "tool.tar.gz".to_string(),
                200_000,
                Utc::now(),
            ),
            Asset::new(
                "https://example.invalid/tool.app".to_string(),
                2,
                "tool.app".to_string(),
                200_000,
                Utc::now(),
            ),
        ],
        false,
        "v1.0.0",
    );

    assert_eq!(
        ProviderManager::resolve_auto_filetype(&release).expect("resolve"),
        Filetype::MacApp
    );
}

#[cfg(windows)]
#[test]
fn resolve_auto_filetype_prefers_winexe_on_windows() {
    let release = make_release(
        vec![
            Asset::new(
                "https://example.invalid/tool.tar.gz".to_string(),
                1,
                "tool.tar.gz".to_string(),
                200_000,
                Utc::now(),
            ),
            Asset::new(
                "https://example.invalid/tool.exe".to_string(),
                2,
                "tool.exe".to_string(),
                200_000,
                Utc::now(),
            ),
        ],
        false,
        "v1.0.0",
    );

    assert_eq!(
        ProviderManager::resolve_auto_filetype(&release).expect("resolve"),
        Filetype::WinExe
    );
}

#[test]
fn get_candidate_assets_sorts_by_score_descending() {
    let manager = ProviderManager::new(None, None, None, None).expect("manager");
    let package = make_package(Filetype::Archive);
    let release = make_release(
        vec![
            Asset::new(
                "https://example.invalid/tool-debug.tar.gz".to_string(),
                1,
                "tool-debug.tar.gz".to_string(),
                200_000,
                Utc::now(),
            ),
            Asset::new(
                "https://example.invalid/tool-static.tar.bz2".to_string(),
                2,
                "tool-static.tar.bz2".to_string(),
                200_000,
                Utc::now(),
            ),
        ],
        false,
        "v1.0.0",
    );

    let candidates = manager
        .get_candidate_assets(&release, &package)
        .expect("candidates");
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].asset.name, "tool-static.tar.bz2");
    assert!(candidates[0].score > candidates[1].score);
}

#[test]
fn find_recommended_asset_returns_highest_scored_compatible_asset() {
    let manager = ProviderManager::new(None, None, None, None).expect("manager");
    let package = make_package(Filetype::Archive);
    let release = make_release(
        vec![
            Asset::new(
                "https://example.invalid/tool-debug.tar.gz".to_string(),
                1,
                "tool-debug.tar.gz".to_string(),
                200_000,
                Utc::now(),
            ),
            Asset::new(
                "https://example.invalid/tool-static.tar.bz2".to_string(),
                2,
                "tool-static.tar.bz2".to_string(),
                200_000,
                Utc::now(),
            ),
        ],
        false,
        "v1.0.0",
    );

    let best = manager
        .find_recommended_asset(&release, &package)
        .expect("best asset");
    assert_eq!(best.name, "tool-static.tar.bz2");
}
