use anyhow::{Result, bail};

// Windows treats backslashes literally except immediately before double quotes.
pub fn split_windows_arguments(template: &str) -> Result<Vec<String>> {
    let mut chars = template.chars().peekable();
    let mut arguments = Vec::new();
    while chars.peek().is_some() {
        while chars.peek().is_some_and(|ch| matches!(ch, ' ' | '\t')) {
            chars.next();
        }

        if chars.peek().is_none() {
            break;
        }

        let mut argument = String::new();
        let mut quoted = false;
        while let Some(&ch) = chars.peek() {
            if !quoted && matches!(ch, ' ' | '\t') {
                break;
            }

            let mut backslashes = 0;
            while chars.peek() == Some(&'\\') {
                backslashes += 1;
                chars.next();
            }

            if chars.peek() == Some(&'"') {
                argument.extend(std::iter::repeat_n('\\', backslashes / 2));
                chars.next();
                if backslashes % 2 == 1 {
                    argument.push('"');
                } else if quoted && chars.peek() == Some(&'"') {
                    argument.push('"');
                    chars.next();
                } else {
                    quoted = !quoted;
                }
            } else {
                argument.extend(std::iter::repeat_n('\\', backslashes));
                if !quoted && chars.peek().is_some_and(|ch| matches!(ch, ' ' | '\t')) {
                    break;
                }

                if let Some(ch) = chars.next() {
                    argument.push(ch);
                }
            }
        }

        if quoted {
            bail!("Unterminated double quote in Windows command line");
        }

        arguments.push(argument);
    }

    Ok(arguments)
}

pub fn quote_windows_argument(argument: &str) -> String {
    if !argument.is_empty() && !argument.contains([' ', '\t', '"']) {
        return argument.to_string();
    }

    let mut quoted = String::from("\"");
    let mut backslashes = 0;
    for ch in argument.chars() {
        if ch == '\\' {
            backslashes += 1;
            continue;
        }

        quoted.extend(std::iter::repeat_n(
            '\\',
            if ch == '"' {
                backslashes * 2 + 1
            } else {
                backslashes
            },
        ));
        backslashes = 0;
        quoted.push(ch);
    }

    quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_parser_preserves_paths_and_uses_double_quotes() {
        assert_eq!(
            split_windows_arguments(r#"C:\Tools\runner.exe "C:\Program Files\tool.exe" --profile "Work Profile" "" C:\O'Brien\ C:\next"#).unwrap(),
            [r"C:\Tools\runner.exe", r"C:\Program Files\tool.exe", "--profile", "Work Profile", "", r"C:\O'Brien\", r"C:\next"]
        );

        assert!(split_windows_arguments(r#"{} "unterminated"#).is_err());
    }

    #[test]
    fn windows_arguments_round_trip_spaces_quotes_and_trailing_backslashes() {
        let arguments = [
            "",
            "plain",
            "Work Profile",
            "tabs\there",
            r"C:\my folder\",
            r#"a\"quoted"value"#,
            "O'Brien",
            "%TEMP%",
            "日本語",
        ];

        let command_line = arguments
            .iter()
            .map(|argument| quote_windows_argument(argument))
            .collect::<Vec<_>>()
            .join(" ");

        assert_eq!(split_windows_arguments(&command_line).unwrap(), arguments);
    }

    #[test]
    fn windows_parser_matches_documented_backslash_and_quote_examples() {
        for (command, expected) in [
            (r#""a b c" d e"#, vec!["a b c", "d", "e"]),
            (r#""ab\"c" "\\" d"#, vec!["ab\"c", "\\", "d"]),
            (r#"a\\\b d"e f"g h"#, vec![r"a\\\b", "de fg", "h"]),
            (r#"a\\\"b c d"#, vec![r#"a\"b"#, "c", "d"]),
            (r#"a\\\\"b c" d e"#, vec![r"a\\b c", "d", "e"]),
        ] {
            assert_eq!(
                split_windows_arguments(command).unwrap(),
                expected,
                "{command}"
            );
        }
    }
}
