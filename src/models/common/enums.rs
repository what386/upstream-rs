use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum BuildProfile {
    Rust,
    Dotnet,
    Go,
    Zig,
    Cmake,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum, Hash)]
pub enum Filetype {
    AppImage,
    Archive,
    Compressed,
    Binary,
    WinExe,
    Checksum,
    Auto, // select automatically
}

impl fmt::Display for Filetype {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Filetype::AppImage => "AppImage",
            Filetype::Archive => "Compressed archive",
            Filetype::Compressed => "Compressed file",
            Filetype::Binary => "Binary",
            Filetype::WinExe => "Windows executable",
            Filetype::Checksum => "Checksum",
            Filetype::Auto => "Auto",
        };

        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum Channel {
    Stable,
    Preview,
    Nightly,
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Channel::Stable => write!(f, "Stable"),
            Channel::Preview => write!(f, "Preview"),
            Channel::Nightly => write!(f, "Nightly"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum TrustMode {
    None,
    BestEffort,
    Checksum,
    Signature,
    All,
}

impl fmt::Display for TrustMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TrustMode::None => "none",
            TrustMode::BestEffort => "best-effort",
            TrustMode::Checksum => "checksum",
            TrustMode::Signature => "signature",
            TrustMode::All => "all",
        };

        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    Github,
    Gitlab,
    Gitea,
    AppImageHub,
    WebScraper,
    Direct,
}

impl Provider {
    /// Whether this provider represents a source-code forge.
    /// Keep this exhaustive so every new provider must declare its classification.
    pub fn is_forge(&self) -> bool {
        match self {
            Self::Github => true,
            Self::Gitlab => true,
            Self::Gitea => true,
            Self::AppImageHub => false,
            Self::WebScraper => false,
            Self::Direct => false,
        }
    }
}

impl FromStr for Provider {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "github" => Ok(Provider::Github),
            "gitlab" => Ok(Provider::Gitlab),
            "gitea" => Ok(Provider::Gitea),
            "appimagehub" => Ok(Provider::AppImageHub),
            "scraper" => Ok(Provider::WebScraper),
            "direct" => Ok(Provider::Direct),
            _ => Err(format!("Unknown provider: {}", s)),
        }
    }
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Provider::Github => write!(f, "github"),
            Provider::Gitlab => write!(f, "gitlab"),
            Provider::Gitea => write!(f, "gitea"),
            Provider::AppImageHub => write!(f, "appimagehub"),
            Provider::WebScraper => write!(f, "scraper"),
            Provider::Direct => write!(f, "direct"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CompressionLevel {
    None,
    Low,
    #[default]
    High,
}
