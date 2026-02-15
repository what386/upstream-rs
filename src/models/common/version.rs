use std::fmt;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub is_prerelease: bool,
}

impl Version {
    pub fn new(major: u32, minor: u32, patch: u32, is_prerelease: bool) -> Self {
        Self {
            major,
            minor,
            patch,
            is_prerelease,
        }
    }

    pub fn is_newer_than(&self, other: &Version) -> bool {
        if self.major != other.major {
            return self.major > other.major;
        }
        if self.minor != other.minor {
            return self.minor > other.minor;
        }
        if self.patch != other.patch {
            return self.patch > other.patch;
        }
        if self.is_prerelease != other.is_prerelease {
            return !self.is_prerelease;
        }

        false
    }

    pub fn parse(s: &str) -> Result<Self> {
        if s.trim().is_empty() {
            bail!("Cannot parse empty version",);
        }

        let parts: Vec<&str> = s.split('.').collect();

        if parts.is_empty() || parts.len() > 3 {
            bail!("Invalid version format",);
        }

        let major = match parts[0].parse::<u32>() {
            Ok(v) => v,
            Err(_) => bail!("Invalid major"),
        };

        let minor = if parts.len() > 1 {
            match parts[1].parse::<u32>() {
                Ok(v) => v,
                Err(_) => bail!("Invalid minor"),
            }
        } else {
            0
        };

        let patch = if parts.len() > 2 {
            match parts[2].parse::<u32>() {
                Ok(v) => v,
                Err(_) => bail!("Invalid patch"),
            }
        } else {
            0
        };

        Ok(Version::new(major, minor, patch, false))
    }

    pub fn from_filename(s: &str) -> Result<Self> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            bail!("Cannot parse empty version");
        }

        if let Some(candidate) = Self::find_triplet(trimmed) {
            return Self::parse(&candidate);
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

        Self::from_filename(cleaned).or_else(|_| Self::parse(cleaned))
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
            if patch_start == i {
                continue;
            }

            return Some(s[major_start..i].to_string());
        }

        None
    }

    pub fn cmp(&self, other: &Version) -> std::cmp::Ordering {
        match self.major.cmp(&other.major) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.minor.cmp(&other.minor) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.patch.cmp(&other.patch) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }

        // Stable releases are "greater than" prereleases for the same version
        match (self.is_prerelease, other.is_prerelease) {
            (false, true) => std::cmp::Ordering::Greater,
            (true, false) => std::cmp::Ordering::Less,
            _ => std::cmp::Ordering::Equal,
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_prerelease {
            write!(f, "{}.{}.{}-pre", self.major, self.minor, self.patch)
        } else {
            write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
        }
    }
}

// Implement PartialOrd and Ord for proper comparison
impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        Version::cmp(self, other)
    }
}
