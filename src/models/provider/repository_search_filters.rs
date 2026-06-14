use chrono::NaiveDate;
use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RepositorySearchFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_stars: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pushed_after: Option<NaiveDate>,
    pub include_forks: bool,
    pub include_archived: bool,
}

impl RepositorySearchFilters {
    pub fn new(
        language: Option<String>,
        topic: Option<String>,
        min_stars: Option<u64>,
        pushed_after: Option<NaiveDate>,
        include_forks: bool,
        include_archived: bool,
    ) -> Self {
        Self {
            language: normalize_filter_value(language),
            topic: normalize_filter_value(topic),
            min_stars,
            pushed_after,
            include_forks,
            include_archived,
        }
    }
}

fn normalize_filter_value(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
