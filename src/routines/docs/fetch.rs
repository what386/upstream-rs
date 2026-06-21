use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::{
    models::upstream::Package, providers::provider_manager::ProviderManager,
    utils::filesystem::atomic_ops::write_atomic,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectReadme {
    pub document_name: String,
    pub contents: String,
    pub source: ProjectReadmeSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectReadmeSource {
    Remote,
    CachedFallback,
    CachedOffline,
}

pub async fn fetch_project_readme(
    provider_manager: &ProviderManager,
    cache_dir: &Path,
    package: &Package,
    offline: bool,
) -> Result<ProjectReadme> {
    let cache_path = cache_path_for_package(cache_dir, package);
    if offline {
        let contents = read_cached_readme(&cache_path).with_context(|| {
            format!(
                "No cached README was available for '{}'. Re-run without --offline to fetch it.",
                package.name
            )
        })?;
        return Ok(ProjectReadme {
            document_name: "README.md".to_string(),
            contents,
            source: ProjectReadmeSource::CachedOffline,
        });
    }

    match provider_manager
        .get_project_readme(
            &package.repo_slug,
            &package.provider,
            package.base_url.as_deref(),
        )
        .await
    {
        Ok(contents) => {
            write_cached_readme(&cache_path, &contents)?;
            Ok(ProjectReadme {
                document_name: "README.md".to_string(),
                contents,
                source: ProjectReadmeSource::Remote,
            })
        }
        Err(fetch_error) => {
            let contents = read_cached_readme(&cache_path).with_context(|| {
                format!(
                    "Failed to fetch README for '{}' and no cached README was available",
                    package.name
                )
            })?;

            if contents.trim().is_empty() {
                return Err(fetch_error).with_context(|| {
                    format!(
                        "Failed to fetch README for '{}' and cached README is empty",
                        package.name
                    )
                });
            }

            Ok(ProjectReadme {
                document_name: "README.md".to_string(),
                contents,
                source: ProjectReadmeSource::CachedFallback,
            })
        }
    }
}

pub async fn refetch_project_readme(
    provider_manager: &ProviderManager,
    cache_dir: &Path,
    package: &Package,
) -> Result<ProjectReadme> {
    let contents = provider_manager
        .get_project_readme(
            &package.repo_slug,
            &package.provider,
            package.base_url.as_deref(),
        )
        .await
        .with_context(|| format!("Failed to fetch README for '{}'", package.name))?;

    let cache_path = cache_path_for_package(cache_dir, package);
    write_cached_readme(&cache_path, &contents)?;

    Ok(ProjectReadme {
        document_name: "README.md".to_string(),
        contents,
        source: ProjectReadmeSource::Remote,
    })
}

fn cache_path_for_package(cache_dir: &Path, package: &Package) -> PathBuf {
    cache_dir
        .join("docs")
        .join(format!(
            "{}_{}",
            sanitize_cache_component(&package.repo_slug),
            sanitize_cache_component(&package.name)
        ))
        .join("README.md")
}

fn sanitize_cache_component(value: &str) -> String {
    let mut out = String::new();
    let mut previous_separator = false;

    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
            out.push(ch);
            previous_separator = false;
        } else if !previous_separator && !out.is_empty() {
            out.push('_');
            previous_separator = true;
        }
    }

    while out.ends_with('_') {
        out.pop();
    }

    if out.is_empty() {
        "repo".to_string()
    } else {
        out
    }
}

fn read_cached_readme(path: &Path) -> Result<String> {
    fs::read_to_string(path)
        .with_context(|| format!("Failed to read cached README '{}'", path.display()))
}

fn write_cached_readme(path: &Path, contents: &str) -> Result<()> {
    write_atomic(path, contents.as_bytes())
        .with_context(|| format!("Failed to write cached README '{}'", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{
        ProjectReadmeSource, cache_path_for_package, fetch_project_readme, read_cached_readme,
        sanitize_cache_component, write_cached_readme,
    };
    use crate::models::{
        common::enums::{Channel, Filetype, Provider},
        upstream::Package,
    };
    use crate::providers::provider_manager::ProviderManager;
    use std::path::Path;

    fn test_package() -> Package {
        Package::with_defaults(
            "upstream".to_string(),
            "what386/upstream-rs".to_string(),
            Filetype::Archive,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        )
    }

    #[test]
    fn sanitize_cache_component_keeps_paths_flat() {
        assert_eq!(
            sanitize_cache_component("what386/upstream-rs"),
            "what386_upstream-rs"
        );
        assert_eq!(
            sanitize_cache_component("group/sub group/project"),
            "group_sub_group_project"
        );
        assert_eq!(sanitize_cache_component("///"), "repo");
    }

    #[test]
    fn cache_path_uses_repo_slug_and_package_name() {
        let package = test_package();

        let path = cache_path_for_package(Path::new("/tmp/cache"), &package);

        assert_eq!(
            path,
            Path::new("/tmp/cache")
                .join("docs")
                .join("what386_upstream-rs_upstream")
                .join("README.md")
        );
    }

    #[test]
    fn cached_readme_round_trips() {
        let root = crate::utils::test_support::temp_root("docs-cache", "readme");
        let path = root.join("docs/owner_repo_tool/README.md");

        write_cached_readme(&path, "# README\n").expect("write cache");
        let contents = read_cached_readme(&path).expect("read cache");

        assert_eq!(contents, "# README\n");
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[tokio::test]
    async fn offline_mode_reads_cached_readme_without_provider_fetch() {
        let root = crate::utils::test_support::temp_root("docs-cache", "offline");
        let package = test_package();
        let cache_path = cache_path_for_package(&root, &package);
        write_cached_readme(&cache_path, "# Cached README\n").expect("write cache");
        let manager =
            ProviderManager::new(None, None, None, Default::default()).expect("provider manager");

        let readme = fetch_project_readme(&manager, &root, &package, true)
            .await
            .expect("offline readme");

        assert_eq!(readme.contents, "# Cached README\n");
        assert_eq!(readme.source, ProjectReadmeSource::CachedOffline);
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
