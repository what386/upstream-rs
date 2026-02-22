
use super::{GitlabClient, GitlabReleaseDto};

#[test]
fn new_normalizes_base_url_without_scheme() {
    let client = GitlabClient::new(None, Some("gitlab.example.com")).expect("client");
    assert_eq!(client.base_url, "https://gitlab.example.com");
}

#[test]
fn encode_project_path_percent_encodes_slashes() {
    assert_eq!(
        GitlabClient::encode_project_path("group/subgroup/project"),
        "group%2Fsubgroup%2Fproject"
    );
}

#[test]
fn gitlab_release_dto_deserializes_minimal_valid_payload() {
    let json = r#"
        {
          "tag_name": "v1.0.0",
          "name": "v1.0.0",
          "description": "notes",
          "created_at": "2026-02-21T00:00:00Z",
          "released_at": null,
          "upcoming_release": false,
          "assets": { "count": 0, "sources": [], "links": [] }
        }
        "#;

    let parsed = serde_json::from_str::<GitlabReleaseDto>(json).expect("parse release");
    assert_eq!(parsed.tag_name, "v1.0.0");
    assert_eq!(parsed.assets.count, 0);
    assert!(parsed.assets.links.is_empty());
}
