use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct RepositorySearchResult {
    pub repo_slug: String,
    pub display_name: String,
    pub description: String,
    pub stars: u64,
    pub language: String,
    pub updated_at: DateTime<Utc>,
}
