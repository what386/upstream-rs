use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Result, anyhow, bail, ensure};
use blake2::Blake2bVar;
use blake2::digest::{Update, VariableOutput};

use crate::models::provider::{Asset, Release};
use crate::models::upstream::Package;
use crate::utils::platform::platform_info::{ArchitectureInfo, CpuArch, OSKind};

const EMBEDDED_BLAKE2B_MODEL: &[u8] = include_bytes!("selector-feature-v2b-full.nasl");
const MODEL_MAGIC: &[u8; 4] = b"NASL";
const MODEL_VERSION: u16 = 1;
const HASH_BLAKE2B64: u16 = 1;

const PLATFORM_TOKENS: &[(&str, &[&str])] = &[
    (
        "windows",
        &[
            "windows",
            "win",
            "win32",
            "win64",
            "msvc",
            "pc-windows-msvc",
            "exe",
        ],
    ),
    (
        "linux",
        &[
            "linux",
            "gnu",
            "musl",
            "glibc",
            "unknown-linux-gnu",
            "unknown-linux-musl",
            "appimage",
        ],
    ),
    ("macos", &["mac", "macos", "darwin", "apple", "osx", "dmg"]),
];
const ARCH_TOKENS: &[(&str, &[&str])] = &[
    ("x64", &["x64", "x86_64", "x86-64", "amd64", "64bit"]),
    ("x86", &["x86", "i386", "i686", "32bit"]),
    ("arm64", &["arm64", "aarch64", "apple-silicon"]),
    ("armv7", &["arm", "armv7", "armhf"]),
];
const BAD_TOKENS: &[&str] = &[
    "sha256",
    "sha512",
    "checksums",
    "checksum",
    "sig",
    "sigstore",
    "asc",
    "minisig",
    "sbom",
    "source",
    "src",
    "debug",
    "symbols",
];
const INSTALLABLE_EXTS: &[&str] = &[
    "zip", "7z", "tar", "gz", "tgz", "xz", "zst", "tar.gz", "tar.xz", "tar.zst", "tar.bz2", "msi",
    "exe", "dmg", "pkg", "deb", "rpm", "appimage",
];
const ARCH_NEUTRAL_TOKENS: &[&str] = &["all", "any", "fat", "noarch", "universal", "universal2"];
const LINUX_PACKAGE_EXTS: &[&str] = &["deb", "rpm", "appimage"];
const WINDOWS_PACKAGE_EXTS: &[&str] = &["exe", "msi"];
const MACOS_PACKAGE_EXTS: &[&str] = &["dmg", "pkg"];
const ARCHIVE_EXTS: &[&str] = &[
    "zip", "7z", "tar", "gz", "tgz", "xz", "zst", "tar.gz", "tar.xz", "tar.zst", "tar.bz2",
];

#[derive(Debug, Clone)]
pub struct NeuralAssetPrediction {
    pub asset_index: usize,
    pub asset_name: String,
    pub score: f64,
    pub confidence: f64,
    pub alternatives: Vec<NeuralAssetAlternative>,
}

#[derive(Debug, Clone)]
pub struct NeuralAssetAlternative {
    pub asset_index: usize,
    pub asset_name: String,
    pub score: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
pub struct NeuralAssetSelector {
    model: ReleaseAssetModel,
    architecture_info: ArchitectureInfo,
}

#[derive(Debug, Clone)]
struct ReleaseAssetModel {
    feature_size: usize,
    weight_scale: f64,
    weights_i16: Vec<i16>,
}

impl NeuralAssetSelector {
    pub fn embedded_blake2b() -> Result<Self> {
        Self::with_architecture(ArchitectureInfo::new())
    }

    pub fn with_architecture(architecture_info: ArchitectureInfo) -> Result<Self> {
        Ok(Self {
            model: ReleaseAssetModel::decode(EMBEDDED_BLAKE2B_MODEL)?,
            architecture_info,
        })
    }

    pub fn predict(
        &self,
        release: &Release,
        package: &Package,
        top_k: usize,
    ) -> Result<NeuralAssetPrediction> {
        self.predict_for_target(release, package, &self.architecture_info, top_k)
    }

    pub fn predict_for_target(
        &self,
        release: &Release,
        package: &Package,
        target: &ArchitectureInfo,
        top_k: usize,
    ) -> Result<NeuralAssetPrediction> {
        ensure!(!release.assets.is_empty(), "release has no assets");
        let platform = model_platform(&target.os_kind)
            .ok_or_else(|| anyhow!("unsupported target OS for neural selector"))?;
        let arch = model_arch(&target.cpu_arch)
            .ok_or_else(|| anyhow!("unsupported target CPU architecture for neural selector"))?;
        let package_name = package.name.as_str();
        let version = if release.tag.is_empty() {
            release.name.as_str()
        } else {
            release.tag.as_str()
        };
        let scores: Vec<f64> = release
            .assets
            .iter()
            .map(|asset| {
                self.model
                    .score_asset(package_name, version, platform, arch, asset)
            })
            .collect::<Result<Vec<_>>>()?;
        let probabilities = softmax(&scores);
        let mut ranked: Vec<usize> = (0..scores.len()).collect();
        ranked.sort_by(|left, right| scores[*right].total_cmp(&scores[*left]));
        let best = *ranked.first().expect("non-empty scores");
        let alternatives = ranked
            .iter()
            .take(top_k.max(1))
            .map(|index| NeuralAssetAlternative {
                asset_index: *index,
                asset_name: release.assets[*index].name.clone(),
                score: scores[*index],
                confidence: probabilities[*index],
            })
            .collect();
        Ok(NeuralAssetPrediction {
            asset_index: best,
            asset_name: release.assets[best].name.clone(),
            score: scores[best],
            confidence: probabilities[best],
            alternatives,
        })
    }
}

impl ReleaseAssetModel {
    fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() >= 20, "model is too small");
        ensure!(&bytes[0..4] == MODEL_MAGIC, "invalid model magic");
        let version = read_u16(bytes, 4)?;
        ensure!(
            version == MODEL_VERSION,
            "unsupported model version: {version}"
        );
        let hash_id = read_u16(bytes, 6)?;
        ensure!(
            hash_id == HASH_BLAKE2B64,
            "expected Blake2b model hash id, got {hash_id}"
        );
        let feature_size = read_u32(bytes, 8)? as usize;
        let weight_scale = read_f64(bytes, 12)?;
        let expected_len = 20 + feature_size * 2;
        ensure!(bytes.len() == expected_len, "invalid model length");
        let mut weights_i16 = Vec::with_capacity(feature_size);
        for index in 0..feature_size {
            weights_i16.push(read_i16(bytes, 20 + index * 2)?);
        }
        Ok(Self {
            feature_size,
            weight_scale,
            weights_i16,
        })
    }

    fn score_asset(
        &self,
        package: &str,
        version: &str,
        platform: &str,
        arch: &str,
        asset: &Asset,
    ) -> Result<f64> {
        let features = asset_features(package, version, platform, arch, asset, self.feature_size)?;
        Ok(features
            .into_iter()
            .map(|(index, value)| self.weight(index) * value)
            .sum())
    }

    fn weight(&self, index: usize) -> f64 {
        self.weights_i16[index] as f64 * self.weight_scale
    }
}

fn asset_features(
    package: &str,
    version: &str,
    platform: &str,
    arch: &str,
    asset: &Asset,
    feature_size: usize,
) -> Result<HashMap<usize, f64>> {
    let mut values = HashMap::new();
    let name = asset.name.to_lowercase();
    let normalized_name = name.replace('_', "-");
    let mut tokens: HashSet<String> = tokenize(&name).into_iter().collect();
    tokens.extend(tokenize(&normalized_name));
    let package_tokens: HashSet<String> = tokenize(package).into_iter().collect();
    let version_tokens: HashSet<String> = tokenize(version.trim_start_matches('v'))
        .into_iter()
        .collect();
    let ext = extension(&name);
    let artifact_family = artifact_family(&ext);

    add(&mut values, "bias", 1.0, feature_size)?;
    add(
        &mut values,
        &format!("target:platform={platform}"),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!("target:arch={arch}"),
        1.0,
        feature_size,
    )?;
    add(&mut values, &format!("ext={ext}"), 1.0, feature_size)?;
    add(
        &mut values,
        &format!("target_platform_x_ext={platform}:{ext}"),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!("target_arch_x_ext={arch}:{ext}"),
        0.5,
        feature_size,
    )?;
    add(
        &mut values,
        &format!("size_bucket={}", size_bucket(asset.size)),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!(
            "installable_ext={}",
            py_bool(contains(INSTALLABLE_EXTS, &ext))
        ),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!("artifact_family={artifact_family}"),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!("target_platform_x_artifact_family={platform}:{artifact_family}"),
        1.0,
        feature_size,
    )?;

    for token in &tokens {
        add(
            &mut values,
            &format!("asset_token={token}"),
            1.0,
            feature_size,
        )?;
        add(
            &mut values,
            &format!("target_platform_x_token={platform}:{token}"),
            0.5,
            feature_size,
        )?;
        add(
            &mut values,
            &format!("target_arch_x_token={arch}:{token}"),
            0.5,
            feature_size,
        )?;
    }
    for token in &package_tokens {
        add(
            &mut values,
            &format!("package_token={token}"),
            0.5,
            feature_size,
        )?;
    }
    for token in &version_tokens {
        add(
            &mut values,
            &format!("version_token={token}"),
            0.25,
            feature_size,
        )?;
    }

    let platform_match_from_name =
        matches_any(&normalized_name, &tokens, platform_aliases(platform));
    let platform_match_from_ext = platform_exts(platform).is_some_and(|exts| contains(exts, &ext));
    let platform_match = platform_match_from_name || platform_match_from_ext;
    let foreign_platforms: Vec<&str> = PLATFORM_TOKENS
        .iter()
        .filter_map(|(other_platform, aliases)| {
            if *other_platform == platform {
                return None;
            }
            let ext_matches =
                platform_exts(other_platform).is_some_and(|exts| contains(exts, &ext));
            (matches_any(&normalized_name, &tokens, aliases) || ext_matches)
                .then_some(*other_platform)
        })
        .collect();
    let platform_mismatch = !foreign_platforms.is_empty();
    let arch_match = matches_any(&normalized_name, &tokens, arch_aliases(arch));
    let arch_neutral = tokens
        .iter()
        .any(|token| contains(ARCH_NEUTRAL_TOKENS, token));
    let arch_mismatch = !arch_neutral
        && ARCH_TOKENS.iter().any(|(other_arch, aliases)| {
            *other_arch != arch && matches_any(&normalized_name, &tokens, aliases)
        });
    let package_lower = package.to_lowercase();
    let package_overlap = package_tokens.iter().any(|token| tokens.contains(token))
        || normalized_name.contains(&package_lower);
    let version_key = version.to_lowercase().trim_start_matches('v').to_string();
    let version_overlap = version_tokens.iter().any(|token| tokens.contains(token))
        || (!version_key.is_empty() && normalized_name.contains(&version_key));
    let bad_token = tokens.iter().any(|token| contains(BAD_TOKENS, token));
    let package_ext = package_ext_for_platform(&ext, platform);
    let archive_ext = contains(ARCHIVE_EXTS, &ext);
    let macos_universal = platform == "macos" && arch_neutral;
    let platform_specific_ext = platform_specific_ext(&ext);

    add(
        &mut values,
        &format!("platform_match={}", py_bool(platform_match)),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!(
            "platform_match_from_name={}",
            py_bool(platform_match_from_name)
        ),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!(
            "platform_match_from_ext={}",
            py_bool(platform_match_from_ext)
        ),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!("platform_mismatch={}", py_bool(platform_mismatch)),
        1.0,
        feature_size,
    )?;
    for foreign_platform in foreign_platforms {
        add(
            &mut values,
            &format!("foreign_platform={foreign_platform}"),
            1.0,
            feature_size,
        )?;
        add(
            &mut values,
            &format!("target_platform_x_foreign_platform={platform}:{foreign_platform}"),
            1.0,
            feature_size,
        )?;
    }
    add(
        &mut values,
        &format!("arch_match={}", py_bool(arch_match)),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!("arch_neutral={}", py_bool(arch_neutral)),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!(
            "target_arch_x_arch_neutral={arch}:{}",
            py_bool(arch_neutral)
        ),
        0.75,
        feature_size,
    )?;
    add(
        &mut values,
        &format!("arch_mismatch={}", py_bool(arch_mismatch)),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!("package_overlap={}", py_bool(package_overlap)),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!("version_overlap={}", py_bool(version_overlap)),
        0.5,
        feature_size,
    )?;
    add(
        &mut values,
        &format!("bad_token={}", py_bool(bad_token)),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!(
            "platform_arch_match={}",
            py_bool(platform_match && arch_match)
        ),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!(
            "platform_arch_or_neutral_match={}",
            py_bool(platform_match && (arch_match || arch_neutral))
        ),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!(
            "platform_or_arch_mismatch={}",
            py_bool(platform_mismatch || arch_mismatch)
        ),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!("target_package_ext={platform}:{}", py_bool(package_ext)),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!("target_archive_ext={platform}:{}", py_bool(archive_ext)),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!("macos_universal={}", py_bool(macos_universal)),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!(
            "target_platform_x_no_extension={platform}:{}",
            py_bool(ext == "none")
        ),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!(
            "target_platform_x_universal_archive={platform}:{}",
            py_bool(arch_neutral && archive_ext)
        ),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!("platform_specific_ext={platform_specific_ext}"),
        1.0,
        feature_size,
    )?;
    add(
        &mut values,
        &format!("target_platform_x_platform_specific_ext={platform}:{platform_specific_ext}"),
        1.0,
        feature_size,
    )?;
    if ext == "7z" {
        add(
            &mut values,
            "windows_archive_7z_candidate",
            if platform == "windows" { 1.0 } else { -1.0 },
            feature_size,
        )?;
    }
    add(
        &mut values,
        "log_size",
        ((asset.size as f64).ln_1p()) / 20.0,
        feature_size,
    )?;
    Ok(values)
}

fn add(
    values: &mut HashMap<usize, f64>,
    name: &str,
    value: f64,
    feature_size: usize,
) -> Result<()> {
    let index = blake2b_feature_index(name, feature_size)?;
    *values.entry(index).or_insert(0.0) += value;
    Ok(())
}

fn blake2b_feature_index(name: &str, feature_size: usize) -> Result<usize> {
    let mut hasher =
        Blake2bVar::new(8).map_err(|err| anyhow!("failed to create Blake2b hasher: {err}"))?;
    hasher.update(name.as_bytes());
    let mut out = [0_u8; 8];
    hasher
        .finalize_variable(&mut out)
        .map_err(|err| anyhow!("failed to finalize Blake2b hash: {err}"))?;
    Ok((u64::from_le_bytes(out) as usize) % feature_size)
}

fn tokenize(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            current.push(ch);
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn matches_any(name: &str, tokens: &HashSet<String>, aliases: &[&str]) -> bool {
    aliases.iter().any(|alias| {
        let normalized = alias.to_lowercase().replace('_', "-");
        tokens.contains(&normalized)
            || (normalized.contains('-') && contains_alias_phrase(name, &normalized))
    })
}

fn contains_alias_phrase(name: &str, alias: &str) -> bool {
    let mut start = 0;
    while let Some(relative_index) = name[start..].find(alias) {
        let index = start + relative_index;
        let before = if index == 0 {
            '-'
        } else {
            name[..index].chars().next_back().unwrap_or('-')
        };
        let after_index = index + alias.len();
        let after = name[after_index..].chars().next().unwrap_or('-');
        if matches!(before, '.' | '-' | '_') && matches!(after, '.' | '-' | '_') {
            return true;
        }
        start = index + 1;
    }
    false
}

fn extension(name: &str) -> String {
    let path = Path::new(name);
    let suffixes: Vec<String> = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(name)
        .split('.')
        .skip(1)
        .map(|part| part.to_ascii_lowercase())
        .collect();
    if suffixes.len() >= 2 {
        let compound = format!(
            "{}.{}",
            suffixes[suffixes.len() - 2],
            suffixes[suffixes.len() - 1]
        );
        if matches!(
            compound.as_str(),
            "tar.gz" | "tar.xz" | "tar.zst" | "tar.bz2"
        ) {
            return compound;
        }
    }
    suffixes
        .last()
        .cloned()
        .unwrap_or_else(|| "none".to_string())
}

fn size_bucket(size: u64) -> &'static str {
    if size == 0 {
        return "unknown";
    }
    let mib = size as f64 / (1024.0 * 1024.0);
    if mib < 1.0 {
        "tiny"
    } else if mib < 5.0 {
        "small"
    } else if mib < 25.0 {
        "medium"
    } else if mib < 100.0 {
        "large"
    } else {
        "huge"
    }
}

fn artifact_family(ext: &str) -> &'static str {
    if contains(LINUX_PACKAGE_EXTS, ext) {
        "linux_package"
    } else if contains(WINDOWS_PACKAGE_EXTS, ext) {
        "windows_package"
    } else if contains(MACOS_PACKAGE_EXTS, ext) {
        "macos_package"
    } else if contains(ARCHIVE_EXTS, ext) {
        "archive"
    } else if ext == "none" {
        "raw_binary"
    } else if contains(INSTALLABLE_EXTS, ext) {
        "installable_other"
    } else {
        "other"
    }
}

fn package_ext_for_platform(ext: &str, platform: &str) -> bool {
    match platform {
        "linux" => contains(LINUX_PACKAGE_EXTS, ext),
        "windows" => contains(WINDOWS_PACKAGE_EXTS, ext),
        "macos" => contains(MACOS_PACKAGE_EXTS, ext),
        _ => false,
    }
}

fn platform_specific_ext(ext: &str) -> &'static str {
    if contains(LINUX_PACKAGE_EXTS, ext) {
        "linux"
    } else if contains(WINDOWS_PACKAGE_EXTS, ext) {
        "windows"
    } else if contains(MACOS_PACKAGE_EXTS, ext) {
        "macos"
    } else {
        "none"
    }
}

fn platform_aliases(platform: &str) -> &'static [&'static str] {
    PLATFORM_TOKENS
        .iter()
        .find_map(|(key, aliases)| (*key == platform).then_some(*aliases))
        .unwrap_or(&[])
}

fn arch_aliases(arch: &str) -> &'static [&'static str] {
    ARCH_TOKENS
        .iter()
        .find_map(|(key, aliases)| (*key == arch).then_some(*aliases))
        .unwrap_or(&[])
}

fn platform_exts(platform: &str) -> Option<&'static [&'static str]> {
    match platform {
        "linux" => Some(LINUX_PACKAGE_EXTS),
        "windows" => Some(WINDOWS_PACKAGE_EXTS),
        "macos" => Some(MACOS_PACKAGE_EXTS),
        _ => None,
    }
}

fn model_platform(os: &OSKind) -> Option<&'static str> {
    match os {
        OSKind::Windows => Some("windows"),
        OSKind::Linux => Some("linux"),
        OSKind::MacOS => Some("macos"),
        _ => None,
    }
}

fn model_arch(arch: &CpuArch) -> Option<&'static str> {
    match arch {
        CpuArch::X86_64 => Some("x64"),
        CpuArch::Aarch64 => Some("arm64"),
        CpuArch::X86 => Some("x86"),
        CpuArch::Arm => Some("armv7"),
        _ => None,
    }
}

fn contains(values: &[&str], value: &str) -> bool {
    values.iter().any(|item| *item == value)
}

fn py_bool(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

fn softmax(scores: &[f64]) -> Vec<f64> {
    if scores.is_empty() {
        return Vec::new();
    }
    let offset = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let exps: Vec<f64> = scores.iter().map(|score| (score - offset).exp()).collect();
    let total: f64 = exps.iter().sum();
    exps.into_iter().map(|value| value / total).collect()
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(read_array(bytes, offset)?))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(read_array(bytes, offset)?))
}

fn read_i16(bytes: &[u8], offset: usize) -> Result<i16> {
    Ok(i16::from_le_bytes(read_array(bytes, offset)?))
}

fn read_f64(bytes: &[u8], offset: usize) -> Result<f64> {
    Ok(f64::from_le_bytes(read_array(bytes, offset)?))
}

fn read_array<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N]> {
    let end = offset + N;
    if end > bytes.len() {
        bail!("model read out of bounds");
    }
    bytes[offset..end]
        .try_into()
        .map_err(|_| anyhow!("failed to read model bytes"))
}

#[cfg(test)]
mod tests {
    use super::{NeuralAssetSelector, ReleaseAssetModel};
    use crate::models::common::Version;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::provider::{Asset, Release};
    use crate::models::upstream::Package;
    use crate::utils::platform::platform_info::{ArchitectureInfo, CpuArch, OSKind};
    use chrono::Utc;

    fn codex_package() -> Package {
        Package::with_defaults(
            "codex".to_string(),
            "openai/codex".to_string(),
            Filetype::Auto,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        )
    }

    fn codex_release() -> Release {
        let names = [
            ("codex-package-x86_64-pc-windows-msvc.tar.zst", 84_538_161),
            (
                "codex-app-server-package-x86_64-pc-windows-msvc.tar.zst",
                72_436_248,
            ),
            ("codex-package-x86_64-pc-windows-msvc.tar.gz", 113_464_362),
            ("codex-command-runner-x86_64-pc-windows-msvc.exe", 1_293_616),
            ("codex-x86_64-pc-windows-msvc.exe", 323_007_280),
            ("codex-package-aarch64-apple-darwin.tar.zst", 80_000_000),
            ("codex-package-x86_64-unknown-linux-gnu.tar.zst", 90_000_000),
        ];
        Release {
            id: 1,
            tag: "rust-v0.142.3".to_string(),
            name: "rust-v0.142.3".to_string(),
            body: String::new(),
            is_draft: false,
            is_prerelease: false,
            assets: names
                .iter()
                .enumerate()
                .map(|(index, (name, size))| {
                    Asset::new(
                        format!("https://example.invalid/{name}"),
                        index as u64,
                        (*name).to_string(),
                        *size,
                        Utc::now(),
                    )
                })
                .collect(),
            version: Version::new(0, 142, 3, false),
            published_at: Utc::now(),
        }
    }

    #[test]
    fn embedded_model_decodes() {
        let selector = NeuralAssetSelector::embedded_blake2b().expect("decode embedded model");
        assert_eq!(selector.model.feature_size, 8192);
        assert_eq!(selector.model.weights_i16.len(), 8192);
    }

    #[test]
    fn rejects_non_blake2b_model() {
        let mut bytes = include_bytes!("selector-feature-v2b-full.nasl").to_vec();
        bytes[6] = 2;
        let error = ReleaseAssetModel::decode(&bytes).expect_err("hash id should be rejected");
        assert!(error.to_string().contains("expected Blake2b"));
    }

    #[test]
    fn predicts_codex_windows_x64_package() {
        let selector = NeuralAssetSelector::embedded_blake2b().expect("selector");
        let target = ArchitectureInfo {
            is_os_64_bit: true,
            cpu_arch: CpuArch::X86_64,
            os_kind: OSKind::Windows,
        };
        let prediction = selector
            .predict_for_target(&codex_release(), &codex_package(), &target, 3)
            .expect("prediction");
        assert_eq!(
            prediction.asset_name,
            "codex-package-x86_64-pc-windows-msvc.tar.zst"
        );
        assert!(prediction.confidence > 0.9);
    }
}
