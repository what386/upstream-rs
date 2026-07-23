pub fn ranked_matches<'a>(names: impl IntoIterator<Item = &'a str>, query: &str) -> Vec<String> {
    let query_lower = query.to_lowercase();
    let mut candidates = names
        .into_iter()
        .filter_map(|name| {
            let name_lower = name.to_lowercase();
            let distance = strsim::damerau_levenshtein(&query_lower, &name_lower);
            let max_len = query_lower.chars().count().max(name_lower.chars().count());
            let allowed = 1_usize.max((max_len * 3).div_ceil(10));
            let rank = if name_lower == query_lower {
                Some(0)
            } else if name_lower.contains(&query_lower) {
                Some(1)
            } else if query_lower.contains(&name_lower) {
                Some(2)
            } else if query_lower.chars().count() >= 3 && distance <= allowed {
                Some(3)
            } else {
                None
            };
            rank.map(|rank| (rank, distance, name.to_string()))
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.into_iter().map(|(_, _, name)| name).collect()
}

pub fn suggestions<'a>(
    names: impl IntoIterator<Item = &'a str>,
    query: &str,
    limit: usize,
) -> Vec<String> {
    ranked_matches(names, query)
        .into_iter()
        .take(limit)
        .collect()
}

pub fn did_you_mean(suggestions: &[String]) -> String {
    if suggestions.is_empty() {
        String::new()
    } else {
        format!(" Did you mean: {}?", suggestions.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::{ranked_matches, suggestions};

    #[test]
    fn ranks_substrings_before_edit_distance_matches() {
        let names = ["ripgrep", "bat", "bottom", "bot"];
        assert_eq!(suggestions(names, "ripgre", 3), vec!["ripgrep"]);
        assert_eq!(ranked_matches(names, "bot"), vec!["bot", "bottom", "bat"]);
        assert!(suggestions(names, "xyz", 3).is_empty());
    }
}
