use serde::{Serialize, Deserialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Filetype {
    AppImage,
    Binary,
    Compressed,
    Archive,
    Script,
    WinExe,
    Checksum,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Channel {
    Stable,
    Beta,
    Nightly,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    Github,
}

impl fmt::Display for Channel{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Channel::Stable => write!(f, "Stable"),
            Channel::Beta => write!(f, "Beta"),
            Channel::Nightly => write!(f, "Nightly"),
            Channel::All => write!(f, "All"),
        }
    }
}

impl fmt::Display for Provider{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Provider::Github => write!(f, "Github"),
        }
    }
}
