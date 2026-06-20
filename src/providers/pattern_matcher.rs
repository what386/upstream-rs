use std::collections::HashSet;
use std::fmt;

use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::models::common::Version;
use crate::models::provider::Asset;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedAssetPatterns {
    pub match_pattern: PatternTable,
    pub exclude_pattern: PatternTable,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PatternTable {
    patterns: Vec<String>,
}

impl PatternTable {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_cli_arg(value: Option<String>) -> Self {
        value
            .map(|value| Self::from_comma_separated(&value))
            .unwrap_or_default()
    }

    pub fn from_patterns<I, S>(patterns: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for pattern in patterns {
            let normalized = normalize_pattern(pattern.as_ref());
            if !normalized.is_empty() && seen.insert(normalized.clone()) {
                out.push(normalized);
            }
        }
        Self { patterns: out }
    }

    pub fn from_comma_separated(value: &str) -> Self {
        Self::from_patterns(value.split(','))
    }

    fn from_legacy_string(value: &str) -> Self {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for chunk in value.split(',') {
            for pattern in chunk.split_whitespace() {
                let normalized = normalize_pattern(pattern);
                if !normalized.is_empty() && seen.insert(normalized.clone()) {
                    out.push(normalized);
                }
            }
        }
        Self { patterns: out }
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    pub fn as_slice(&self) -> &[String] {
        &self.patterns
    }

    pub fn match_ratio(&self, value: &str) -> f64 {
        if self.patterns.is_empty() {
            return 0.0;
        }

        let matched = self
            .patterns
            .iter()
            .filter(|pattern| pattern_matches_value(value, pattern))
            .count();
        matched as f64 / self.patterns.len() as f64
    }
}

impl fmt::Display for PatternTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.patterns.join(","))
    }
}

impl Serialize for PatternTable {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.patterns.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PatternTable {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(PatternTableVisitor)
    }
}

struct PatternTableVisitor;

impl<'de> Visitor<'de> for PatternTableVisitor {
    type Value = PatternTable;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("null, a string, or an array of pattern strings")
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(PatternTable::empty())
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(PatternTable::empty())
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(PatternTable::from_legacy_string(value))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(PatternTable::from_legacy_string(&value))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut patterns = Vec::new();
        while let Some(value) = seq.next_element::<String>()? {
            patterns.push(value);
        }
        Ok(PatternTable::from_patterns(patterns))
    }
}

pub fn pattern_match_ratio(value: &str, patterns: &PatternTable) -> f64 {
    patterns.match_ratio(value)
}

fn pattern_matches_value(value: &str, pattern: &str) -> bool {
    let value = value.to_ascii_lowercase();
    let pattern = normalize_pattern(pattern);
    if pattern.is_empty() {
        return false;
    }

    if value.contains(&pattern) {
        return true;
    }

    let value_tokens: HashSet<String> = asset_pattern_tokens(&value).into_iter().collect();
    let pattern_tokens = asset_pattern_tokens(&pattern);
    !pattern_tokens.is_empty()
        && pattern_tokens
            .iter()
            .all(|token| value_tokens.contains(token))
}

fn normalize_pattern(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn asset_pattern_tokens(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for segment in value
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '.')
        .filter(|segment| !segment.is_empty())
    {
        if is_semver_like_token(segment) {
            continue;
        }

        if segment.contains('.') {
            for part in segment.split('.') {
                push_asset_pattern_token(&mut tokens, part);
            }
        } else {
            push_asset_pattern_token(&mut tokens, segment);
        }
    }

    dedupe_preserving_order(tokens)
}

fn push_asset_pattern_token(tokens: &mut Vec<String>, value: &str) {
    let normalized = normalize_pattern(value);
    if normalized.is_empty() || is_semver_like_token(&normalized) {
        return;
    }
    tokens.push(normalized);
}

fn dedupe_preserving_order(tokens: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    tokens
        .into_iter()
        .filter(|token| seen.insert(token.clone()))
        .collect()
}

fn is_semver_like_token(token: &str) -> bool {
    let trimmed = token.strip_prefix('v').unwrap_or(token);
    trimmed.contains('.') && Version::parse(trimmed).is_ok()
}

fn pattern_tokens_for_asset(asset_name: &str) -> Vec<String> {
    asset_pattern_tokens(asset_name)
}

fn pattern_set_for_asset(asset_name: &str) -> HashSet<String> {
    pattern_tokens_for_asset(asset_name).into_iter().collect()
}

fn pattern_table_from_set(tokens: HashSet<String>) -> PatternTable {
    let mut tokens: Vec<String> = tokens.into_iter().collect();
    tokens.sort();
    PatternTable::from_patterns(tokens)
}

pub fn generate_patterns_for_asset(
    selected: &Asset,
    release_assets: &[Asset],
    package_name: &str,
) -> GeneratedAssetPatterns {
    let package_tokens: HashSet<String> =
        pattern_tokens_for_asset(package_name).into_iter().collect();
    let mut selected_set = pattern_set_for_asset(&selected.name);
    selected_set.retain(|token| !package_tokens.contains(token));

    if selected_set.is_empty() {
        selected_set.extend(asset_platform_tokens(selected));
    }

    let mut exclude_tokens = HashSet::new();
    for asset in release_assets {
        if asset.id == selected.id {
            continue;
        }

        if asset.filetype != selected.filetype {
            continue;
        }

        let mut other_tokens = pattern_set_for_asset(&asset.name);
        other_tokens.retain(|token| !package_tokens.contains(token));
        for token in other_tokens.difference(&selected_set) {
            exclude_tokens.insert(token.clone());
        }
    }

    GeneratedAssetPatterns {
        match_pattern: pattern_table_from_set(selected_set),
        exclude_pattern: pattern_table_from_set(exclude_tokens),
    }
}

fn asset_platform_tokens(asset: &Asset) -> Vec<String> {
    let mut tokens = Vec::new();
    if let Some(os) = &asset.target_os {
        tokens.push(format!("{os:?}").to_ascii_lowercase());
    }
    if let Some(arch) = &asset.target_arch {
        tokens.push(format!("{arch:?}").to_ascii_lowercase());
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::{PatternTable, generate_patterns_for_asset, pattern_match_ratio};
    use crate::models::common::Version;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::provider::{Asset, Release};
    use crate::models::upstream::Package;
    use crate::providers::asset_selector::AssetSelector;
    use chrono::Utc;

    fn make_release(assets: Vec<Asset>, prerelease: bool, tag: &str) -> Release {
        Release {
            id: 1,
            tag: tag.to_string(),
            name: tag.to_string(),
            body: String::new(),
            is_draft: false,
            is_prerelease: prerelease,
            assets,
            version: Version::new(1, 0, 0, prerelease),
            published_at: Utc::now(),
        }
    }

    #[test]
    fn pattern_match_ratio_scores_matched_tokens_as_percentage() {
        assert_eq!(
            pattern_match_ratio(
                "tool-x86_64-linux-musl.tar.gz",
                &PatternTable::from_patterns(["x86", "64", "linux", "musl"])
            ),
            1.0
        );
        assert_eq!(
            pattern_match_ratio(
                "tool-x86_64-linux-gnu.tar.gz",
                &PatternTable::from_patterns(["x86", "64", "linux", "musl"])
            ),
            3.0 / 4.0
        );
        assert_eq!(
            pattern_match_ratio(
                "tool-aarch64-darwin.tar.gz",
                &PatternTable::from_patterns(["x86", "64", "linux", "musl"])
            ),
            1.0 / 4.0
        );
    }

    #[test]
    fn cli_patterns_split_on_commas_only() {
        let table = PatternTable::from_cli_arg(Some("linux-x86_64,musl".to_string()));
        assert_eq!(table.as_slice(), ["linux-x86_64", "musl"]);
    }

    #[test]
    fn legacy_strings_split_on_whitespace_and_commas() {
        let json = r#""x86_64 linux,musl""#;
        let table: PatternTable = serde_json::from_str(json).expect("legacy table");
        assert_eq!(table.as_slice(), ["x86_64", "linux", "musl"]);
    }

    #[test]
    fn generate_patterns_for_selected_asset_keeps_stable_platform_tokens() {
        let selected = Asset::new(
            "https://example.invalid/tool-v1.2.3-x86_64-unknown-linux-musl.tar.gz".to_string(),
            1,
            "tool-v1.2.3-x86_64-unknown-linux-musl.tar.gz".to_string(),
            200_000,
            Utc::now(),
        );
        let release_assets = vec![
            selected.clone(),
            Asset::new(
                "https://example.invalid/tool-v1.2.3-aarch64-unknown-linux-musl.tar.gz".to_string(),
                2,
                "tool-v1.2.3-aarch64-unknown-linux-musl.tar.gz".to_string(),
                200_000,
                Utc::now(),
            ),
        ];

        let generated = generate_patterns_for_asset(&selected, &release_assets, "tool");
        assert!(
            generated
                .match_pattern
                .as_slice()
                .contains(&"x86".to_string())
        );
        assert!(
            generated
                .match_pattern
                .as_slice()
                .contains(&"64".to_string())
        );
        assert!(
            generated
                .match_pattern
                .as_slice()
                .contains(&"linux".to_string())
        );
        assert!(
            generated
                .match_pattern
                .as_slice()
                .contains(&"musl".to_string())
        );
        assert!(!generated.match_pattern.to_string().contains("1.2.3"));
        assert!(
            generated
                .exclude_pattern
                .as_slice()
                .contains(&"aarch64".to_string())
        );
    }

    #[test]
    fn generate_patterns_for_selected_asset_keeps_flavor_tokens() {
        let selected = Asset::new(
            "https://example.invalid/ffmpeg-release-essentials.7z".to_string(),
            1,
            "ffmpeg-release-essentials.7z".to_string(),
            200_000,
            Utc::now(),
        );
        let release_assets = vec![
            selected.clone(),
            Asset::new(
                "https://example.invalid/ffmpeg-release-full.7z".to_string(),
                2,
                "ffmpeg-release-full.7z".to_string(),
                200_000,
                Utc::now(),
            ),
            Asset::new(
                "https://example.invalid/ffmpeg-release-full-shared.7z".to_string(),
                3,
                "ffmpeg-release-full-shared.7z".to_string(),
                200_000,
                Utc::now(),
            ),
        ];

        let generated = generate_patterns_for_asset(&selected, &release_assets, "ffmpeg");
        assert!(
            generated
                .match_pattern
                .as_slice()
                .contains(&"essentials".to_string())
        );
        assert!(
            generated
                .exclude_pattern
                .as_slice()
                .contains(&"full".to_string())
        );
        assert!(
            generated
                .exclude_pattern
                .as_slice()
                .contains(&"shared".to_string())
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn generated_patterns_select_similar_asset_on_future_release() {
        let selector = AssetSelector::new();
        let selected = Asset::new(
            "https://example.invalid/tool-v1.2.3-x86_64-unknown-linux-musl.tar.gz".to_string(),
            1,
            "tool-v1.2.3-x86_64-unknown-linux-musl.tar.gz".to_string(),
            200_000,
            Utc::now(),
        );
        let generated = generate_patterns_for_asset(
            &selected,
            &[
                selected.clone(),
                Asset::new(
                    "https://example.invalid/tool-v1.2.3-x86_64-unknown-linux-gnu.tar.gz"
                        .to_string(),
                    2,
                    "tool-v1.2.3-x86_64-unknown-linux-gnu.tar.gz".to_string(),
                    200_000,
                    Utc::now(),
                ),
            ],
            "tool",
        );
        let package = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Archive,
            Some(generated.match_pattern.to_string()),
            Some(generated.exclude_pattern.to_string()),
            Channel::Stable,
            Provider::Github,
            None,
        );
        let future_release = make_release(
            vec![
                Asset::new(
                    "https://example.invalid/tool-v1.3.0-x86_64-unknown-linux-gnu.tar.gz"
                        .to_string(),
                    3,
                    "tool-v1.3.0-x86_64-unknown-linux-gnu.tar.gz".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool-v1.3.0-x86_64-unknown-linux-musl.tar.gz"
                        .to_string(),
                    4,
                    "tool-v1.3.0-x86_64-unknown-linux-musl.tar.gz".to_string(),
                    200_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.3.0",
        );

        let best = selector
            .find_recommended_asset(&future_release, &package)
            .expect("best asset");
        assert!(best.name.contains("musl"));
    }
}
