use clap::{Parser, error::ErrorKind};
use std::{fs, path::Path};
use upstream_rs::application::cli::arguments::Cli;

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
