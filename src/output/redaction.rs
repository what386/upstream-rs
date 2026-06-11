pub fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("api_token")
        || key.contains("token")
        || key.contains("secret")
        || key.contains("password")
}

pub fn redact_secret(value: &str) -> String {
    if value.is_empty() {
        return "(empty)".to_string();
    }
    if value.chars().count() <= 8 {
        return "********".to_string();
    }

    let prefix: String = value.chars().take(4).collect();
    let suffix: String = value
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}...{suffix}")
}

#[cfg(test)]
mod tests {
    use super::{is_sensitive_key, redact_secret};

    #[test]
    fn sensitive_values_are_detected_and_redacted() {
        assert!(is_sensitive_key("github.api_token"));
        assert!(is_sensitive_key("auth.password"));
        assert!(!is_sensitive_key("github.enabled"));
        assert_eq!(
            redact_secret("ghp_abcdefghijklmnopqrstuvwxyz"),
            "ghp_...wxyz"
        );
        assert_eq!(redact_secret("short"), "********");
    }
}
