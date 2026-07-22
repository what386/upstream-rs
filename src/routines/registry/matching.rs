use super::schema::RegistryIndex;

pub(super) fn missing_package_message(index: &RegistryIndex, name: &str) -> String {
    let suggestions = suggestions(index, name, 3);
    let mut message = format!("Package '{name}' was not found in the registry.");
    if !suggestions.is_empty() {
        message.push_str(&format!(" Did you mean: {}?", suggestions.join(", ")));
    }
    message.push_str(&format!(
        " Use 'upstream add {name} --fetch' to refresh the local index."
    ));
    message
}

fn suggestions(index: &RegistryIndex, query: &str, limit: usize) -> Vec<String> {
    let query_lower = query.to_lowercase();
    let mut candidates = index
        .packages
        .keys()
        .filter_map(|name| {
            let name_lower = name.to_lowercase();
            let substring = name_lower.contains(&query_lower) || query_lower.contains(&name_lower);
            let distance = strsim::damerau_levenshtein(&query_lower, &name_lower);
            let max_len = query_lower.chars().count().max(name_lower.chars().count());
            let allowed = 1_usize.max((max_len * 3).div_ceil(10));
            (substring || (query_lower.chars().count() >= 3 && distance <= allowed))
                .then(|| (!substring, distance, name.clone()))
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates
        .into_iter()
        .take(limit)
        .map(|(_, _, name)| name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::suggestions;
    use crate::routines::registry::schema::parse_index;

    #[test]
    fn ranks_substrings_before_fuzzy_matches() {
        let index = parse_index(br#"{"version":1,"packages":{"ripgrep":{"revision":1,"desktop":false,"trust":"checksum","install":{"type":"release","repo":"o/ripgrep","provider":"github"}},"bat":{"revision":1,"desktop":false,"trust":"checksum","install":{"type":"release","repo":"o/bat","provider":"github"}},"bottom":{"revision":1,"desktop":false,"trust":"checksum","install":{"type":"release","repo":"o/bottom","provider":"github"}}}}"#).expect("valid index");
        assert_eq!(suggestions(&index, "ripgre", 3), vec!["ripgrep"]);
        assert_eq!(
            suggestions(&index, "bot", 3).first().map(String::as_str),
            Some("bottom")
        );
        assert!(suggestions(&index, "xyz", 3).is_empty());
    }
}
