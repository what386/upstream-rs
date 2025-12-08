use serde::{Serialize, Deserialize};
use anyhow::{Context, Result, bail};


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub is_prerelease: bool,
}

#[derive(Debug, Clone)]
pub enum VersionParseError {
    Empty,
    InvalidFormat(String),
    InvalidMajor(String),
    InvalidMinor(String),
    InvalidPatch(String),
}

impl std::error::Error for VersionParseError {}

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

    pub fn parse(s: &str) -> Result<Self> {
        if s.trim().is_empty() {
            bail!(
                "Cannot parse empty version",
            );
        }

        let parts: Vec<&str> = s.split('.').collect();

        if parts.is_empty() || parts.len() > 3 {
            bail!(
                "Invalid version format",
            );
        }

        let major = match parts[0].parse::<u32>() {
            Ok(v) => v,
            Err(_) => bail!("Invalid major")
        };

        let minor = if parts.len() > 1 {
            match parts[1].parse::<u32>() {
                Ok(v) => v,
                Err(_) => bail!("Invalid minor")
            }
        } else {
            0
        };

        let patch = if parts.len() > 2 {
            match parts[2].parse::<u32>() {
                Ok(v) => v,
                Err(_) => bail!("Invalid patch")
            }
        } else {
            0
        };

        Ok(Version::new(major, minor, patch, false))
    }
}


