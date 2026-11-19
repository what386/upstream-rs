//! Provider-dependent CLI policy. Clap handles provider-independent argument conflicts.
//! Keep these gates at the CLI boundary; providers and services need not validate flags.
use clap::{CommandFactory, error::ErrorKind};

use super::{
    arguments::{Cli, Commands},
    shared::{ProviderOptions, SearchOptions},
};
use crate::{
    models::common::enums::{BuildProfile, Channel, Provider},
    providers::discovery::infer_source,
};

type GateResult = Result<(), clap::Error>;

fn invalid(message: impl Into<String>) -> clap::Error {
    Cli::command().error(ErrorKind::ArgumentConflict, message.into())
}

fn source_provider(source: &str, options: &ProviderOptions) -> Option<Provider> {
    options
        .provider
        .clone()
        .or_else(|| infer_source(source).ok().map(|source| source.provider))
}

fn registry_options(
    provider: &Provider,
    channel: Option<&Channel>,
    base_url: Option<&str>,
) -> GateResult {
    if *provider != Provider::CratesIo {
        return Ok(());
    }
    if base_url.is_some() {
        return Err(invalid("crates.io does not support --base-url"));
    }
    if channel == Some(&Channel::Nightly) {
        return Err(invalid("crates.io does not support the nightly channel"));
    }
    Ok(())
}

fn release_source(command: &str, source: &str, options: &ProviderOptions) -> GateResult {
    if source_provider(source, options) == Some(Provider::CratesIo) {
        return Err(invalid(format!(
            "crates.io does not support {command}; use `upstream search <query> --provider crates-io` or `upstream build crates-io:<name>`",
        )));
    }
    Ok(())
}

fn search(options: &ProviderOptions, filters: &SearchOptions) -> GateResult {
    let provider = options.provider.clone().unwrap_or(Provider::Github);
    if !matches!(
        provider,
        Provider::Github | Provider::AppImageHub | Provider::CratesIo
    ) {
        return Err(invalid(format!(
            "{provider} does not support repository search"
        )));
    }
    registry_options(&provider, None, options.base_url.as_deref())?;
    if provider != Provider::Github {
        let unsupported = [
            (filters.language.is_some(), "--language"),
            (filters.topic.is_some(), "--topic"),
            (filters.min_stars.is_some(), "--min-stars"),
            (filters.max_stars.is_some(), "--max-stars"),
            (filters.pushed_after.is_some(), "--pushed-after"),
            (filters.include_forks, "--include-forks"),
            (filters.include_archived, "--include-archived"),
        ];
        if let Some((_, flag)) = unsupported.into_iter().find(|(set, _)| *set) {
            return Err(invalid(format!(
                "{provider} search does not support {flag}; use a text query and --limit"
            )));
        }
    }
    Ok(())
}

/// Reject invalid combinations immediately after parsing, before normal startup.
pub fn validate(command: &Commands) -> GateResult {
    match command {
        Commands::Build {
            repo_slug,
            branch,
            provider_options,
            channel,
            build_profile,
            ..
        } => {
            let Some(provider) = source_provider(repo_slug, provider_options) else {
                return Ok(());
            };
            if !provider.can_build() {
                return Err(invalid(format!(
                    "{provider} does not support source builds"
                )));
            }
            registry_options(
                &provider,
                Some(channel),
                provider_options.base_url.as_deref(),
            )?;
            if provider == Provider::CratesIo {
                if branch.is_some() {
                    return Err(invalid("crates.io does not support --branch"));
                }
                if build_profile.is_some_and(|profile| profile != BuildProfile::Rust) {
                    return Err(invalid("crates.io requires the Rust build profile"));
                }
            }
        }
        Commands::Install {
            repo_slug,
            install_options,
            ..
        } => {
            release_source("install", repo_slug, &install_options.provider_options)?;
        }
        Commands::Probe {
            repo_slug,
            provider_options,
            ..
        } => {
            release_source("probe", repo_slug, provider_options)?;
        }
        Commands::Search {
            provider_options,
            search_options,
            ..
        } => search(provider_options, search_options)?,
        Commands::Find {
            provider_options,
            search_options,
            ..
        } => {
            if provider_options.provider == Some(Provider::CratesIo) {
                return Err(invalid(
                    "crates.io does not support find's release installation; use `upstream search <query> --provider crates-io` then `upstream build crates-io:<name>`",
                ));
            }
            search(provider_options, search_options)?;
        }
        Commands::Reinstall {
            install_options, ..
        } => {
            if let Some(provider) = &install_options.provider_options.provider {
                registry_options(
                    provider,
                    install_options.channel.as_ref(),
                    install_options.provider_options.base_url.as_deref(),
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(std::iter::once("upstream").chain(args.iter().copied()))
            .unwrap_or_else(|error| panic!("{error}"))
    }

    fn rejected(args: &[&str], message: &str) {
        let error = validate(&parse(args).command).expect_err("expected a CLI gate rejection");
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
        assert!(error.to_string().contains(message), "{args:?}: {error}");
    }

    #[test]
    fn crates_build_gates_cover_explicit_and_inferred_providers() {
        let sources: &[&[&str]] = &[
            &["build", "ripgrep", "--provider", "crates-io"],
            &["build", "crates-io:ripgrep"],
            &["build", "https://crates.io/crates/ripgrep"],
        ];
        let invalid_flags: &[(&[&str], &str)] = &[
            (&["--branch", "main"], "--branch"),
            (&["--channel", "nightly"], "nightly"),
            (&["--build-profile", "go"], "Rust build profile"),
        ];
        for source in sources {
            for (flags, message) in invalid_flags {
                let args = source
                    .iter()
                    .chain(flags.iter())
                    .copied()
                    .collect::<Vec<_>>();
                rejected(&args, message);
            }
            for flags in [
                &[][..],
                &["--tag", "0.0.0"],
                &["--tag", "1.2.3-rc.1+build.5"],
                &["--semver", "0.0.0"],
                &["--semver", "1.2.3-rc.1+build.5"],
                &["--build-profile", "rust"],
                &["--channel", "preview"],
            ] {
                let args = source
                    .iter()
                    .chain(flags.iter())
                    .copied()
                    .collect::<Vec<_>>();
                validate(&parse(&args).command).unwrap();
            }
        }
        rejected(
            &[
                "build",
                "ripgrep",
                "--provider",
                "crates-io",
                "--base-url",
                "https://example.com",
            ],
            "--base-url",
        );
    }

    #[test]
    fn source_only_provider_rejects_release_commands() {
        for command in ["install", "probe"] {
            for source in ["crates-io:ripgrep", "https://crates.io/crates/ripgrep"] {
                rejected(&[command, source], "upstream build");
            }
            rejected(
                &[command, "ripgrep", "--provider", "crates-io"],
                "upstream build",
            );
        }
        rejected(
            &["find", "ripgrep", "--provider", "crates-io"],
            "upstream search",
        );
    }

    #[test]
    fn catalog_search_rejects_each_forge_filter_and_keeps_text_and_limit() {
        let flags: &[&[&str]] = &[
            &["--language", "rust"],
            &["--topic", "cli"],
            &["--min-stars", "0"],
            &["--max-stars", "10"],
            &["--pushed-after", "2026-01-01"],
            &["--include-forks"],
            &["--include-archived"],
        ];
        for provider in ["crates-io", "appimagehub"] {
            let source = ["search", "terminal", "--provider", provider];
            validate(&parse(&source).command).unwrap();
            for flag in flags {
                let args = source
                    .iter()
                    .chain(flag.iter())
                    .copied()
                    .collect::<Vec<_>>();
                rejected(&args, flag[0]);
            }
            let args = source
                .iter()
                .chain(["--limit", "5"].iter())
                .copied()
                .collect::<Vec<_>>();
            validate(&parse(&args).command).unwrap();
        }
        rejected(
            &[
                "search",
                "terminal",
                "--provider",
                "crates-io",
                "--base-url",
                "https://example.com",
            ],
            "--base-url",
        );
        rejected(
            &[
                "find",
                "terminal",
                "--provider",
                "appimagehub",
                "--language",
                "rust",
            ],
            "--language",
        );
    }

    #[test]
    fn search_and_find_reject_every_provider_without_a_search_implementation() {
        for command in ["search", "find"] {
            for provider in ["gitlab", "gitea", "direct", "scraper"] {
                rejected(
                    &[command, "terminal", "--provider", provider],
                    "does not support repository search",
                );
            }
        }
    }

    #[test]
    fn provider_capabilities_and_explicit_overrides_are_respected() {
        rejected(&["build", "appimagehub:123"], "source builds");
        for args in [
            vec!["build", "owner/repo", "--branch", "main"],
            vec![
                "build",
                "owner/repo",
                "--tag",
                "v1.2.3",
                "--build-profile",
                "go",
            ],
            vec![
                "build",
                "crates-io:ripgrep",
                "--provider",
                "github",
                "--tag",
                "v1.2.3",
            ],
            vec![
                "search",
                "terminal",
                "--language",
                "rust",
                "--include-forks",
            ],
            vec![
                "search",
                "terminal",
                "--provider",
                "appimagehub",
                "--base-url",
                "https://example.com",
            ],
        ] {
            validate(&parse(&args).command).unwrap();
        }
    }

    #[test]
    fn clap_still_handles_provider_independent_conflicts() {
        assert!(
            Cli::try_parse_from([
                "upstream",
                "build",
                "owner/repo",
                "--tag",
                "v1",
                "--branch",
                "main"
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from([
                "upstream",
                "build",
                "owner/repo",
                "--tag",
                "v1",
                "--semver",
                "1.0.0"
            ])
            .is_err()
        );
    }

    #[test]
    fn reinstall_accepts_versions_as_tags_without_resolving_stored_providers() {
        for name in ["rg", "crates-io:ripgrep"] {
            for flag in ["--tag", "--semver"] {
                let args = ["reinstall", name, flag, "14.1.1"];
                validate(&parse(&args).command).unwrap();
                let mut explicit = args.to_vec();
                explicit.extend(["--provider", "crates-io"]);
                validate(&parse(&explicit).command).unwrap();
            }
        }
    }
}
