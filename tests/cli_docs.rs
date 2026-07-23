use clap::{Parser, error::ErrorKind};
use std::{fs, path::Path};
use upstream_rs::application::cli::arguments::{Cli, Commands};

const DOCUMENTS: &[&str] = &[
    "README.md",
    "docs/backup.md",
    "docs/build.md",
    "docs/commands.md",
    "docs/configuration.md",
    "docs/index.md",
    "docs/installation.md",
    "docs/packages.md",
    "docs/troubleshooting.md",
    "docs/trust.md",
];

#[test]
fn documented_example_commands_parse() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for relative_path in DOCUMENTS {
        let path = root.join(relative_path);
        let text = fs::read_to_string(&path).expect("read documentation");

        for (line_index, line) in text.lines().enumerate() {
            let command = line.trim();
            if !command.starts_with("upstream ") || command.contains('[') {
                continue;
            }

            let args = command
                .split_whitespace()
                .map(|part| part.trim_matches(['\'', '"']))
                .collect::<Vec<_>>();
            if let Err(error) = Cli::try_parse_from(args)
                && !matches!(
                    error.kind(),
                    ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
                )
            {
                failures.push(format!(
                    "{relative_path}:{}: {command}\n{}",
                    line_index + 1,
                    error.render().ansi()
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "documented commands that no longer parse:\n\n{}",
        failures.join("\n\n")
    );
}

#[test]
fn semantic_version_flags_parse_for_install_and_build() {
    let install = Cli::try_parse_from(["upstream", "install", "owner/tool", "tool", "-v", "1.2.4"])
        .expect("install semver");
    assert!(matches!(
        install.command,
        Commands::Install {
            semver: Some(value),
            ..
        } if value == "1.2.4"
    ));

    let build = Cli::try_parse_from([
        "upstream",
        "build",
        "owner/tool",
        "tool",
        "--semver",
        "1.2.4",
    ])
    .expect("build semver");
    assert!(matches!(
        build.command,
        Commands::Build {
            semver: Some(value),
            ..
        } if value == "1.2.4"
    ));
}

#[test]
fn semantic_version_conflicts_with_exact_refs() {
    let install = Cli::try_parse_from([
        "upstream",
        "install",
        "owner/tool",
        "tool",
        "--tag",
        "v1.2.4",
        "--semver",
        "1.2.4",
    ])
    .err()
    .expect("tag conflict");
    assert_eq!(install.kind(), ErrorKind::ArgumentConflict);

    let build = Cli::try_parse_from([
        "upstream",
        "build",
        "owner/tool",
        "tool",
        "--branch",
        "main",
        "--semver",
        "1.2.4",
    ])
    .err()
    .expect("branch conflict");
    assert_eq!(build.kind(), ErrorKind::ArgumentConflict);
}

#[test]
fn cache_and_package_settings_commands_parse() {
    for args in [
        vec!["upstream", "add", "upstream"],
        vec!["upstream", "add", "upstream", "--fetch"],
        vec!["upstream", "add", "--fetch"],
        vec!["upstream", "add", "upstream", "--dry-run"],
        vec!["upstream", "cache", "list", "--json"],
        vec!["upstream", "cache", "clean", "registry"],
        vec!["upstream", "cache", "clean", "build", "docs", "--dry-run"],
        vec![
            "upstream",
            "package",
            "set",
            "rg",
            "match_pattern=linux,x86_64",
        ],
        vec!["upstream", "package", "get", "rg", "trust_mode", "--json"],
        vec!["upstream", "package", "unset", "rg", "exclude_pattern"],
    ] {
        Cli::try_parse_from(args).expect("new command should parse");
    }
}
