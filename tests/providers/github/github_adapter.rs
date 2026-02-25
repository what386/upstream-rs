use super::GithubAdapter;
use crate::providers::github::github_client::GithubClient;
use crate::providers::github::github_dtos::{GithubAssetDto, GithubReleaseDto};

#[test]
fn parse_timestamp_returns_min_for_invalid_or_empty_values() {
    assert_eq!(
        GithubAdapter::parse_timestamp(""),
        chrono::DateTime::<chrono::Utc>::MIN_UTC
    );
    assert_eq!(
        GithubAdapter::parse_timestamp("not-a-date"),
        chrono::DateTime::<chrono::Utc>::MIN_UTC
    );
}

#[test]
fn convert_release_maps_assets_and_version() {
    let adapter = GithubAdapter::new(GithubClient::new(None).expect("github client"));
    let dto = GithubReleaseDto {
        id: 12,
        tag_name: "v2.3.4".to_string(),
        name: "Release 2.3.4".to_string(),
        body: "notes".to_string(),
        prerelease: true,
        draft: false,
        published_at: "2026-02-21T00:00:00Z".to_string(),
        assets: vec![GithubAssetDto {
            id: 9,
            name: "tool-linux-x86_64.tar.gz".to_string(),
            browser_download_url: "https://example.invalid/tool-linux-x86_64.tar.gz".to_string(),
            size: 123,
            content_type: "application/gzip".to_string(),
            created_at: "2026-02-20T00:00:00Z".to_string(),
        }],
    };

    let release = adapter.convert_release(dto);
    assert_eq!(release.id, 12);
    assert_eq!(release.version.to_string(), "2.3.4");
    assert!(release.is_prerelease);
    assert_eq!(release.assets.len(), 1);
    assert_eq!(release.assets[0].id, 9);
}
