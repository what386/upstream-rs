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
    pub changed: String,
    pub created: String,
    pub files: Vec<AppImageHubFileDto>,
}

impl AppImageHubContentDto {
    /// OCS returns a flat envelope with dynamically numbered download fields.
    pub fn from_response(response: &Value, id: &str) -> Result<Self> {
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

        let content = items
            .iter()
            .find_map(|item| {
                let item = item.as_object()?;
                (text(item, "id") == id).then_some(item)
            })
            .ok_or_else(|| anyhow!("AppImageHub listing {id} was not found"))?;

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
            id: id.to_owned(),
            name: text(content, "name"),
            version: text(content, "version"),
            changed: text(content, "changed"),
            created: text(content, "created"),
            files,
        })
    }
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
