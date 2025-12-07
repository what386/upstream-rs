use std::str::FromStr;
use serde::{Serialize, Deserialize};

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

        return false
    }
}

#[derive(Debug, Clone)]
pub enum VersionParseError {
    Empty,
    InvalidFormat(String),
    InvalidMajor(String),
    InvalidMinor(String),
    InvalidPatch(String),
}

impl std::fmt::Display for VersionParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionParseError::Empty => write!(f, "Version string is empty"),
            VersionParseError::InvalidFormat(s) => write!(f, "Invalid version format: {}", s),
            VersionParseError::InvalidMajor(s) => write!(f, "Invalid major version: {}", s),
            VersionParseError::InvalidMinor(s) => write!(f, "Invalid minor version: {}", s),
            VersionParseError::InvalidPatch(s) => write!(f, "Invalid patch version: {}", s),
        }
    }
}

impl std::error::Error for VersionParseError {}

impl FromStr for Version {
    type Err = VersionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.trim().is_empty() {
            return Err(VersionParseError::Empty);
        }

        let parts: Vec<&str> = s.split('.').collect();

        if parts.is_empty() || parts.len() > 3 {
            return Err(VersionParseError::InvalidFormat(s.to_string()));
        }

        let major = parts[0]
            .parse::<u32>()
            .map_err(|_| VersionParseError::InvalidMajor(parts[0].to_string()))?;

        let minor = if parts.len() > 1 {
            parts[1]
                .parse::<u32>()
                .map_err(|_| VersionParseError::InvalidMinor(parts[1].to_string()))?
        } else {
            0
        };

        let patch = if parts.len() > 2 {
            parts[2]
                .parse::<u32>()
                .map_err(|_| VersionParseError::InvalidPatch(parts[2].to_string()))?
        } else {
            0
        };

        Ok(Version::new(major, minor, patch, false))
    }
}

