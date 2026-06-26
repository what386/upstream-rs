pub fn median_sorted(sorted_values: &[u64]) -> Option<u64> {
    if sorted_values.is_empty() {
        return None;
    }

    let mid = sorted_values.len() / 2;
    if sorted_values.len().is_multiple_of(2) {
        Some(sorted_values[mid - 1].midpoint(sorted_values[mid]))
    } else {
        Some(sorted_values[mid])
    }
}
