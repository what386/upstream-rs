pub mod markers;
pub mod parser;
pub mod simplify;

/// Convert an identifier into one flat, filesystem-safe name.
pub fn filesystem_name(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::filesystem_name;

    #[test]
    fn filesystem_name_flattens_canonical_package_ids() {
        assert_eq!(
            filesystem_name("github:BurntSushi/ripgrep"),
            "github_BurntSushi_ripgrep"
        );
    }
}
