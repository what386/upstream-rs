
use super::GitlabAdapter;
use crate::providers::gitlab::gitlab_client::{
    GitlabAssetsDto, GitlabClient, GitlabLinkDto, GitlabReleaseDto, GitlabSourceDto,
};

#[test]
fn parse_timestamp_handles_invalid_values() {
    assert_eq!(
        GitlabAdapter::parse_timestamp(""),
        chrono::DateTime::<chrono::Utc>::MIN_UTC
    );
    assert_eq!(
        GitlabAdapter::parse_timestamp("bad-date"),
        chrono::DateTime::<chrono::Utc>::MIN_UTC
    );
}

#[test]
fn convert_release_combines_links_and_sources_into_assets() {
    let adapter = GitlabAdapter::new(GitlabClient::new(None, None).expect("gitlab client"));
    let dto = GitlabReleaseDto {
        tag_name: "v1.9.0".to_string(),
        name: "v1.9.0".to_string(),
        description: "notes".to_string(),
        created_at: "2026-02-21T00:00:00Z".to_string(),
        released_at: None,
        upcoming_release: Some(false),
        assets: GitlabAssetsDto {
            count: 2,
            links: vec![GitlabLinkDto {
                id: 1,
                name: "tool-linux.tar.gz".to_string(),
                url: "https://example.invalid/tool-linux.tar.gz".to_string(),
                direct_asset_url: None,
                link_type: None,
            }],
            sources: vec![GitlabSourceDto {
                format: "tar.gz".to_string(),
                url: "https://example.invalid/source.tar.gz".to_string(),
            }],
        },
    };

    let release = adapter.convert_release(dto);
    assert_eq!(release.version.to_string(), "1.9.0");
    assert_eq!(release.assets.len(), 2);
    assert_eq!(release.assets[1].name, "source.tar.gz");
}
