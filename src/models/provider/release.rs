use chrono::{DateTime, Utc};

use crate::models::common::version::Version;
use crate::models::provider::asset::Asset;

#[derive(Debug, Clone)]
pub struct Release {
    pub id: u64,
    pub tag: String,
    pub name: String,
    pub body: String,

    pub is_draft: bool,
    pub is_prerelease: bool,

    pub assets: Vec<Asset>,
    pub version: Version,

    pub published_at: DateTime<Utc>,
}

impl Release {
    pub fn get_asset_by_name_invariant(&self, name: &str) -> Option<&Asset> {
        self.assets
            .iter()
            .find(|a| a.name.to_lowercase() == name.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::Release;
    use crate::models::provider::Asset;
    use crate::models::common::Version;
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
}
