use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use reqwest::{Response, StatusCode, header};

pub(crate) fn error_for_status(response: &Response, service: &str, url: &str) -> Result<()> {
    if let Some(message) = rate_limit_message(response.status(), response.headers(), service, url) {
        bail!("{message}");
    }

    response
        .error_for_status_ref()
        .with_context(|| format!("{service} returned error for {url}"))?;

    Ok(())
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

    let mut message = format!("{service} rate limit hit while requesting {url} ({status})");

    if let Some(retry_after) = header_value(headers, header::RETRY_AFTER.as_str()) {
        message.push_str(&format!(
            ". Retry after {}",
            format_retry_after(retry_after)
        ));
    } else if let Some(reset) = header_value(headers, "x-ratelimit-reset") {
        message.push_str(&format!(
            ". Rate limit resets {}",
            format_rate_limit_reset(reset)
        ));
    } else {
        message.push_str(". Wait and retry later");
    }

    if let Some(limit) = header_value(headers, "x-ratelimit-limit") {
        message.push_str(&format!(". API limit: {limit} requests"));
    }

    message.push_str(". Using an API token or waiting for the reset usually fixes this.");
    Some(message)
}

fn is_rate_limited(status: StatusCode, headers: &header::HeaderMap) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || (status == StatusCode::FORBIDDEN
            && header_value(headers, "x-ratelimit-remaining") == Some("0"))
        || (status == StatusCode::FORBIDDEN
            && header_value(headers, header::RETRY_AFTER.as_str()).is_some())
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

    use super::rate_limit_message;

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

        assert!(message.contains("GitHub API rate limit hit"));
        assert!(message.contains("429 Too Many Requests"));
        assert!(message.contains("Retry after 120 seconds"));
        assert!(message.contains("API token"));
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

        assert!(message.contains("403 Forbidden"));
        assert!(message.contains("Rate limit resets at 2026-01-01 00:00:00 UTC"));
        assert!(message.contains("API limit: 60 requests"));
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
}
