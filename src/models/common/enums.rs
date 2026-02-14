use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
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
    Nightly,
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Channel::Stable => write!(f, "Stable"),
            Channel::Nightly => write!(f, "Nightly"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    Github,
    Gitlab,
    Gitea,
    Http,
    Direct,
}

impl FromStr for Provider {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "github" => Ok(Provider::Github),
            "gitlab" => Ok(Provider::Gitlab),
            "gitea" => Ok(Provider::Gitea),
            "http" => Ok(Provider::Http),
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
            Provider::Http => write!(f, "http"),
            Provider::Direct => write!(f, "direct"),
        }
    }
}
