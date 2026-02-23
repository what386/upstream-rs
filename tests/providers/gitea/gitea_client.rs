use super::{GiteaClient, GiteaReleaseDto};

#[test]
fn new_normalizes_base_url_without_scheme() {
    let client = GiteaClient::new(None, Some("gitea.example.com")).expect("client");
    assert_eq!(client.base_url, "https://gitea.example.com");
}

#[test]
fn nullable_string_fields_deserialize_to_empty_strings() {
    let json = r#"
        {
          "id": 7,
          "tag_name": null,
          "name": null,
          "body": null,
          "prerelease": false,
          "draft": false,
          "published_at": null,
          "assets": [
            {
              "id": 1,
              "name": null,
              "browser_download_url": null,
              "size": 0,
              "content_type": null,
              "created_at": null
            }
          ]
        }
        "#;

    let parsed = serde_json::from_str::<GiteaReleaseDto>(json).expect("parse release");
    assert_eq!(parsed.tag_name, "");
    assert_eq!(parsed.name, "");
    assert_eq!(parsed.body, "");
    assert_eq!(parsed.published_at, "");
    assert_eq!(parsed.assets[0].name, "");
    assert_eq!(parsed.assets[0].browser_download_url, "");
}
