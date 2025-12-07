use serde::{Serialize, Deserialize};

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
