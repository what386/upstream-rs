use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::{
    models::common::enums::{Filetype, Provider, TrustMode},
    routines::build::BuildProfile,
};

const SUPPORTED_INDEX_VERSION: u64 = 1;

#[derive(Debug, Deserialize)]
pub(super) struct RegistryIndex {
    pub(super) version: u64,
    pub(super) packages: BTreeMap<String, RegistryPackage>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RegistryPackage {
    pub(super) revision: u64,
    #[serde(default)]
    pub(super) binary: Option<String>,
    pub(super) desktop: bool,
    pub(super) trust: RegistryTrust,
    #[serde(default)]
    pub(super) r#match: Vec<String>,
    #[serde(default)]
    pub(super) exclude: Vec<String>,
    pub(super) install: RegistryInstall,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(super) enum RegistryInstall {
    Release {
        repo: String,
        provider: RegistryProvider,
    },
    Build {
        repo: String,
        provider: RegistryProvider,
        #[serde(default)]
        profile: Option<RegistryBuildProfile>,
        #[serde(default)]
        branch: Option<String>,
    },
    Http {
        url: String,
        #[serde(default)]
        filetype: RegistryFiletype,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum RegistryProvider {
    Github,
    Gitlab,
    Gitea,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum RegistryTrust {
    None,
    BestEffort,
    Checksum,
    Signature,
    All,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum RegistryBuildProfile {
    Rust,
    Dotnet,
    Go,
    Zig,
    Cmake,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum RegistryFiletype {
    Appimage,
    MacApp,
    MacDmg,
    Archive,
    Compressed,
    Binary,
    WinExe,
    #[default]
    Auto,
}

impl RegistryProvider {
    pub(super) fn provider(&self) -> Provider {
        match self {
            Self::Github => Provider::Github,
            Self::Gitlab => Provider::Gitlab,
            Self::Gitea => Provider::Gitea,
        }
    }
}

impl RegistryTrust {
    pub(super) fn trust_mode(&self) -> TrustMode {
        match self {
            Self::None => TrustMode::None,
            Self::BestEffort => TrustMode::BestEffort,
            Self::Checksum => TrustMode::Checksum,
            Self::Signature => TrustMode::Signature,
            Self::All => TrustMode::All,
        }
    }
}

impl RegistryBuildProfile {
    pub(super) fn build_profile(&self) -> BuildProfile {
        match self {
            Self::Rust => BuildProfile::Rust,
            Self::Dotnet => BuildProfile::Dotnet,
            Self::Go => BuildProfile::Go,
            Self::Zig => BuildProfile::Zig,
            Self::Cmake => BuildProfile::Cmake,
        }
    }
}

impl RegistryFiletype {
    pub(super) fn filetype(&self) -> Filetype {
        match self {
            Self::Appimage => Filetype::AppImage,
            Self::MacApp => Filetype::MacApp,
            Self::MacDmg => Filetype::MacDmg,
            Self::Archive => Filetype::Archive,
            Self::Compressed => Filetype::Compressed,
            Self::Binary => Filetype::Binary,
            Self::WinExe => Filetype::WinExe,
            Self::Auto => Filetype::Auto,
        }
    }
}

pub(super) fn parse_index(bytes: &[u8]) -> Result<RegistryIndex> {
    let index: RegistryIndex =
        serde_json::from_slice(bytes).context("Failed to parse registry index JSON")?;
    if index.version != SUPPORTED_INDEX_VERSION {
        bail!(
            "Unsupported registry index version {}; this build supports version {}",
            index.version,
            SUPPORTED_INDEX_VERSION
        );
    }
    if let Some((name, _)) = index
        .packages
        .iter()
        .find(|(_, package)| package.revision == 0)
    {
        bail!("Registry package '{name}' has invalid revision 0");
    }
    let mut installed_names = BTreeMap::new();
    for (name, package) in &index.packages {
        let installed_name = package.binary.as_deref().unwrap_or(name);
        validate_binary_name(installed_name)
            .with_context(|| format!("Registry package '{name}' has an invalid binary name"))?;
        if let Some(previous) = installed_names.insert(installed_name, name) {
            bail!("Registry packages '{previous}' and '{name}' both install as '{installed_name}'");
        }
    }
    Ok(index)
}

fn validate_binary_name(name: &str) -> Result<()> {
    let valid = !name.is_empty()
        && name != "."
        && name != ".."
        && name.trim() == name
        && !name.contains('/')
        && !name.contains('\\')
        && !name.chars().any(char::is_control)
        && !name.to_ascii_lowercase().ends_with(".exe");
    if !valid {
        bail!("expected a safe command basename without path separators or a platform extension");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{RegistryInstall, parse_index};

    #[test]
    fn validates_schema_and_supported_recipes() {
        let index = parse_index(br#"{"version":1,"packages":{"source":{"revision":1,"desktop":false,"trust":"best-effort","install":{"type":"build","repo":"o/source","provider":"github","profile":"rust"}},"direct":{"revision":1,"desktop":false,"trust":"checksum","install":{"type":"http","url":"https://example.com/tool","filetype":"binary"}}}}"#).expect("valid recipes");
        assert!(matches!(
            index.packages["source"].install,
            RegistryInstall::Build { .. }
        ));
        assert!(matches!(
            index.packages["direct"].install,
            RegistryInstall::Http { .. }
        ));
    }

    #[test]
    fn rejects_invalid_versions_revisions_and_binary_paths() {
        assert!(
            parse_index(br#"{"version":2,"packages":{}}"#)
                .unwrap_err()
                .to_string()
                .contains("Unsupported")
        );
        assert!(parse_index(br#"{"version":1,"packages":{"tool":{"revision":0,"desktop":false,"trust":"checksum","install":{"type":"release","repo":"o/tool","provider":"github"}}}}"#).unwrap_err().to_string().contains("revision 0"));
        assert!(parse_index(br#"{"version":1,"packages":{"tool":{"revision":1,"binary":"../Tool","desktop":false,"trust":"checksum","install":{"type":"release","repo":"o/tool","provider":"github"}}}}"#).unwrap_err().to_string().contains("invalid binary"));
    }

    #[test]
    fn accepts_uppercase_and_spaces_but_rejects_duplicate_installed_names() {
        parse_index(br#"{"version":1,"packages":{"audacity":{"revision":1,"binary":"Audacity App","desktop":true,"trust":"checksum","install":{"type":"release","repo":"o/audacity","provider":"github"}}}}"#).expect("safe basename");
        assert!(parse_index(br#"{"version":1,"packages":{"one":{"revision":1,"binary":"tool","desktop":false,"trust":"checksum","install":{"type":"release","repo":"o/one","provider":"github"}},"two":{"revision":1,"binary":"tool","desktop":false,"trust":"checksum","install":{"type":"release","repo":"o/two","provider":"github"}}}}"#).unwrap_err().to_string().contains("both install"));
    }
}
