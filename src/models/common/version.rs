use std::{cmp::Ordering, fmt};

use anyhow::{Result, bail};
use chrono::NaiveDateTime;
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
    },
    Datetime {
        timestamp: NaiveDateTime,
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
            } => Some((*major, *minor, *patch, *is_prerelease)),
            Self::Datetime { .. } => None,
        }
    }

    pub fn datetime_components(&self) -> Option<(NaiveDateTime, &str)> {
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
            } => format!("{}-{revision}", timestamp.format(DATETIME_FORMAT)),
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

        Self::parse(trimmed)
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

    fn parse_semver(s: &str) -> Result<Self> {
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
        let timestamp = NaiveDateTime::parse_from_str(timestamp, DATETIME_FORMAT)
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
                },
                Self::Semver {
                    major: b_major,
                    minor: b_minor,
                    patch: b_patch,
                    is_prerelease: b_pre,
                },
            ) => {
                Some((a_major, a_minor, a_patch, !a_pre).cmp(&(b_major, b_minor, b_patch, !b_pre)))
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

#[derive(Serialize, Deserialize)]
struct SemverWire {
    major: u32,
    minor: u32,
    patch: u32,
    is_prerelease: bool,
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
            }
            .serialize(serializer),
            Self::Semver {
                major,
                minor,
                patch,
                is_prerelease,
            } => SemverWire {
                major: *major,
                minor: *minor,
                patch: *patch,
                is_prerelease: *is_prerelease,
            }
            .serialize(serializer),
            Self::Datetime {
                timestamp,
                revision,
            } => DatetimeWire {
                scheme: "datetime".to_string(),
                timestamp: timestamp.format(DATETIME_FORMAT).to_string(),
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
            VersionWire::Semver(value) => Ok(Self::new(
                value.major,
                value.minor,
                value.patch,
                value.is_prerelease,
            )),
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
