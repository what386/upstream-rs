use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum SearchMetadataValue {
    Integer(u64),
    Text(String),
}

#[derive(Debug, Clone)]
pub struct RepositorySearchResult {
    pub repo_slug: String,
    pub display_name: String,
    pub description: String,
    /// Provider-defined columns, in display order.
    pub metadata: Vec<(String, SearchMetadataValue)>,
    pub updated_at: DateTime<Utc>,
}
