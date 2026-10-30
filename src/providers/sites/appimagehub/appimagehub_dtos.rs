use anyhow::{Context, Result, anyhow, ensure};
use serde_json::{Map, Value};

#[derive(Debug, Clone)]
pub struct AppImageHubFileDto {
    pub id: u64,
    pub name: String,
    pub download_url: String,
    pub package_type: String,
    pub architecture: String,
    pub version: String,
    pub size_kib: u64,
}

#[derive(Debug, Clone)]
pub struct AppImageHubContentDto {
    pub id: String,
    pub name: String,
    pub version: String,
    pub summary: String,
    pub description: String,
    pub changed: String,
    pub created: String,
    pub files: Vec<AppImageHubFileDto>,
}

impl AppImageHubContentDto {
    /// OCS returns a flat envelope with dynamically numbered download fields.
    pub fn from_response(response: &Value, id: &str) -> Result<Self> {
        let items = response_items(response)?;

        let content = items
            .iter()
            .find_map(|item| {
                let item = item.as_object()?;
                (text(item, "id") == id).then_some(item)
            })
            .ok_or_else(|| anyhow!("AppImageHub listing {id} was not found"))?;

        Self::from_fields(content)
    }

    fn from_fields(content: &Map<String, Value>) -> Result<Self> {
        let id = text(content, "id")
            .parse::<u64>()
            .context("Invalid AppImageHub content ID")?;

        ensure!(id > 0, "AppImageHub content ID must be positive");
        let mut indices = content
            .keys()
            .filter_map(|key| key.strip_prefix("downloadlink")?.parse::<u64>().ok())
            .collect::<Vec<_>>();

        indices.sort_unstable();
        let files = indices
            .into_iter()
            .map(|index| AppImageHubFileDto {
                id: index,
                name: text(content, &format!("downloadname{index}")),
                download_url: text(content, &format!("downloadlink{index}")),
                package_type: text(content, &format!("download_package_type{index}")),
                architecture: text(content, &format!("download_package_arch{index}")),
                version: text(content, &format!("download_version{index}")),
                size_kib: text(content, &format!("downloadsize{index}"))
                    .parse()
                    .unwrap_or(0),
            })
            .collect();

        Ok(Self {
            id: id.to_string(),
            name: text(content, "name"),
            version: text(content, "version"),
            summary: text(content, "summary"),
            description: text(content, "description"),
            changed: text(content, "changed"),
            created: text(content, "created"),
            files,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AppImageHubCategoryDto {
    pub id: String,
    pub xdg_type: String,
}

impl AppImageHubCategoryDto {
    pub fn from_response(response: &Value) -> Result<Vec<Self>> {
        response_items(response)?
            .iter()
            .map(|item| {
                let fields = item.as_object().context("Invalid AppImageHub category")?;
                let id = text(fields, "id")
                    .parse::<u64>()
                    .context("Invalid AppImageHub category ID")?;

                Ok(Self {
                    id: id.to_string(),
                    xdg_type: text(fields, "xdg_type"),
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct AppImageHubSearchPageDto {
    pub contents: Vec<AppImageHubContentDto>,
    pub total_items: u64,
}

impl AppImageHubSearchPageDto {
    pub fn from_response(response: &Value) -> Result<Self> {
        let contents = response_items(response)?
            .iter()
            .map(|item| {
                AppImageHubContentDto::from_fields(
                    item.as_object()
                        .context("Invalid AppImageHub search item")?,
                )
            })
            .collect::<Result<Vec<_>>>()?;

        let fields = response
            .as_object()
            .context("Invalid AppImageHub search response")?;

        let total_items = text(fields, "totalitems")
            .parse()
            .context("Invalid AppImageHub search result count")?;

        Ok(Self {
            contents,
            total_items,
        })
    }
}

impl AppImageHubFileDto {
    pub fn is_appimage(&self) -> bool {
        self.package_type.eq_ignore_ascii_case("appimage")
            || self.name.to_ascii_lowercase().ends_with(".appimage")
    }
}

fn response_items(response: &Value) -> Result<&[Value]> {
    let envelope = response
        .as_object()
        .context("Invalid AppImageHub OCS response")?;

    let status = text(envelope, "statuscode");
    ensure!(
        status == "100" && text(envelope, "status") == "ok",
        "AppImageHub OCS error {status}: {}",
        text(envelope, "message")
    );

    let items = response
        .get("data")
        .and_then(Value::as_array)
        .context("Invalid AppImageHub OCS content data")?;

    Ok(items)
}

fn text(fields: &Map<String, Value>, key: &str) -> String {
    match fields.get(key) {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_empty_search_results_and_validates_counts_and_categories() {
        let empty =
            serde_json::json!({"status":"ok","statuscode":"100","totalitems":"0","data":[]});

        let page = AppImageHubSearchPageDto::from_response(&empty).unwrap();
        assert!(page.contents.is_empty());
        assert_eq!(page.total_items, 0);
        assert!(
            AppImageHubCategoryDto::from_response(&empty)
                .unwrap()
                .is_empty()
        );

        let invalid =
            serde_json::json!({"status":"ok","statuscode":100,"totalitems":"invalid","data":[]});

        assert!(AppImageHubSearchPageDto::from_response(&invalid).is_err());
        assert!(AppImageHubSearchPageDto::from_response(&serde_json::json!({"status":"failed","statuscode":200,"message":"rate limited","data":[]})).is_err());
    }

    #[test]
    fn rejects_api_errors_and_missing_content() {
        for value in [
            json!({"status":"failed", "statuscode":998, "message":"not found", "data":[]}),
            json!({"status":"ok", "statuscode":100, "data":[]}),
            json!({"status":"ok", "statuscode":100, "data":null}),
        ] {
            assert!(AppImageHubContentDto::from_response(&value, "2259793").is_err());
        }
    }
}
