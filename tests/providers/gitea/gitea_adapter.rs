
use super::GiteaAdapter;
use crate::providers::gitea::gitea_client::{GiteaAssetDto, GiteaClient, GiteaReleaseDto};

#[test]
fn parse_timestamp_returns_min_on_invalid_inputs() {
    assert_eq!(
        GiteaAdapter::parse_timestamp(""),
        chrono::DateTime::<chrono::Utc>::MIN_UTC
    );
    assert_eq!(
        GiteaAdapter::parse_timestamp("bad"),
        chrono::DateTime::<chrono::Utc>::MIN_UTC
    );
}

#[test]
fn convert_release_maps_core_fields() {
    let adapter = GiteaAdapter::new(GiteaClient::new(None, None).expect("gitea client"));
    let dto = GiteaReleaseDto {
        id: 7,
        tag_name: "v3.1.4".to_string(),
        name: "release".to_string(),
        body: "notes".to_string(),
        prerelease: false,
        draft: true,
        published_at: "2026-02-21T00:00:00Z".to_string(),
        assets: vec![GiteaAssetDto {
            id: 10,
            name: "tool-linux.tar.gz".to_string(),
            browser_download_url: "https://example.invalid/tool-linux.tar.gz".to_string(),
            size: 55,
            content_type: "application/gzip".to_string(),
            created_at: "2026-02-20T00:00:00Z".to_string(),
        }],
    };

    let release = adapter.convert_release(dto);
    assert_eq!(release.id, 7);
    assert_eq!(release.version.to_string(), "3.1.4");
    assert!(release.is_draft);
    assert_eq!(release.assets.len(), 1);
    assert_eq!(release.assets[0].id, 10);
}
