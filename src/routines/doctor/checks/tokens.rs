use reqwest::StatusCode;
use std::collections::BTreeSet;

use crate::{
    models::{
        common::enums::Provider,
        upstream::{AppConfig, Package},
    },
    providers::{
        gitea::GiteaClient, github::GithubClient, gitlab::GitlabClient, http::http_status,
    },
};

use super::super::{DoctorReport, Level};

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenValidation {
    Valid,
    Invalid(String),
    RateLimited(String),
    Unknown(String),
}

fn configured_provider_targets<'a>(
    packages: impl IntoIterator<Item = &'a Package>,
    provider: Provider,
) -> Vec<Option<String>> {
    let mut targets = BTreeSet::new();
    for package in packages {
        if package.provider == provider {
            targets.insert(package.base_url.clone());
        }
    }

    if targets.is_empty() {
        return vec![None];
    }

    targets.into_iter().collect()
}

fn target_label(provider: &str, base_url: Option<&str>) -> String {
    match base_url {
        Some(base) => format!("{provider} API token for {base}"),
        None => format!("{provider} API token"),
    }
}

fn record_token_validation(report: &mut DoctorReport, label: String, validation: TokenValidation) {
    match validation {
        TokenValidation::Valid => {
            report.line(Level::Ok, format!("{label} works"));
        }
        TokenValidation::Invalid(message) => {
            report.line(Level::Fail, format!("{label} is invalid: {message}"));
        }
        TokenValidation::RateLimited(message) => {
            report.line(
                Level::Warn,
                format!("{label} could not be verified: {message}"),
            );
        }
        TokenValidation::Unknown(message) => {
            report.line(
                Level::Warn,
                format!("{label} could not be verified: {message}"),
            );
        }
    }
}

fn token_validation_from_response(
    response: &reqwest::Response,
    service: &str,
    url: &str,
) -> TokenValidation {
    let status = response.status();
    if status.is_success() {
        return TokenValidation::Valid;
    }

    if let Some(message) = http_status::rate_limit_message(status, response.headers(), service, url)
    {
        return TokenValidation::RateLimited(message);
    }

    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return TokenValidation::Invalid(format!("{service} rejected the token ({status})"));
    }

    TokenValidation::Unknown(format!("{service} returned {status}"))
}

async fn validate_github_token(token: &str) -> TokenValidation {
    let client = match GithubClient::new(Some(token), Default::default()) {
        Ok(client) => client,
        Err(err) => return TokenValidation::Unknown(format!("{err}")),
    };
    match client.check_token().await {
        Ok(response) => {
            token_validation_from_response(&response, "GitHub API", "https://api.github.com/user")
        }
        Err(err) => TokenValidation::Unknown(format!("GitHub API request failed: {err}")),
    }
}

async fn validate_gitlab_token(token: &str, base_url: Option<&str>) -> TokenValidation {
    let client = match GitlabClient::new(Some(token), base_url, Default::default()) {
        Ok(client) => client,
        Err(err) => return TokenValidation::Unknown(format!("{err}")),
    };
    match client.check_token().await {
        Ok(response) => {
            let url = response.url().to_string();
            token_validation_from_response(&response, "GitLab API", &url)
        }
        Err(err) => TokenValidation::Unknown(format!("GitLab API request failed: {err}")),
    }
}

async fn validate_gitea_token(token: &str, base_url: Option<&str>) -> TokenValidation {
    let client = match GiteaClient::new(Some(token), base_url, Default::default()) {
        Ok(client) => client,
        Err(err) => return TokenValidation::Unknown(format!("{err}")),
    };
    match client.check_token().await {
        Ok(response) => {
            let url = response.url().to_string();
            token_validation_from_response(&response, "Gitea API", &url)
        }
        Err(err) => TokenValidation::Unknown(format!("Gitea API request failed: {err}")),
    }
}

pub(in crate::routines::doctor) async fn check_provider_tokens(
    config: &AppConfig,
    packages: &[Package],
    report: &mut DoctorReport,
) {
    let mut configured = 0_u32;

    match config.github.api_token.as_deref().map(str::trim) {
        Some("") => {
            configured += 1;
            report.line(Level::Fail, "GitHub API token is configured but empty");
        }
        Some(token) => {
            configured += 1;
            record_token_validation(
                report,
                target_label("GitHub", None),
                validate_github_token(token).await,
            );
        }
        None => {}
    }

    for base_url in configured_provider_targets(packages, Provider::Gitlab) {
        match config.gitlab.api_token.as_deref().map(str::trim) {
            Some("") => {
                configured += 1;
                report.line(Level::Fail, "GitLab API token is configured but empty");
                break;
            }
            Some(token) => {
                configured += 1;
                record_token_validation(
                    report,
                    target_label("GitLab", base_url.as_deref()),
                    validate_gitlab_token(token, base_url.as_deref()).await,
                );
            }
            None => break,
        }
    }

    for base_url in configured_provider_targets(packages, Provider::Gitea) {
        match config.gitea.api_token.as_deref().map(str::trim) {
            Some("") => {
                configured += 1;
                report.line(Level::Fail, "Gitea API token is configured but empty");
                break;
            }
            Some(token) => {
                configured += 1;
                record_token_validation(
                    report,
                    target_label("Gitea", base_url.as_deref()),
                    validate_gitea_token(token, base_url.as_deref()).await,
                );
            }
            None => break,
        }
    }

    if configured == 0 {
        report.line(Level::Ok, "No provider API tokens configured");
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        models::{
            common::enums::{Channel, Filetype, Provider},
            upstream::Package,
        },
        routines::doctor::DoctorReport,
    };

    use super::{
        TokenValidation, configured_provider_targets, record_token_validation, target_label,
    };

    fn package_for_provider(name: &str, provider: Provider, base_url: Option<&str>) -> Package {
        Package::with_defaults(
            name.to_string(),
            format!("owner/{name}"),
            Filetype::Archive,
            None,
            None,
            Channel::Stable,
            provider,
            base_url.map(str::to_string),
        )
    }

    #[test]
    fn configured_provider_targets_uses_package_base_urls() {
        let packages = vec![
            package_for_provider("gitlab-default", Provider::Gitlab, None),
            package_for_provider(
                "gitlab-self-hosted",
                Provider::Gitlab,
                Some("git.example.com"),
            ),
            package_for_provider("github", Provider::Github, None),
        ];

        let targets = configured_provider_targets(&packages, Provider::Gitlab);

        assert_eq!(targets, vec![None, Some("git.example.com".to_string())]);
    }

    #[test]
    fn configured_provider_targets_defaults_when_provider_has_no_packages() {
        let packages = vec![package_for_provider("github", Provider::Github, None)];

        assert_eq!(
            configured_provider_targets(&packages, Provider::Gitea),
            vec![None]
        );
    }

    #[test]
    fn token_validation_outcomes_update_report_levels() {
        let mut report = DoctorReport::new();

        record_token_validation(
            &mut report,
            target_label("GitHub", None),
            TokenValidation::Valid,
        );
        record_token_validation(
            &mut report,
            target_label("GitLab", Some("git.example.com")),
            TokenValidation::Invalid("GitLab API rejected the token (401 Unauthorized)".into()),
        );
        record_token_validation(
            &mut report,
            target_label("Gitea", None),
            TokenValidation::RateLimited("Gitea API rate limit hit".into()),
        );

        assert_eq!(report.ok, 1);
        assert_eq!(report.fail, 1);
        assert_eq!(report.warn, 1);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.message == "GitHub API token works")
        );
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.contains("GitLab API token for git.example.com is invalid"))
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("Gitea API token could not be verified"))
        );
    }
}
