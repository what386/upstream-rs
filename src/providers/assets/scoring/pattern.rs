use crate::models::upstream::Package;
use crate::providers::pattern_matcher::pattern_match_ratio;

pub(super) fn score(name: &str, package: &Package) -> i32 {
    let mut score = 0;

    if !package.match_pattern.is_empty() {
        score += (pattern_match_ratio(name, &package.match_pattern) * 100.0).round() as i32;
    }

    if !package.exclude_pattern.is_empty() {
        score -= (pattern_match_ratio(name, &package.exclude_pattern) * 100.0).round() as i32;
    }

    score
}
