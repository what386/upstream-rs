use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::models::common::enums::Provider;
use crate::utils::filesystem::manifest_sync::sync_manifested_tree;

use super::downloader::{SourceDownloader, cache_key};

impl<'a> SourceDownloader<'a> {
    pub(super) fn resolve_workspace_root(extracted_path: &Path) -> Result<PathBuf> {
        if is_build_root(extracted_path) {
            return Ok(extracted_path.to_path_buf());
        }

        let entries = std::fs::read_dir(extracted_path).context(format!(
            "Failed to scan extracted source root '{}'",
            extracted_path.display()
        ))?;

        let mut candidates = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir() && is_build_root(path))
            .collect::<Vec<_>>();

        match candidates.len() {
            0 => Ok(extracted_path.to_path_buf()),
            1 => Ok(candidates.remove(0)),
            _ => {
                candidates.sort();
                let listed = candidates
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");

                Err(anyhow!(
                    "Build source root is ambiguous under '{}': found multiple candidate repositories [{}]",
                    extracted_path.display(),
                    listed
                ))
            }
        }
    }

    pub(super) fn cache_archive_workspace(
        &self,
        repo_slug: &str,
        provider: &Provider,
        base_url: Option<&str>,
        git_ref: &str,
        workspace_path: &Path,
        status_callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<PathBuf> {
        let cache_root = self.source_archive_cache_dir.join(cache_key(
            base_url,
            provider,
            &format!("{repo_slug}/{git_ref}"),
        ));

        let destination = cache_root.join("workspace");
        let manifest_path = cache_root.join("manifest.json");
        Self::emit_status(
            status_callback,
            format!("Syncing source archive cache '{}'", destination.display()),
        );
        sync_manifested_tree(workspace_path, &destination, &manifest_path).context(format!(
            "Failed to sync source archive cache for '{}' at '{}'",
            repo_slug,
            destination.display()
        ))?;
        Ok(destination)
    }
}

fn is_build_root(path: &Path) -> bool {
    ["Cargo.toml", "go.mod", "build.zig", "CMakeLists.txt"]
        .iter()
        .any(|name| path.join(name).is_file())
        || std::fs::read_dir(path).ok().is_some_and(|entries| {
            entries.flatten().any(|entry| {
                entry.path().extension().is_some_and(|ext| {
                    let ext = ext.to_string_lossy();
                    ext.eq_ignore_ascii_case("sln") || ext.eq_ignore_ascii_case("csproj")
                })
            })
        })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::SourceDownloader;
    use super::is_build_root;

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);

        std::env::temp_dir().join(format!("upstream-downloader-test-{name}-{nanos}"))
    }

    fn fixture_path(relative: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(relative)
    }

    fn copy_fixture(relative: &str, destination: &Path) {
        fs::create_dir_all(destination.parent().expect("fixture parent"))
            .expect("create fixture parent");
        fs::copy(fixture_path(relative), destination).expect("copy fixture");
    }

    #[test]
    fn build_root_markers_are_detected() {
        let root = temp_root("markers");
        copy_fixture("builder/rust-single.Cargo.toml", &root.join("Cargo.toml"));
        assert!(is_build_root(&root));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_workspace_root_uses_root_when_manifest_exists() {
        let root = temp_root("rust-single");
        copy_fixture("builder/rust-single.Cargo.toml", &root.join("Cargo.toml"));
        assert_eq!(
            SourceDownloader::resolve_workspace_root(&root).expect("resolve"),
            root
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_workspace_root_selects_single_child_repo() {
        let root = temp_root("single-child");
        copy_fixture(
            "builder/pax-noise.Cargo.toml",
            &root.join("child/Cargo.toml"),
        );
        fs::write(root.join("pax_global_header"), "").expect("write noise");
        assert_eq!(
            SourceDownloader::resolve_workspace_root(&root).expect("resolve"),
            root.join("child")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_workspace_root_errors_on_ambiguous_children() {
        let root = temp_root("ambiguous");
        copy_fixture("builder/ambiguous-multi.go.mod", &root.join("go/go.mod"));
        copy_fixture(
            "builder/ambiguous-multi.Cargo.toml",
            &root.join("rust/Cargo.toml"),
        );
        let error = SourceDownloader::resolve_workspace_root(&root).expect_err("ambiguous");
        assert!(error.to_string().contains("ambiguous"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_workspace_root_returns_input_without_candidates() {
        let root = temp_root("no-candidates");
        fs::create_dir_all(&root).expect("create root");
        fs::write(root.join("README.md"), "hello").expect("write readme");
        assert_eq!(
            SourceDownloader::resolve_workspace_root(&root).expect("resolve"),
            root
        );

        let _ = fs::remove_dir_all(root);
    }
}
