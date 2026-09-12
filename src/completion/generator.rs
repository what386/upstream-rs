use clap::{CommandFactory, Parser, ValueEnum};
use clap_complete::{Generator, Shell, generate};
use std::io;
use upstream_rs::application::cli::arguments::Cli;

#[derive(Parser, Debug)]
#[command(
    name = "upstream-completions",
    about = "Generate shell completions for the upstream CLI"
)]
struct CompletionArgs {
    #[arg(value_enum)]
    shell: CompletionShell,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum CompletionShell {
    Bash,
    Elvish,
    Fish,
    Powershell,
    Zsh,
}

fn main() {
    let args = CompletionArgs::parse();
    let mut cmd = Cli::command();
    let mut out = io::stdout();

    match args.shell {
        CompletionShell::Bash => generate_for_shell("bash", Shell::Bash, &mut cmd, &mut out),
        CompletionShell::Elvish => generate_for_shell("elvish", Shell::Elvish, &mut cmd, &mut out),
        CompletionShell::Fish => generate_for_shell("fish", Shell::Fish, &mut cmd, &mut out),
        CompletionShell::Powershell => {
            generate_for_shell("powershell", Shell::PowerShell, &mut cmd, &mut out)
        }
        CompletionShell::Zsh => generate_for_shell("zsh", Shell::Zsh, &mut cmd, &mut out),
    }
}

fn generate_for_shell<G: Generator>(
    shell: &str,
    generator: G,
    cmd: &mut clap::Command,
    out: &mut dyn io::Write,
) {
    let mut generated = Vec::new();
    generate(generator, cmd, "upstream", &mut generated);

    if shell == "powershell" {
        let generated = String::from_utf8(generated).expect("clap generated UTF-8 completion");
        let generated = generated
            .replace(
                "    $completions = @(switch ($command) {",
                r#"    $dynamicCompletions = @()
    $dynamicCommand = $command.Split(';')[-1]
    if ($dynamicCommand -in @('changelog', 'docs', 'doctor', 'history', 'info', 'list', 'package', 'reinstall', 'remove', 'rollback', 'upgrade')) {
        $dynamicWords = @()
        for ($i = 2; $i -lt $commandElements.Count; $i++) {
            $dynamicWords += $commandElements[$i].Value
        }
        $dynamicCursor = [Math]::Max(0, $dynamicWords.Count - 1)
        $dynamicCompletions = @(& upstream __complete $dynamicCommand $dynamicCursor -- $dynamicWords | ForEach-Object {
            [CompletionResult]::new($_, $_, [CompletionResultType]::ParameterValue, 'Installed package')
        })
    }
"#,
            )
            .replace(
                "    })\n\n    $completions.Where",
                "    })\n\n    $completions += $dynamicCompletions\n\n    $completions.Where",
            );
        out.write_all(generated.as_bytes())
            .expect("write generated PowerShell completion");
    } else if shell == "elvish" {
        let generated = String::from_utf8(generated).expect("clap generated UTF-8 completion");
        let generated = generated.replace(
            "    $completions[$command]\n}",
            r#"    var dynamic_command = (str:split ';' $command)[-1]
    if (or (== $dynamic_command 'changelog') (== $dynamic_command 'docs') (== $dynamic_command 'doctor') (== $dynamic_command 'history') (== $dynamic_command 'info') (== $dynamic_command 'list') (== $dynamic_command 'package') (== $dynamic_command 'reinstall') (== $dynamic_command 'remove') (== $dynamic_command 'rollback') (== $dynamic_command 'upgrade')) {
        var dynamic_words = $words[2..-1]
        var dynamic_cursor = (- (count $dynamic_words) 1)
        var dynamic_values = (upstream __complete $dynamic_command $dynamic_cursor -- $dynamic_words | slurp | str:split '\n')
        for value $dynamic_values {
            if (not (== $value '')) {
                cand $value 'Installed package'
            }
        }
    }
    $completions[$command]
}"#,
        );
        out.write_all(generated.as_bytes())
            .expect("write generated Elvish completion");
    } else {
        out.write_all(&generated)
            .expect("write generated completion");
    }
    let _ = write_dynamic_hook(shell, out);
}

fn write_dynamic_hook(shell: &str, out: &mut dyn io::Write) -> io::Result<()> {
    let hook = match shell {
        "bash" => {
            r#"

# Dynamic values are supplied by upstream's reduced completion startup.
_upstream_dynamic() {
    _upstream "$@"
    local command="${COMP_WORDS[1]}"
    case "$command" in
        changelog|docs|doctor|history|info|list|package|reinstall|remove|rollback|upgrade) ;;
        *) return ;;
    esac
    local offset=2
    local cursor=$((COMP_CWORD - offset))
    local -a words=("${COMP_WORDS[@]:offset}")
    local -a dynamic
    mapfile -t dynamic < <(command upstream __complete "$command" "$cursor" -- "${words[@]}")
    COMPREPLY+=("${dynamic[@]}")
}
complete -F _upstream_dynamic -o nosort -o bashdefault -o default upstream
"#
        }
        "fish" => {
            r#"

function __upstream_dynamic
    set -l words (commandline -opc)
    set -e words[1]
    test (count $words) -gt 0; or return
    set -l command $words[1]
    set -e words[1]
    set -a words (commandline -ct)
    set -l cursor (math (count $words) - 1)
    switch $command
        case changelog docs doctor history info list package reinstall remove rollback upgrade
            command upstream __complete $command $cursor -- $words
    end
end
for command in changelog docs doctor history info list package reinstall remove rollback upgrade
    complete -c upstream -n "__fish_upstream_using_subcommand $command" -a '(__upstream_dynamic)'
end
"#
        }
        "zsh" => {
            r#"

_upstream_dynamic() {
    _upstream "$@"
    local command=${words[2]}
    case "$command" in
        changelog|docs|doctor|history|info|list|package|reinstall|remove|rollback|upgrade) ;;
        *) return ;;
    esac
    local cursor=$((CURRENT - 3))
    local -a dynamic
    dynamic=(${(f)"$(command upstream __complete "$command" "$cursor" -- ${words[@]:3})"})
    compadd -- $dynamic
}
compdef _upstream_dynamic upstream
"#
        }
        "powershell" => {
            r#"

# Dynamic package values are delegated to the reduced native completer.
# The generated static completer remains responsible for flags and commands.
"#
        }
        "elvish" => {
            r#"

# Dynamic package values are delegated to the reduced native completer.
"#
        }
        _ => "",
    };

    out.write_all(hook.as_bytes())
}
