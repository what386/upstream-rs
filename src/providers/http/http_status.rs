use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use reqwest::{Response, StatusCode, header};

pub(crate) fn error_for_status(response: &Response, service: &str, url: &str) -> Result<()> {
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }

    if let Some(message) = rate_limit_message(status, response.headers(), service, url) {
        bail!("{message}");
    }

    bail!(
        "{}",
        status_error_message(status, response.headers(), service, url)
    )
}

pub(crate) fn rate_limit_message(
    status: StatusCode,
    headers: &header::HeaderMap,
    service: &str,
    url: &str,
) -> Option<String> {
    if !is_rate_limited(status, headers) {
        return None;
    }

    let mut message = format!(
        "{service} request failed: {} while requesting {url}",
        friendly_status(status)
    );

    if let Some(retry_after) = header_value(headers, header::RETRY_AFTER.as_str()) {
        message.push_str(&format!(
            "; retry after {}",
            format_retry_after(retry_after)
        ));
    } else if let Some(reset) = header_value(headers, "x-ratelimit-reset") {
        message.push_str(&format!("; resets {}", format_rate_limit_reset(reset)));
    } else {
        message.push_str("; wait and try again later");
    }

    if let Some(limit) = header_value(headers, "x-ratelimit-limit") {
        message.push_str(&format!("; limit: {limit} requests"));
    }

    message.push_str("; configure an API token or retry after the reset");
    Some(message)
}

fn status_error_message(
    status: StatusCode,
    headers: &header::HeaderMap,
    service: &str,
    url: &str,
) -> String {
    let mut message = format!(
        "{service} request failed: {} while requesting {url}",
        friendly_status(status)
    );

    if let Some(retry_after) = header_value(headers, header::RETRY_AFTER.as_str()) {
        message.push_str(&format!(
            "; retry after {}",
            format_retry_after(retry_after)
        ));
    }

    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        message.push_str("; check the configured API token and repository access");
    } else if status == StatusCode::NOT_FOUND {
        message.push_str("; check the repository, release, or asset URL");
    }

    message
}

fn is_rate_limited(status: StatusCode, headers: &header::HeaderMap) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || (status == StatusCode::FORBIDDEN
            && header_value(headers, "x-ratelimit-remaining") == Some("0"))
        || (status == StatusCode::FORBIDDEN
            && header_value(headers, header::RETRY_AFTER.as_str()).is_some())
}

fn friendly_status(status: StatusCode) -> String {
    let label = match status {
        StatusCode::BAD_REQUEST => "bad request".to_string(),
        StatusCode::UNAUTHORIZED => "authentication failed".to_string(),
        StatusCode::FORBIDDEN => "access denied".to_string(),
        StatusCode::NOT_FOUND => "not found".to_string(),
        StatusCode::REQUEST_TIMEOUT => "request timed out".to_string(),
        StatusCode::CONFLICT => "conflict".to_string(),
        StatusCode::GONE => "gone".to_string(),
        StatusCode::PAYLOAD_TOO_LARGE => "response too large".to_string(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "unsupported content type".to_string(),
        StatusCode::UNPROCESSABLE_ENTITY => "unprocessable response".to_string(),
        StatusCode::TOO_MANY_REQUESTS => "rate limited".to_string(),
        StatusCode::INTERNAL_SERVER_ERROR => "server error".to_string(),
        StatusCode::BAD_GATEWAY => "bad gateway".to_string(),
        StatusCode::SERVICE_UNAVAILABLE => "service unavailable".to_string(),
        StatusCode::GATEWAY_TIMEOUT => "gateway timed out".to_string(),
        _ => status
            .canonical_reason()
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| "HTTP error".to_string()),
    };

    format!("{label} ({})", status.as_u16())
}

fn header_value<'a>(headers: &'a header::HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok().map(str::trim)
}

fn format_retry_after(value: &str) -> String {
    if value.chars().all(|c| c.is_ascii_digit()) {
        return format!("{value} seconds");
    }

    value.to_string()
}

fn format_rate_limit_reset(value: &str) -> String {
    let Some(seconds) = value.parse::<i64>().ok() else {
        return format!("at {value}");
    };

    let Some(reset_at) = DateTime::<Utc>::from_timestamp(seconds, 0) else {
        return format!("at {value}");
    };

    format!("at {}", reset_at.format("%Y-%m-%d %H:%M:%S UTC"))
}

#[cfg(test)]
mod tests {
    use reqwest::{StatusCode, header};

    use super::{friendly_status, rate_limit_message, status_error_message};

    #[test]
    fn rate_limit_message_explains_429_retry_after() {
        let mut headers = header::HeaderMap::new();
        headers.insert(header::RETRY_AFTER, header::HeaderValue::from_static("120"));

        let message = rate_limit_message(
            StatusCode::TOO_MANY_REQUESTS,
            &headers,
            "GitHub API",
            "https://api.example.invalid",
        )
        .expect("rate limit message");

        assert!(message.contains("GitHub API request failed"));
        assert!(message.contains("rate limited (429)"));
        assert!(message.contains("https://api.example.invalid"));
        assert!(message.contains("retry after 120 seconds"));
        assert!(message.contains("configure an API token"));
        assert!(!message.contains('\n'));
    }

    #[test]
    fn rate_limit_message_explains_github_forbidden_quota() {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            "x-ratelimit-remaining",
            header::HeaderValue::from_static("0"),
        );
        headers.insert(
            "x-ratelimit-reset",
            header::HeaderValue::from_static("1767225600"),
        );
        headers.insert("x-ratelimit-limit", header::HeaderValue::from_static("60"));

        let message = rate_limit_message(
            StatusCode::FORBIDDEN,
            &headers,
            "GitHub API",
            "https://api.example.invalid",
        )
        .expect("rate limit message");

        assert!(message.contains("access denied (403)"));
        assert!(message.contains("resets at 2026-01-01 00:00:00 UTC"));
        assert!(message.contains("limit: 60 requests"));
        assert!(!message.contains('\n'));
    }

    #[test]
    fn non_rate_limit_status_has_no_rate_limit_message() {
        let headers = header::HeaderMap::new();

        assert!(
            rate_limit_message(
                StatusCode::NOT_FOUND,
                &headers,
                "GitHub API",
                "https://api.example.invalid"
            )
            .is_none()
        );
    }

    #[test]
    fn status_error_message_formats_not_found_context() {
        let headers = header::HeaderMap::new();

        let message = status_error_message(
            StatusCode::NOT_FOUND,
            &headers,
            "GitHub API",
            "https://api.example.invalid/repos/owner/missing",
        );

        assert!(message.contains("GitHub API request failed"));
        assert!(message.contains("not found (404)"));
        assert!(message.contains("https://api.example.invalid/repos/owner/missing"));
        assert!(message.contains("check the repository"));
        assert!(!message.contains('\n'));
    }

    #[test]
    fn status_error_message_formats_auth_hint_and_retry_header() {
        let mut headers = header::HeaderMap::new();
        headers.insert(header::RETRY_AFTER, header::HeaderValue::from_static("30"));

        let message = status_error_message(
            StatusCode::UNAUTHORIZED,
            &headers,
            "GitLab API",
            "https://gitlab.example.invalid/api/v4/user",
        );

        assert!(message.contains("GitLab API request failed"));
        assert!(message.contains("authentication failed (401)"));
        assert!(message.contains("retry after 30 seconds"));
        assert!(message.contains("check the configured API token"));
        assert!(!message.contains('\n'));
    }

    #[test]
    fn friendly_status_names_common_http_errors() {
        assert_eq!(
            friendly_status(StatusCode::TOO_MANY_REQUESTS),
            "rate limited (429)"
        );
        assert_eq!(
            friendly_status(StatusCode::UNAUTHORIZED),
            "authentication failed (401)"
        );
        assert_eq!(friendly_status(StatusCode::NOT_FOUND), "not found (404)");
        assert_eq!(
            friendly_status(StatusCode::INTERNAL_SERVER_ERROR),
            "server error (500)"
        );
    }
}
