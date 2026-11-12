use clap::{ArgAction, Command};
use clap_complete_nushell::Nushell;
use std::io::Write;

// Keep these helpers inside the generated module so only externs are exported.
// Return all names and let Nushell handle quoting, prefix/fuzzy matching and sorting.
const HELPERS: &str = r#"
  def "nu-complete upstream values" [command: string, cursor: int, ...words: string] {
    try {
      let result = (^upstream __complete $command $cursor -- ...$words | complete)
      if $result.exit_code == 0 { $result.stdout | lines } else { [] }
    } catch { [] }
  }

  def "nu-complete upstream packages" [] {
    nu-complete upstream values info 0 ""
  }

  def "nu-complete upstream aliases" [] {
    nu-complete upstream values package 1 rename ""
  }

  def "nu-complete upstream docs keywords" [place: record] {
    if "--fetch" in $place.command {
      nu-complete upstream packages
    } else {
      []
    }
  }
"#;

pub fn generate(cmd: &mut Command, out: &mut dyn Write) {
    let mut cmd = cmd.clone();
    // The Nushell generator doesn't emit command aliases.
    let uninstall = cmd
        .find_subcommand("remove")
        .expect("remove command")
        .clone()
        .alias(None::<&str>)
        .name("uninstall");
    cmd = cmd
        .mut_subcommand("remove", |cmd| cmd.alias(None::<&str>))
        .subcommand(uninstall)
        .mut_subcommand("docs", |cmd| {
            // Nushell cannot express a flag with zero or more values. Declare the
            // switch and let name/keywords carry its values, including a bare --fetch.
            cmd.mut_arg("fetch", |arg| arg.action(ArgAction::SetTrue).num_args(0))
        });

    let mut generated = Vec::new();
    clap_complete::generate(Nushell, &mut cmd, "upstream", &mut generated);
    let generated = String::from_utf8(generated).expect("Nushell completion is UTF-8");
    let mut command = "";
    for line in generated.lines() {
        if let Some(name) = line.trim().strip_prefix("export extern ") {
            command = name.trim_end_matches(" [").trim_matches('"');
        }
        // A declared --help is intercepted by Nushell. Leave it undeclared so
        // both --help and -h reach upstream, like other unknown external flags.
        if !command.is_empty() && line.trim_start().starts_with("--help(") {
            continue;
        }

        // Augment only argument declarations in an extern signature. Enum
        // completers and all static command/flag metadata stay generator-owned.
        let arg = line.trim().split(':').next().unwrap_or("");
        let arg = arg.trim_start_matches("...").trim_end_matches('?');
        let completer = match (command, arg) {
            (
                "upstream remove" | "upstream uninstall" | "upstream reinstall"
                | "upstream upgrade" | "upstream doctor",
                "names",
            )
            | ("upstream info", "query")
            | ("upstream list", "filter")
            | ("upstream changelog" | "upstream docs", "name")
            | ("upstream history", "--package") => Some("packages"),
            (name, "name") if name.starts_with("upstream package ") => Some("packages"),
            ("upstream package rename", "old_name") => Some("aliases"),
            ("upstream docs", "keywords") => Some("docs keywords"),
            _ => None,
        };
        if let Some(completer) = completer {
            let replacement = format!(": string@\"nu-complete upstream {completer}\"");
            writeln!(out, "{}", line.replacen(": string", &replacement, 1))
                .expect("write Nushell argument completion");
        } else {
            writeln!(out, "{line}").expect("write Nushell completion");
        }
        if line == "module completions {" {
            writeln!(out, "{HELPERS}").expect("write Nushell completion helpers");
        } else if line.trim() == "]" {
            command = "";
        }
    }
}
