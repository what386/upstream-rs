use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubAssetDto {
    pub id: i64,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub name: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub browser_download_url: String,
    pub size: i64,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub content_type: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubReleaseDto {
    pub id: i64,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub tag_name: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub name: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub body: String,
    pub prerelease: bool,
    pub draft: bool,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub published_at: String,
    pub assets: Vec<GithubAssetDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubRepositorySearchResponseDto {
    pub items: Vec<GithubRepositorySearchItemDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubRepositorySearchItemDto {
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub full_name: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub name: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub description: String,
    pub stargazers_count: u64,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub language: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub updated_at: String,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub fork: bool,
}

fn deserialize_nullable_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}
