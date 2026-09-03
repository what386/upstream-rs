use super::markers::contains_platform_marker;

/// Derive a command alias from an executable filename by removing a trailing
/// version, platform, or caller-provided asset-match identifier.
pub fn executable_alias(filename: &str, match_identifiers: &[String]) -> String {
    let filename = filename
        .strip_suffix(".exe")
        .or_else(|| filename.strip_suffix(".cmd"))
        .or_else(|| filename.strip_suffix(".bat"))
        .unwrap_or(filename);

    for (index, character) in filename.char_indices() {
        if !matches!(character, '-' | '_' | '.') {
            continue;
        }

        let suffix = &filename[index + character.len_utf8()..];
        if is_match_identifier(suffix, match_identifiers) || is_version_identifier(suffix) {
            return filename[..index].to_string();
        }
    }

    filename.to_string()
}

fn is_match_identifier(value: &str, match_identifiers: &[String]) -> bool {
    contains_platform_marker(value)
        || match_identifiers
            .iter()
            .any(|identifier| identifier_contains(value, identifier))
}

fn identifier_contains(value: &str, identifier: &str) -> bool {
    let normalize = |value: &str| {
        value
            .chars()
            .map(|character| match character {
                '-' | '_' | '.' => '-',
                character => character.to_ascii_lowercase(),
            })
            .collect::<String>()
    };

    let value = normalize(value);
    let identifier = normalize(identifier);
    value == identifier
        || value.starts_with(&format!("{identifier}-"))
        || value.ends_with(&format!("-{identifier}"))
        || value.contains(&format!("-{identifier}-"))
}

fn is_version_identifier(value: &str) -> bool {
    let value = value.strip_prefix('v').unwrap_or(value);
    let mut segments = value.split(['-', '_', '.']);
    let Some(first) = segments.next() else {
        return false;
    };

    first.chars().all(|character| character.is_ascii_digit())
        && segments
            .next()
            .is_some_and(|segment| segment.chars().all(|character| character.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::executable_alias;

    #[test]
    fn strips_platform_and_version_suffixes() {
        assert_eq!(
            executable_alias("code-x86_64-unknown-linux-musl", &[]),
            "code"
        );

        assert_eq!(executable_alias("yq_linux_amd64", &[]), "yq");
        assert_eq!(executable_alias("bat-v0.26.1", &[]), "bat");
    }

    #[test]
    fn strips_configured_match_identifier_only_at_a_boundary() {
        assert_eq!(
            executable_alias("code-portable", &["portable".to_string()]),
            "code"
        );

        assert_eq!(executable_alias("code-helper", &[]), "code-helper");
    }
}
