use anyhow::Result;

use super::search::{DocsSearchResult, search_readme};

const EMBEDDED_README: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"));

pub fn run(package_name: &str, query: &str) -> Result<DocsSearchResult> {
    Ok(search_readme(
        package_name,
        "README.md",
        query,
        EMBEDDED_README,
    ))
}
