use crate::providers::github::github_dtos::GithubReleaseDto;

#[test]
fn github_release_dto_accepts_nullable_string_fields() {
    let json = r#"
    {
      "id": 1,
      "tag_name": "v1.0.0",
      "name": null,
      "body": null,
      "prerelease": false,
      "draft": false,
      "published_at": null,
      "assets": [
        {
          "id": 42,
          "name": "tree-sitter-linux.tar.gz",
          "browser_download_url": "https://example.com/asset.tar.gz",
          "size": 1234,
          "content_type": null,
          "created_at": null
        }
      ]
    }
    "#;

    let parsed = serde_json::from_str::<GithubReleaseDto>(json).expect("valid release JSON");
    assert_eq!(parsed.name, "");
    assert_eq!(parsed.body, "");
    assert_eq!(parsed.published_at, "");
    assert_eq!(parsed.assets[0].content_type, "");
    assert_eq!(parsed.assets[0].created_at, "");
}
