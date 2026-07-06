use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use crate::{
    models::common::enums::Filetype,
    models::{
        provider::{Asset, Release},
        upstream::Package,
    },
    providers::provider_manager::ProviderManager,
    utils::platform::platform_info::{ArchitectureInfo, CpuArch, OSKind},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DotSlashAsset {
    pub dotslash_name: String,
    pub platform: DotSlashPlatform,
    pub filename: String,
    pub url: String,
    pub size: u64,
    pub hash: String,
    pub digest: String,
    pub filetype: Filetype,
    pub format: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DotSlashPlatform {
    pub key: String,
    pub os: OSKind,
    pub arch: CpuArch,
}

#[derive(Debug, Deserialize)]
struct DotSlashFile {
    name: String,
    platforms: BTreeMap<String, DotSlashPlatformEntry>,
}

#[derive(Debug, Deserialize)]
struct DotSlashPlatformEntry {
    size: u64,
    hash: String,
    digest: String,
    format: String,
    path: String,
    providers: Vec<DotSlashProvider>,
}

#[derive(Debug, Deserialize)]
struct DotSlashProvider {
    url: Option<String>,
}

pub fn select_asset(contents: &str) -> Result<DotSlashAsset> {
    select_asset_for_architecture(contents, &ArchitectureInfo::new())
}

pub fn select_asset_filename(contents: &str) -> Result<String> {
    Ok(select_asset(contents)?.filename)
}

pub fn find_asset<'a>(release: &'a Release, package: &Package) -> Option<&'a Asset> {
    release
        .assets
        .iter()
        .find(|asset| is_asset(&asset.name, package))
}

pub fn is_asset(asset_name: &str, package: &Package) -> bool {
    let path = Path::new(asset_name);
    if path.extension().is_some() {
        return false;
    }

    let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    let repo_name = package.repo_slug.rsplit('/').next().unwrap_or("");
    filename.eq_ignore_ascii_case(repo_name) || filename.eq_ignore_ascii_case(&package.name)
}

pub async fn resolve_selected_asset<H>(
    package: &Package,
    asset: &Asset,
    provider_manager: &ProviderManager,
    download_cache: &Path,
    message_callback: Option<&mut H>,
) -> Result<Option<Asset>>
where
    H: FnMut(&str),
{
    if !is_asset(&asset.name, package) {
        return Ok(None);
    }

    resolve_asset(
        package,
        asset,
        provider_manager,
        download_cache,
        message_callback,
    )
    .await
}

pub async fn resolve_asset<H>(
    package: &Package,
    descriptor: &Asset,
    provider_manager: &ProviderManager,
    download_cache: &Path,
    mut message_callback: Option<&mut H>,
) -> Result<Option<Asset>>
where
    H: FnMut(&str),
{
    if let Some(cb) = message_callback.as_mut() {
        cb(&format!(
            "Inspecting DotSlash descriptor '{}'",
            descriptor.name
        ));
    }

    let mut no_progress: Option<fn(u64, u64)> = None;
    let descriptor_path = provider_manager
        .download_asset(
            descriptor,
            &package.provider,
            download_cache,
            &mut no_progress,
        )
        .await
        .context(format!(
            "Failed to download DotSlash descriptor '{}'",
            descriptor.name
        ))?;

    let Ok(contents) = fs::read_to_string(&descriptor_path) else {
        return Ok(None);
    };

    let Ok(selected) = select_asset(&contents) else {
        return Ok(None);
    };

    if let Some(cb) = message_callback.as_mut() {
        cb(&format!(
            "Resolved DotSlash asset '{}' for '{}'",
            selected.filename, selected.platform.key
        ));
    }

    Ok(Some(Asset::with_filetype(
        selected.url,
        descriptor.id,
        selected.filename,
        selected.size,
        descriptor.created_at,
        selected.filetype,
    )))
}

pub fn select_asset_for_architecture(
    contents: &str,
    architecture: &ArchitectureInfo,
) -> Result<DotSlashAsset> {
    let file = parse_dotslash(contents)?;

    let mut unsupported_platforms = Vec::new();

    for (platform_key, entry) in file.platforms {
        let platform = match parse_platform_key(&platform_key) {
            Ok(platform) => platform,
            Err(_) => {
                unsupported_platforms.push(platform_key);
                continue;
            }
        };

        if platform.os != architecture.os_kind || platform.arch != architecture.cpu_arch {
            continue;
        }

        let url = entry
            .providers
            .iter()
            .filter_map(|provider| provider.url.as_deref())
            .find(|url| !url.trim().is_empty())
            .ok_or_else(|| anyhow!("DotSlash platform '{}' has no URL provider", platform.key))?;
        let filename = filename_from_url(url)?;

        return Ok(DotSlashAsset {
            dotslash_name: file.name,
            platform,
            filename,
            url: url.to_string(),
            size: entry.size,
            hash: entry.hash,
            digest: entry.digest,
            filetype: parse_format_filetype(&entry.format)?,
            format: entry.format,
            path: entry.path,
        });
    }

    let available = unsupported_platforms.join(", ");
    if available.is_empty() {
        Err(anyhow!(
            "DotSlash file has no asset for platform {}-{}",
            os_key(&architecture.os_kind),
            arch_key(&architecture.cpu_arch)
        ))
    } else {
        Err(anyhow!(
            "DotSlash file has no asset for platform {}-{}; unsupported platform keys: {}",
            os_key(&architecture.os_kind),
            arch_key(&architecture.cpu_arch),
            available
        ))
    }
}

fn parse_dotslash(contents: &str) -> Result<DotSlashFile> {
    let json = strip_json_comments(json_payload(contents)?);
    serde_json::from_str(&json).map_err(|err| anyhow!("Failed to parse DotSlash file: {err}"))
}

fn json_payload(contents: &str) -> Result<&str> {
    let offset = contents
        .find('{')
        .ok_or_else(|| anyhow!("DotSlash file does not contain a JSON object"))?;
    Ok(&contents[offset..])
}

fn strip_json_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaping = false;

    while let Some(ch) = chars.next() {
        if in_string {
            output.push(ch);
            if escaping {
                escaping = false;
            } else if ch == '\\' {
                escaping = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => {
                in_string = true;
                output.push(ch);
            }
            '/' if chars.peek() == Some(&'/') => {
                let _ = chars.next();
                for next in chars.by_ref() {
                    if next == '\n' {
                        output.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                let _ = chars.next();
                let mut previous = '\0';
                for next in chars.by_ref() {
                    if previous == '*' && next == '/' {
                        break;
                    }
                    if next == '\n' {
                        output.push('\n');
                    }
                    previous = next;
                }
            }
            _ => output.push(ch),
        }
    }

    output
}

fn parse_platform_key(platform_key: &str) -> Result<DotSlashPlatform> {
    let (os, arch) = platform_key
        .split_once('-')
        .ok_or_else(|| anyhow!("Invalid DotSlash platform key '{platform_key}'"))?;
    let os = parse_os_key(os)?;
    let arch = parse_arch_key(arch)?;

    if os == OSKind::Unknown || arch == CpuArch::Unknown {
        return Err(anyhow!(
            "Unsupported DotSlash platform key '{platform_key}'"
        ));
    }

    Ok(DotSlashPlatform {
        key: platform_key.to_string(),
        os,
        arch,
    })
}

fn parse_os_key(os: &str) -> Result<OSKind> {
    match os {
        "darwin" => Ok(OSKind::MacOS),
        "win" => Ok(OSKind::Windows),
        _ => OSKind::from_str(os).map_err(|_| anyhow!("Unsupported DotSlash OS '{os}'")),
    }
}

fn parse_arch_key(arch: &str) -> Result<CpuArch> {
    match arch {
        "amd64" | "x64" => Ok(CpuArch::X86_64),
        "arm64" => Ok(CpuArch::Aarch64),
        "i386" | "i686" => Ok(CpuArch::X86),
        _ => CpuArch::from_str(arch).map_err(|err| anyhow!("Unsupported DotSlash arch: {err}")),
    }
}

fn filename_from_url(url: &str) -> Result<String> {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .trim_end_matches('/');
    let filename = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow!("DotSlash provider URL has no filename: {url}"))?;

    Ok(filename.to_string())
}

fn parse_format_filetype(format: &str) -> Result<Filetype> {
    let normalized = format.trim().trim_start_matches('.');
    if normalized.is_empty() {
        return Err(anyhow!("DotSlash asset format is empty"));
    }

    let filetype = crate::utils::filename_parser::parse_filetype(&format!(".{normalized}"));
    if filetype == Filetype::Binary && !matches!(normalized, "bin" | "binary" | "raw") {
        return Err(anyhow!("Unsupported DotSlash asset format '{format}'"));
    }

    Ok(filetype)
}

fn os_key(os: &OSKind) -> &'static str {
    match os {
        OSKind::Windows => "windows",
        OSKind::MacOS => "macos",
        OSKind::Linux => "linux",
        OSKind::FreeBSD => "freebsd",
        OSKind::OpenBSD => "openbsd",
        OSKind::NetBSD => "netbsd",
        OSKind::Android => "android",
        OSKind::Ios => "ios",
        OSKind::Unknown => "unknown",
    }
}

fn arch_key(arch: &CpuArch) -> &'static str {
    match arch {
        CpuArch::X86 => "x86",
        CpuArch::X86_64 => "x86_64",
        CpuArch::Arm => "arm",
        CpuArch::Aarch64 => "aarch64",
        CpuArch::Ppc => "powerpc",
        CpuArch::Ppc64 => "powerpc64",
        CpuArch::Riscv64 => "riscv64",
        CpuArch::S390x => "s390x",
        CpuArch::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::common::enums::{Channel, Filetype, Provider};

    const INVALID_DOTSLASH_FIXTURE: &str = include_str!("../../../tests/fixtures/dotslash-invalid");
    const VALID_DOTSLASH_FIXTURE: &str = include_str!("../../../tests/fixtures/dotslash-valid");

    fn architecture(os_kind: OSKind, cpu_arch: CpuArch) -> ArchitectureInfo {
        ArchitectureInfo {
            is_os_64_bit: true,
            cpu_arch,
            os_kind,
        }
    }

    fn package(name: &str) -> Package {
        Package::with_defaults(
            name.to_string(),
            format!("owner/{name}"),
            Filetype::Archive,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        )
    }

    #[test]
    fn dotslash_asset_name_matches_repo_basename_without_extension() {
        let package = package("upstream");

        assert!(is_asset("upstream", &package));
        assert!(is_asset("UPSTREAM", &package));
        assert!(!is_asset("upstream.tar.gz", &package));
    }

    #[test]
    fn dotslash_asset_name_can_match_package_name() {
        let mut package = package("node");
        package.repo_slug = "nodejs/node".to_string();

        assert!(is_asset("node", &package));
        assert!(!is_asset("node.exe", &package));
    }

    #[test]
    fn selects_linux_x86_64_asset_from_example_file() {
        let asset = select_asset_for_architecture(
            VALID_DOTSLASH_FIXTURE,
            &architecture(OSKind::Linux, CpuArch::X86_64),
        )
        .expect("select asset");

        assert_eq!(asset.dotslash_name, "node-v18.19.0");
        assert_eq!(asset.platform.key, "linux-x86_64");
        assert_eq!(asset.filename, "node-v18.19.0-linux-x64.tar.gz");
        assert_eq!(
            asset.url,
            "https://nodejs.org/dist/v18.19.0/node-v18.19.0-linux-x64.tar.gz"
        );
        assert_eq!(asset.filetype, Filetype::Archive);
        assert_eq!(asset.path, "node-v18.19.0-linux-x64/bin/node");
    }

    #[test]
    fn selects_macos_aarch64_asset_from_example_file() {
        let filename = select_asset_for_architecture(
            VALID_DOTSLASH_FIXTURE,
            &architecture(OSKind::MacOS, CpuArch::Aarch64),
        )
        .expect("select asset")
        .filename;

        assert_eq!(filename, "node-v18.19.0-darwin-arm64.tar.gz");
    }

    #[test]
    fn ignores_shebang_and_comments_before_json() {
        let contents = r#"#!/usr/bin/env dotslash
// leading comment
{
  "name": "tool",
  "platforms": {
    "linux-x86_64": {
      "size": 12,
      "hash": "blake3",
      "digest": "abc",
      "format": "tar.gz",
      "path": "tool/bin/tool",
      "providers": [{ "url": "https://example.com/tool.tar.gz?download=1" }]
    }
  }
}"#;

        let filename =
            select_asset_for_architecture(contents, &architecture(OSKind::Linux, CpuArch::X86_64))
                .expect("select asset")
                .filename;

        assert_eq!(filename, "tool.tar.gz");
    }

    #[test]
    fn strips_json_comments_without_breaking_urls() {
        let contents = r#"{
  "name": "tool",
  "platforms": {
    "linux-x86_64": {
      "size": 12,
      "hash": "blake3",
      "digest": "abc",
      "format": "tar.gz", // package format
      "path": "tool/bin/tool",
      "providers": [{ "url": "https://example.com/tool.tar.gz" }]
    }
  }
}"#;

        let asset =
            select_asset_for_architecture(contents, &architecture(OSKind::Linux, CpuArch::X86_64))
                .expect("select asset");

        assert_eq!(asset.url, "https://example.com/tool.tar.gz");
        assert_eq!(asset.filename, "tool.tar.gz");
    }

    #[test]
    fn uses_format_to_determine_filetype_instead_of_filename() {
        let contents = r#"{
  "name": "tool",
  "platforms": {
    "linux-x86_64": {
      "size": 12,
      "hash": "blake3",
      "digest": "abc",
      "format": "tar.gz",
      "path": "tool/bin/tool",
      "providers": [{ "url": "https://example.com/tool.bin" }]
    }
  }
}"#;

        let asset =
            select_asset_for_architecture(contents, &architecture(OSKind::Linux, CpuArch::X86_64))
                .expect("select asset");

        assert_eq!(asset.filename, "tool.bin");
        assert_eq!(asset.filetype, Filetype::Archive);
    }

    #[test]
    fn errors_when_format_is_unsupported() {
        let contents = r#"{
  "name": "tool",
  "platforms": {
    "linux-x86_64": {
      "size": 12,
      "hash": "blake3",
      "digest": "abc",
      "format": "mystery",
      "path": "tool/bin/tool",
      "providers": [{ "url": "https://example.com/tool.bin" }]
    }
  }
}"#;

        let err =
            select_asset_for_architecture(contents, &architecture(OSKind::Linux, CpuArch::X86_64))
                .expect_err("unsupported format");

        assert!(
            err.to_string()
                .contains("Unsupported DotSlash asset format")
        );
    }

    #[test]
    fn selects_url_provider_when_other_provider_variants_are_present() {
        let contents = r#"{
  "name": "codex",
  "platforms": {
    "linux-x86_64": {
      "size": 77258145,
      "hash": "blake3",
      "digest": "822f3acafd8f6a700723e7183e5e756b410bdcce130a9931faa223a24acde9e6",
      "format": "tar.zst",
      "path": "bin/codex",
      "providers": [
        {
          "url": "https://github.com/openai/codex/releases/download/rust-v0.142.3/codex-package-x86_64-unknown-linux-musl.tar.zst"
        },
        {
          "type": "github-release",
          "repo": "https://github.com/openai/codex",
          "tag": "rust-v0.142.3",
          "name": "codex-package-x86_64-unknown-linux-musl.tar.zst"
        }
      ]
    }
  }
}"#;

        let asset =
            select_asset_for_architecture(contents, &architecture(OSKind::Linux, CpuArch::X86_64))
                .expect("select asset");

        assert_eq!(
            asset.filename,
            "codex-package-x86_64-unknown-linux-musl.tar.zst"
        );
        assert_eq!(asset.filetype, Filetype::Archive);
        assert_eq!(
            asset.url,
            "https://github.com/openai/codex/releases/download/rust-v0.142.3/codex-package-x86_64-unknown-linux-musl.tar.zst"
        );
        assert_eq!(asset.path, "bin/codex");
    }

    #[test]
    fn errors_when_host_platform_is_missing() {
        let err = select_asset_for_architecture(
            VALID_DOTSLASH_FIXTURE,
            &architecture(OSKind::Windows, CpuArch::X86_64),
        )
        .expect_err("missing platform");

        assert!(
            err.to_string()
                .contains("DotSlash file has no asset for platform windows-x86_64")
        );
    }

    #[test]
    fn errors_when_file_is_not_valid_dotslash() {
        let err = select_asset_for_architecture(
            INVALID_DOTSLASH_FIXTURE,
            &architecture(OSKind::Linux, CpuArch::X86_64),
        )
        .expect_err("invalid dotslash");

        assert!(err.to_string().contains("Failed to parse DotSlash file"));
        assert!(err.to_string().contains("missing field `platforms`"));
    }

    #[test]
    fn errors_when_selected_platform_has_no_provider_url() {
        let contents = r#"{
  "name": "tool",
  "platforms": {
    "linux-x86_64": {
      "size": 12,
      "hash": "blake3",
      "digest": "abc",
      "format": "tar.gz",
      "path": "tool/bin/tool",
      "providers": [{ "url": "" }]
    }
  }
}"#;

        let err =
            select_asset_for_architecture(contents, &architecture(OSKind::Linux, CpuArch::X86_64))
                .expect_err("missing provider");

        assert!(
            err.to_string()
                .contains("DotSlash platform 'linux-x86_64' has no URL provider")
        );
    }
}
