use super::schema::RegistryIndex;
use crate::utils::name_match;

pub(super) fn missing_package_message(index: &RegistryIndex, name: &str) -> String {
    let suggestions = name_match::suggestions(index.packages.keys().map(String::as_str), name, 3);
    let mut message = format!("Package '{name}' was not found in the registry.");
    message.push_str(&name_match::did_you_mean(&suggestions));
    message.push_str(&format!(
        " Use 'upstream add {name} --fetch' to refresh the local index."
    ));
    message
}

#[cfg(test)]
mod tests {
    use super::missing_package_message;
    use crate::routines::registry::schema::parse_index;

    #[test]
    fn ranks_substrings_before_fuzzy_matches() {
        let index = parse_index(br#"{"version":1,"packages":{"ripgrep":{"revision":1,"desktop":false,"trust":"checksum","install":{"type":"release","repo":"o/ripgrep","provider":"github"}},"bat":{"revision":1,"desktop":false,"trust":"checksum","install":{"type":"release","repo":"o/bat","provider":"github"}},"bottom":{"revision":1,"desktop":false,"trust":"checksum","install":{"type":"release","repo":"o/bottom","provider":"github"}}}}"#).expect("valid index");
        assert!(missing_package_message(&index, "ripgre").contains("Did you mean: ripgrep?"));
        assert!(missing_package_message(&index, "bot").contains("Did you mean: bottom"));
        assert!(!missing_package_message(&index, "xyz").contains("Did you mean"));
    }
}
