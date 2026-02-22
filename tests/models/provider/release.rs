
use super::Release;
use crate::models::common::Version;
use crate::models::provider::Asset;
use chrono::Utc;

fn build_release() -> Release {
    Release {
        id: 1,
        tag: "v1.0.0".to_string(),
        name: "v1.0.0".to_string(),
        body: String::new(),
        is_draft: false,
        is_prerelease: false,
        assets: vec![Asset::new(
            "https://example.invalid/Bat".to_string(),
            1,
            "Bat".to_string(),
            10,
            Utc::now(),
        )],
        version: Version::new(1, 0, 0, false),
        published_at: Utc::now(),
    }
}

#[test]
fn get_asset_by_name_invariant_is_case_insensitive() {
    let release = build_release();
    let asset = release
        .get_asset_by_name_invariant("bat")
        .expect("case-insensitive hit");
    assert_eq!(asset.name, "Bat");
    assert!(release.get_asset_by_name_invariant("missing").is_none());
}
