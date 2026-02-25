use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitlabLinkDto {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub direct_asset_url: Option<String>,
    pub link_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitlabSourceDto {
    pub format: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitlabAssetsDto {
    pub count: i64,
    pub sources: Vec<GitlabSourceDto>,
    pub links: Vec<GitlabLinkDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitlabReleaseDto {
    pub tag_name: String,
    pub name: String,
    pub description: String,
    pub created_at: String,
    pub released_at: Option<String>,
    pub upcoming_release: Option<bool>,
    pub assets: GitlabAssetsDto,
}
