use std::{cmp::Ordering, fmt};

use anyhow::{Result, bail};
use jiff::civil::DateTime;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

const DATETIME_FORMAT: &str = "%Y%m%d-%H%M%S";
const MIN_REVISION_LEN: usize = 7;
const MAX_REVISION_LEN: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Version {
    Unknown,
    Semver {
        major: u32,
        minor: u32,
        patch: u32,
        is_prerelease: bool,
        full: Option<String>,
    },
    Datetime {
        timestamp: DateTime,
        revision: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionTagTemplate(String);

impl VersionTagTemplate {
    pub fn from_tag(tag: &str, version: &Version) -> Option<Self> {
        if version.is_unknown() {
            return None;
        }

        let version_text = version.core_string();
        let index = tag.find(&version_text)?;
        let suffix_start = index + version_text.len();
        Some(Self(format!(
            "{}{{}}{}",
            &tag[..index],
            &tag[suffix_start..]
        )))
    }

    pub fn parse(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        value.contains("{}").then_some(Self(value))
    }

    pub fn render(&self, version: &Version) -> String {
        self.0.replacen("{}", &version.core_string(), 1)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Version {
    pub fn new(major: u32, minor: u32, patch: u32, is_prerelease: bool) -> Self {
        if major == 0 && minor == 0 && patch == 0 && !is_prerelease {
            Self::Unknown
        } else {
            Self::Semver {
                major,
                minor,
                patch,
                is_prerelease,
                full: None,
            }
        }
    }

    pub fn is_newer_than(&self, other: &Version) -> bool {
        self.partial_cmp(other) == Some(Ordering::Greater)
    }

    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }

    pub fn is_prerelease(&self) -> bool {
        matches!(
            self,
            Self::Semver {
                is_prerelease: true,
                ..
            }
        )
    }

    pub fn semver_components(&self) -> Option<(u32, u32, u32, bool)> {
        match self {
            Self::Unknown => Some((0, 0, 0, false)),
            Self::Semver {
                major,
                minor,
                patch,
                is_prerelease,
                ..
            } => Some((*major, *minor, *patch, *is_prerelease)),
            Self::Datetime { .. } => None,
        }
    }

    pub fn datetime_components(&self) -> Option<(DateTime, &str)> {
        match self {
            Self::Datetime {
                timestamp,
                revision,
            } => Some((*timestamp, revision)),
            _ => None,
        }
    }

    pub fn core_string(&self) -> String {
        match self {
            Self::Unknown => "0.0.0".to_string(),
            Self::Semver {
                major,
                minor,
                patch,
                ..
            } => format!("{major}.{minor}.{patch}"),
            Self::Datetime {
                timestamp,
                revision,
            } => format!("{}-{revision}", timestamp.strftime(DATETIME_FORMAT)),
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            bail!("Cannot parse empty version");
        }

        Self::parse_semver(trimmed).or_else(|_| Self::parse_datetime(trimmed))
    }

    pub fn from_filename(s: &str) -> Result<Self> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            bail!("Cannot parse empty version");
        }

        if let Some(candidate) = Self::find_triplet(trimmed) {
            return Self::parse_semver(&candidate);
        }

        if let Some(candidate) = Self::find_datetime(trimmed) {
            return Self::parse_datetime(&candidate);
        }

        if let Some((version, _)) = Self::underscore_filename_version(trimmed) {
            return Ok(version);
        }

        Self::parse(trimmed)
    }

    /// Locate underscore-separated release numbers, excluding architecture pairs
    /// such as x86_64. Calendar releases may have only a year and minor number.
    pub(crate) fn underscore_filename_version(s: &str) -> Option<(Self, std::ops::Range<usize>)> {
        let bytes = s.as_bytes();
        let mut start = 0;
        while start < bytes.len() {
            if !bytes[start].is_ascii_digit() {
                start += 1;
                continue;
            }

            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'_') {
                end += 1;
            }

            let run = s[start..end].trim_end_matches('_');
            let parts = run.split('_').collect::<Vec<_>>();
            let year_index = parts.iter().enumerate().find_map(|(index, part)| {
                let year = part.parse::<u32>().ok()?;
                ((1900..3000).contains(&year) && (2..=3).contains(&(parts.len() - index)))
                    .then_some(index)
            });

            let index = year_index.or_else(|| (parts.len() >= 3).then(|| parts.len() - 3));
            if let Some(index) = index {
                let offset = parts[..index]
                    .iter()
                    .map(|part| part.len() + 1)
                    .sum::<usize>();

                let value = parts[index..].join(".");
                if let Ok(version) = Self::parse_semver(&value) {
                    return Some((version, start + offset..start + run.len()));
                }
            }

            start = end;
        }

        None
    }

    pub fn from_tag(tag: &str) -> Result<Self> {
        let tag = tag.trim();
        let tag = tag
            .strip_prefix('v')
            .or_else(|| tag.strip_prefix('V'))
            .unwrap_or(tag);

        const PREFIXES: &[&str] = &["release-", "rel-", "ver-", "version-"];
        let lowered = tag.to_lowercase();
        let cleaned = PREFIXES
            .iter()
            .find_map(|prefix| lowered.strip_prefix(prefix).map(|_| &tag[prefix.len()..]))
            .unwrap_or(tag);

        Self::from_filename(cleaned)
    }

    /// Exact SemVer parsing for registry versions, without forge prefixes.
    pub fn parse_strict_semver(s: &str) -> Result<Self> {
        let (without_build, build) = s.split_once('+').map_or((s, None), |(v, b)| (v, Some(b)));
        let (core, pre) = without_build
            .split_once('-')
            .map_or((without_build, None), |(v, p)| (v, Some(p)));

        let parts = core.split('.').collect::<Vec<_>>();
        if parts.len() != 3 {
            bail!("Expected MAJOR.MINOR.PATCH");
        }

        let number = |part: &str| -> Result<u32> {
            if part.is_empty()
                || !part.bytes().all(|b| b.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
            {
                bail!("Invalid SemVer numeric component");
            }

            Ok(part.parse()?)
        };

        for (identifiers, prerelease) in [(pre, true), (build, false)] {
            if let Some(identifiers) = identifiers {
                for id in identifiers.split('.') {
                    if id.is_empty()
                        || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                        || (prerelease
                            && id.len() > 1
                            && id.starts_with('0')
                            && id.bytes().all(|b| b.is_ascii_digit()))
                    {
                        bail!("Invalid SemVer identifier");
                    }
                }
            }
        }

        let (major, minor, patch) = (number(parts[0])?, number(parts[1])?, number(parts[2])?);
        let full = (pre.is_some() || build.is_some() || (major, minor, patch) == (0, 0, 0))
            .then(|| s.to_owned());

        Ok(Self::Semver {
            major,
            minor,
            patch,
            is_prerelease: pre.is_some(),
            full,
        })
    }

    pub fn full_semver(&self) -> Option<&str> {
        match self {
            Self::Semver { full, .. } => full.as_deref(),
            _ => None,
        }
    }

    fn parse_semver(s: &str) -> Result<Self> {
        if s.contains(['-', '+']) {
            return Self::parse_strict_semver(s);
        }

        let parts: Vec<&str> = s.split('.').collect();
        if parts.is_empty() || parts.len() > 3 {
            bail!("Invalid version format");
        }

        let major = parts[0]
            .parse::<u32>()
            .map_err(|_| anyhow::anyhow!("Invalid major"))?;

        let minor = parts
            .get(1)
            .map(|value| value.parse::<u32>())
            .transpose()
            .map_err(|_| anyhow::anyhow!("Invalid minor"))?
            .unwrap_or(0);

        let patch = parts
            .get(2)
            .map(|value| value.parse::<u32>())
            .transpose()
            .map_err(|_| anyhow::anyhow!("Invalid patch"))?
            .unwrap_or(0);

        Ok(Self::new(major, minor, patch, false))
    }

    fn parse_datetime(s: &str) -> Result<Self> {
        if s.len() < 8 + 1 + 6 + 1 + MIN_REVISION_LEN {
            bail!("Invalid datetime version format");
        }

        let Some((timestamp, revision)) = s.rsplit_once('-') else {
            bail!("Invalid datetime version format");
        };

        if revision.len() < MIN_REVISION_LEN
            || revision.len() > MAX_REVISION_LEN
            || !revision.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            bail!("Invalid datetime revision");
        }

        let timestamp = DateTime::strptime(DATETIME_FORMAT, timestamp)
            .map_err(|_| anyhow::anyhow!("Invalid datetime timestamp"))?;

        Ok(Self::Datetime {
            timestamp,
            revision: revision.to_string(),
        })
    }

    fn find_triplet(s: &str) -> Option<String> {
        let bytes = s.as_bytes();
        let len = bytes.len();
        let mut i = 0;
        while i < len {
            if !bytes[i].is_ascii_digit() {
                i += 1;
                continue;
            }

            let major_start = i;
            while i < len && bytes[i].is_ascii_digit() {
                i += 1;
            }

            if i >= len || bytes[i] != b'.' {
                continue;
            }

            i += 1;
            let minor_start = i;
            while i < len && bytes[i].is_ascii_digit() {
                i += 1;
            }

            if minor_start == i || i >= len || bytes[i] != b'.' {
                continue;
            }

            i += 1;
            let patch_start = i;
            while i < len && bytes[i].is_ascii_digit() {
                i += 1;
            }

            if patch_start != i {
                return Some(s[major_start..i].to_string());
            }
        }

        None
    }

    fn find_datetime(s: &str) -> Option<String> {
        let bytes = s.as_bytes();
        let minimum_len = 8 + 1 + 6 + 1 + MIN_REVISION_LEN;
        for start in 0..bytes.len().saturating_sub(minimum_len - 1) {
            if !bytes[start..start + 8].iter().all(u8::is_ascii_digit)
                || bytes.get(start + 8) != Some(&b'-')
                || !bytes[start + 9..start + 15].iter().all(u8::is_ascii_digit)
                || bytes.get(start + 15) != Some(&b'-')
            {
                continue;
            }

            let revision_start = start + 16;
            let mut end = revision_start;
            while end < bytes.len()
                && bytes[end].is_ascii_hexdigit()
                && end - revision_start < MAX_REVISION_LEN
            {
                end += 1;
            }

            if end - revision_start < MIN_REVISION_LEN
                || (end < bytes.len()
                    && bytes[end].is_ascii_hexdigit()
                    && end - revision_start == MAX_REVISION_LEN)
            {
                continue;
            }

            let candidate = &s[start..end];
            if Self::parse_datetime(candidate).is_ok() {
                return Some(candidate.to_string());
            }
        }

        None
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Semver {
                full: Some(full), ..
            } => write!(f, "{full}"),
            Self::Semver {
                is_prerelease: true,
                ..
            } => write!(f, "{}-pre", self.core_string()),
            _ => write!(f, "{}", self.core_string()),
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (
                Self::Semver {
                    major: a_major,
                    minor: a_minor,
                    patch: a_patch,
                    is_prerelease: a_pre,
                    full: a_full,
                },
                Self::Semver {
                    major: b_major,
                    minor: b_minor,
                    patch: b_patch,
                    is_prerelease: b_pre,
                    full: b_full,
                },
            ) => {
                let core =
                    (a_major, a_minor, a_patch, !a_pre).cmp(&(b_major, b_minor, b_patch, !b_pre));

                if core != Ordering::Equal || !a_pre {
                    return Some(core);
                }

                match (
                    a_full.as_deref().and_then(prerelease),
                    b_full.as_deref().and_then(prerelease),
                ) {
                    (Some(a), Some(b)) => Some(compare_prerelease(a, b)),
                    _ => Some(Ordering::Equal),
                }
            }
            (
                Self::Datetime {
                    timestamp: a_timestamp,
                    revision: a_revision,
                },
                Self::Datetime {
                    timestamp: b_timestamp,
                    revision: b_revision,
                },
            ) => match a_timestamp.cmp(b_timestamp) {
                Ordering::Equal if a_revision != b_revision => None,
                ordering => Some(ordering),
            },
            (Self::Unknown, Self::Unknown) => None,
            _ => None,
        }
    }
}

fn prerelease(version: &str) -> Option<&str> {
    version
        .split('+')
        .next()?
        .split_once('-')
        .map(|(_, pre)| pre)
}

fn compare_prerelease(a: &str, b: &str) -> Ordering {
    let (a, b) = (
        a.split('.').collect::<Vec<_>>(),
        b.split('.').collect::<Vec<_>>(),
    );

    for (a, b) in a.iter().zip(&b) {
        let numeric = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
        let order = match (numeric(a), numeric(b)) {
            (true, true) => a.len().cmp(&b.len()).then_with(|| a.cmp(b)),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => a.cmp(b),
        };

        if order != Ordering::Equal {
            return order;
        }
    }

    a.len().cmp(&b.len())
}

#[derive(Serialize, Deserialize)]
struct SemverWire {
    major: u32,
    minor: u32,
    patch: u32,
    is_prerelease: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    full: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct DatetimeWire {
    scheme: String,
    timestamp: String,
    revision: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum VersionWire {
    Semver(SemverWire),
    Datetime(DatetimeWire),
}

impl Serialize for Version {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Unknown => SemverWire {
                major: 0,
                minor: 0,
                patch: 0,
                is_prerelease: false,
                full: None,
            }
            .serialize(serializer),
            Self::Semver {
                major,
                minor,
                patch,
                is_prerelease,
                full,
            } => SemverWire {
                major: *major,
                minor: *minor,
                patch: *patch,
                is_prerelease: *is_prerelease,
                full: full.clone(),
            }
            .serialize(serializer),
            Self::Datetime {
                timestamp,
                revision,
            } => DatetimeWire {
                scheme: "datetime".to_string(),
                timestamp: timestamp.strftime(DATETIME_FORMAT).to_string(),
                revision: revision.clone(),
            }
            .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match VersionWire::deserialize(deserializer)? {
            VersionWire::Semver(value) => {
                if let Some(full) = value.full {
                    Self::parse_strict_semver(&full).map_err(serde::de::Error::custom)
                } else {
                    Ok(Self::new(
                        value.major,
                        value.minor,
                        value.patch,
                        value.is_prerelease,
                    ))
                }
            }
            VersionWire::Datetime(value) => {
                if value.scheme != "datetime" {
                    return Err(serde::de::Error::custom("unsupported version scheme"));
                }

                Self::parse_datetime(&format!("{}-{}", value.timestamp, value.revision))
                    .map_err(serde::de::Error::custom)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::{Version, VersionTagTemplate};

    #[test]
    fn parse_supports_short_full_and_datetime_versions() {
        assert_eq!(
            Version::parse("1").expect("parse 1"),
            Version::new(1, 0, 0, false)
        );

        assert_eq!(
            Version::parse("1.2").expect("parse 1.2"),
            Version::new(1, 2, 0, false)
        );

        assert_eq!(
            Version::parse("1.2.3").expect("parse 1.2.3"),
            Version::new(1, 2, 3, false)
        );

        assert_eq!(
            Version::parse("20240203-110809-5046fc22")
                .expect("parse datetime")
                .to_string(),
            "20240203-110809-5046fc22"
        );
    }

    #[test]
    fn parse_rejects_invalid_versions() {
        for invalid in [
            "",
            "1.2.3.4",
            "v1.2.3",
            "1.a.3",
            "20240230-110809-5046fc22",
            "20240203-250809-5046fc22",
            "20240203-110809-nothex!",
            "20240203-110809-123456",
        ] {
            assert!(Version::parse(invalid).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn extraction_prefers_semver_and_supports_datetime_wrappers() {
        assert_eq!(
            Version::from_filename("tool-v2.15.9-linux-x86_64.tar.gz")
                .expect("semver")
                .to_string(),
            "2.15.9"
        );

        assert_eq!(
            Version::from_filename("tool-20240203-110809-5046fc22-linux.tar.gz")
                .expect("datetime")
                .to_string(),
            "20240203-110809-5046fc22"
        );

        assert_eq!(
            Version::from_tag("v20240203-110809-ABCDEF1")
                .expect("wrapped datetime")
                .to_string(),
            "20240203-110809-ABCDEF1"
        );
    }

    #[test]
    fn extracts_underscore_release_versions_without_architecture_or_model_numbers() {
        for (name, expected) in [
            (
                "LimelightHardwareManagerSetup2_0_10.exe",
                Version::new(2, 0, 10, false),
            ),
            (
                "LimelightHardwareManager-x86_64_2_0_10.AppImage.zip",
                Version::new(2, 0, 10, false),
            ),
            ("limelight2_2024_10_2.zip", Version::new(2024, 10, 2, false)),
            ("limelight3_2026_1.zip", Version::new(2026, 1, 0, false)),
            (
                "limelight2_2025_1_release.zip",
                Version::new(2025, 1, 0, false),
            ),
        ] {
            assert_eq!(Version::from_filename(name).unwrap(), expected, "{name}");
        }

        assert!(Version::from_filename("tool-x86_64.zip").is_err());
        assert!(Version::from_filename("tool-x86_32.zip").is_err());
    }

    #[test]
    fn comparisons_are_scheme_aware() {
        let stable = Version::new(1, 0, 0, false);
        let prerelease = Version::new(1, 0, 0, true);
        assert_eq!(stable.partial_cmp(&prerelease), Some(Ordering::Greater));

        let older = Version::parse("20240203-110809-5046fc22").expect("older");
        let newer = Version::parse("20240204-110809-5046fc22").expect("newer");
        let collision = Version::parse("20240203-110809-aaaaaaaa").expect("collision");
        assert_eq!(newer.partial_cmp(&older), Some(Ordering::Greater));
        assert_eq!(older.partial_cmp(&collision), None);
        assert_eq!(older.partial_cmp(&stable), None);
    }

    #[test]
    fn serde_preserves_legacy_semver_shape_and_datetime_data() {
        let semver = Version::new(1, 2, 3, true);
        let json = serde_json::to_value(&semver).expect("serialize semver");
        assert_eq!(json["major"], 1);
        assert!(json.get("scheme").is_none());
        assert_eq!(
            serde_json::from_value::<Version>(json).expect("read semver"),
            semver
        );

        let datetime = Version::parse("20240203-110809-5046fc22").expect("datetime");
        let json = serde_json::to_value(&datetime).expect("serialize datetime");
        assert_eq!(json["scheme"], "datetime");
        assert_eq!(json["timestamp"], "20240203-110809");
        assert_eq!(
            serde_json::from_value::<Version>(json).expect("read datetime"),
            datetime
        );
    }

    #[test]
    fn tag_template_infers_wrappers_and_renders_another_version() {
        let installed = Version::new(1, 2, 3, false);
        let requested = Version::new(2, 4, 6, false);
        let template =
            VersionTagTemplate::from_tag("rust-v1.2.3-linux", &installed).expect("template");

        assert_eq!(template.as_str(), "rust-v{}-linux");
        assert_eq!(template.render(&requested), "rust-v2.4.6-linux");
    }
}

#[cfg(test)]
mod strict_semver_tests {
    use super::*;

    #[test]
    fn strict_versions_round_trip_and_follow_semver_precedence() {
        let ordered = [
            "0.0.0",
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-beta.2",
            "1.0.0-beta.10",
            "1.0.0-rc.1",
            "1.0.0",
        ];

        for pair in ordered.windows(2) {
            assert!(
                Version::parse_strict_semver(pair[0]).unwrap()
                    < Version::parse_strict_semver(pair[1]).unwrap()
            );
        }

        for text in ordered.into_iter().chain(["1.0.0-beta.2+build.01"]) {
            let version = Version::parse_strict_semver(text).unwrap();
            assert_eq!(version.to_string(), text);
            assert!(!version.is_unknown());
            assert_eq!(
                serde_json::from_str::<Version>(&serde_json::to_string(&version).unwrap()).unwrap(),
                version
            );
        }

        assert!(Version::new(0, 0, 0, false).is_unknown());
        assert_eq!(
            Version::parse_strict_semver("1.0.0+a")
                .unwrap()
                .partial_cmp(&Version::parse_strict_semver("1.0.0+b").unwrap()),
            Some(Ordering::Equal)
        );
    }

    #[test]
    fn strict_parser_rejects_prefixes_short_versions_and_leading_zeroes() {
        for text in [
            "1",
            "1.0",
            "v1.0.0",
            "01.0.0",
            "1.00.0",
            "1.0.00",
            "1.0.0-01",
            "1.0.0-",
            "1.0.0+",
            "1.0.0-a..b",
            "1.0.0+a+b",
            " 1.0.0",
            "1.0.0-α",
        ] {
            assert!(Version::parse_strict_semver(text).is_err(), "{text}");
        }
    }
}
