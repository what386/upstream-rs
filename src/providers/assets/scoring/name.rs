use crate::models::common::enums::Provider;
use crate::models::upstream::Package;

pub(in crate::providers::assets) fn package_identity(package: &Package) -> String {
    if matches!(
        package.provider,
        Provider::Github | Provider::Gitlab | Provider::Gitea
    ) {
        return package
            .repo_slug
            .rsplit('/')
            .next()
            .unwrap_or(package.repo_slug.as_str())
            .trim_end_matches(".git")
            .to_lowercase();
    }

    package.id.trim().to_lowercase()
}

pub(super) fn primary_score(name: &str, package_name: &str) -> i32 {
    if package_name.is_empty() {
        return 0;
    }

    let stem = strip_known_suffixes(name);
    if stem == package_name {
        return 80;
    }

    if starts_with_primary_target(&stem, package_name) {
        return 50;
    }

    if stem.starts_with(&format!("{package_name}-"))
        || stem.starts_with(&format!("{package_name}_"))
    {
        return 10;
    }

    if contains_name_token(&stem, package_name) {
        return 0;
    }

    -60
}

pub(super) fn role_score(name: &str, package_name: &str) -> i32 {
    let tokens = tokens(name);
    let has = |token: &str| tokens.contains(&token);
    let mut score = 0;

    if has("cli") {
        score += 20;
    }

    if has("bin") || has("binary") {
        score += 15;
    }

    if has("standalone") || has("portable") || has("bundle") {
        score += 10;
    }

    if has("server") || has("proxy") || has("sdk") || has("npm") {
        score -= 25;
    }

    if has("setup") || has("installer") {
        score -= 25;
    }

    if has("zsh") || has("bash") || has("fish") || has("completion") || has("completions") {
        score -= 25;
    }

    if has("package") {
        score -= 10;
    }

    if has("app") && !package_name.contains("app") {
        score -= 10;
    }

    score
}

pub(super) fn auxiliary_penalty(name: &str) -> i32 {
    let mut penalty = 0;
    if name.contains("symbols") || name.contains("debug") || name.contains("pdb") {
        penalty -= 80;
    }

    if is_auxiliary_asset_name(name) {
        penalty -= 100;
    }

    if is_installer_script_name(name) {
        penalty -= 80;
    }

    penalty
}

pub(in crate::providers::assets) fn is_auxiliary_asset_name(name: &str) -> bool {
    name.ends_with(".sig")
        || name.ends_with(".asc")
        || name.ends_with(".sigstore")
        || name.ends_with(".sha256")
        || name.ends_with(".sha256sum")
        || name.ends_with(".sha256sums")
        || name.ends_with("_sha256sum")
        || name.ends_with("_sha256sums")
        || name.ends_with(".checksums")
        || name.ends_with(".checksum")
        || name.ends_with(".json")
        || name.ends_with(".spdx")
        || name.ends_with(".sbom")
        || name.ends_with(".txt")
}

fn starts_with_primary_target(stem: &str, package_name: &str) -> bool {
    const TARGET_PREFIXES: &[&str] = &[
        "x86_64",
        "amd64",
        "x64",
        "aarch64",
        "arm64",
        "arm",
        "x86",
        "i686",
        "linux",
        "darwin",
        "macos",
        "windows",
        "win32",
        "win64",
        "musl",
        "gnu",
        "manylinux",
    ];

    TARGET_PREFIXES.iter().any(|target| {
        stem.starts_with(&format!("{package_name}-{target}"))
            || stem.starts_with(&format!("{package_name}_{target}"))
    })
}

fn contains_name_token(value: &str, package_name: &str) -> bool {
    tokens(value).into_iter().any(|token| token == package_name)
}

fn tokens(name: &str) -> Vec<&str> {
    name.split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect()
}

fn is_installer_script_name(name: &str) -> bool {
    matches!(
        name,
        "install.sh"
            | "install.ps1"
            | "install.bat"
            | "install.cmd"
            | "installer.sh"
            | "installer.ps1"
            | "setup.sh"
            | "setup.ps1"
    )
}

fn strip_known_suffixes(name: &str) -> String {
    const SUFFIXES: &[&str] = &[
        ".tar.bz2",
        ".tar.gz",
        ".tar.xz",
        ".tar.zst",
        ".appimage",
        ".pkg",
        ".msi",
        ".exe",
        ".tgz",
        ".tbz",
        ".txz",
        ".zip",
        ".whl",
        ".gz",
        ".bz2",
        ".xz",
        ".zst",
    ];

    let mut stem = name;
    while let Some(suffix) = SUFFIXES
        .iter()
        .find(|suffix| stem.ends_with(**suffix))
        .copied()
    {
        stem = &stem[..stem.len() - suffix.len()];
    }

    stem.to_string()
}
