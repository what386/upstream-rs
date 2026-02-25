use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiteaAssetDto {
    pub id: i64,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub name: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub browser_download_url: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub content_type: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiteaReleaseDto {
    pub id: i64,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub tag_name: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub name: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub body: String,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub published_at: String,
    pub assets: Vec<GiteaAssetDto>,
}

fn deserialize_nullable_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}
