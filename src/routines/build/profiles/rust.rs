use crate::routines::build::{
    BuildProfile,
    profiles::{BuildProfileHandler, emit_line_callback, run_command_with_line_callback},
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Command,
};

pub struct RustProfile;
impl RustProfile {
    fn default_packages(
        manifest: &Path,
        workspace: &Path,
        locked: bool,
        callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<HashSet<String>> {
        let mut command = Command::new("cargo");
        command
            .args(["metadata", "--no-deps", "--format-version", "1"])
            .arg("--manifest-path")
            .arg(manifest)
            .current_dir(workspace);
        if locked {
            command.arg("--locked");
        }
        let mut packages = None;
        let mut capture = |line: &str| {
            if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(line)
                && let Some(members) = metadata["workspace_default_members"].as_array()
            {
                packages = Some(
                    members
                        .iter()
                        .filter_map(|id| id.as_str().map(str::to_owned))
                        .collect(),
                );
            } else {
                emit_line_callback(callback, line);
            }
        };
        let status = run_command_with_line_callback(
            &mut command,
            "Failed to run cargo metadata; is Cargo installed?",
            &mut Some(&mut capture),
        )?;
        ensure!(status.success(), "Cargo metadata failed");
        packages.context("Cargo metadata did not report default workspace members")
    }

    fn collect_binaries(mut binaries: Vec<(String, PathBuf)>) -> Result<Vec<PathBuf>> {
        binaries.sort();
        binaries.dedup();
        ensure!(
            !binaries.is_empty(),
            "Rust source produced no executable binary targets with default features"
        );
        let mut names = HashSet::new();
        let mut paths = Vec::new();
        for (_, binary) in binaries {
            let name = binary
                .file_name()
                .context("Invalid Cargo executable path")?;
            ensure!(
                names.insert(name.to_string_lossy().to_lowercase()),
                "Multiple binary targets produce the same executable name: {}",
                name.to_string_lossy()
            );
            paths.push(binary);
        }
        Ok(paths)
    }
}
impl BuildProfileHandler for RustProfile {
    fn profile(&self) -> BuildProfile {
        BuildProfile::Rust
    }
    fn detect(&self, workspace: &Path) -> bool {
        workspace.join("Cargo.toml").is_file()
    }
    fn run_build(
        &self,
        workspace: &Path,
        _package_name: &str,
        callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<Vec<PathBuf>> {
        let manifest = workspace
            .join("Cargo.toml")
            .canonicalize()
            .context("Could not find Cargo.toml in source root")?;
        let locked = workspace.join("Cargo.lock").is_file();
        let packages = Self::default_packages(&manifest, workspace, locked, callback)?;
        let mut command = Command::new("cargo");
        command
            .args([
                "build",
                "--release",
                "--bins",
                "--message-format=json-render-diagnostics",
            ])
            .arg("--manifest-path")
            .arg(&manifest)
            .current_dir(workspace);
        if locked {
            command.arg("--locked");
        } else {
            emit_line_callback(
                callback,
                "Source has no Cargo.lock; resolving dependencies.",
            );
        }
        emit_line_callback(callback, "Building all default Rust binary targets ...");
        let mut binaries = Vec::new();
        let mut capture = |line: &str| {
            if let Ok(message) = serde_json::from_str::<serde_json::Value>(line) {
                if message["reason"] == "compiler-artifact"
                    && let Some(package) = message["package_id"].as_str()
                    && packages.contains(package)
                    && message["target"]["kind"]
                        .as_array()
                        .is_some_and(|k| k.iter().any(|v| v == "bin"))
                    && let Some(executable) = message["executable"].as_str()
                {
                    binaries.push((package.to_owned(), PathBuf::from(executable)));
                }
                if let Some(rendered) = message["message"]["rendered"].as_str() {
                    emit_line_callback(callback, rendered);
                }
            } else {
                emit_line_callback(callback, line);
            }
        };
        let status = run_command_with_line_callback(
            &mut command,
            "Failed to run cargo build; is Cargo installed?",
            &mut Some(&mut capture),
        )?;
        ensure!(status.success(), "Cargo build failed");
        Self::collect_binaries(binaries)
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::{
        models::{
            common::{
                Version,
                enums::{Channel, Filetype, Provider},
            },
            upstream::{InstallType, Package},
        },
        providers::provider_manager::ProviderManager,
        services::packaging::{InstallWorkspace, PackageInstaller},
        utils::test_support,
    };
    use std::fs;

    fn names(binaries: &[PathBuf]) -> Vec<String> {
        assert!(binaries.iter().all(|path| path.is_file()));
        binaries
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    struct Fixture(PathBuf);
    impl Fixture {
        fn new(name: &str) -> Self {
            let root = test_support::temp_root("rust-build", name);
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn write(&self, file: &str, content: &str) {
            let path = self.0.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn builds_and_installs_all_default_commands_directly() {
        let f = Fixture::new("commands");
        f.write("Cargo.toml", "[package]\nname='fixture'\nversion='1.0.0'\nedition='2021'\n[features]\nextra=[]\n[[bin]]\nname='feature-only'\npath='src/gated.rs'\nrequired-features=['extra']\n");
        for path in ["src/main.rs", "src/bin/helper.rs", "src/gated.rs"] {
            f.write(path, "fn main() {}\n");
        }
        let binaries = RustProfile.run_build(&f.0, "fixture", &mut None).unwrap();
        let names = names(&binaries);
        assert!(names.contains(&"fixture".to_owned()));
        assert!(names.contains(&"helper".to_owned()));
        assert!(!names.contains(&"feature-only".to_owned()));
        let paths = test_support::upstream_paths(&f.0.join("home"));
        let manager = ProviderManager::new(None, None, None, Default::default()).unwrap();
        let workspace = InstallWorkspace::new(&paths, "github:owner/fixture").unwrap();
        let installer = PackageInstaller::new_for_workspace(&manager, &paths, workspace).unwrap();
        let mut package = Package::with_defaults(
            "github:owner/fixture".into(),
            "owner/fixture".into(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        package.install_type = InstallType::Build;
        let installed = installer
            .install_local_files_to_workspace(
                package,
                &binaries,
                Version::new(1, 0, 0, false),
                &mut None::<fn(&str)>,
            )
            .unwrap();
        assert_eq!(installed.filetype, Filetype::Binary);
        assert_eq!(installed.executables.len(), 2);
        assert!(installed.executables.iter().all(|e| e.path.is_file()));
        assert!(
            installer
                .install_local_files_to_workspace(
                    installed,
                    std::slice::from_ref(&f.0),
                    Version::new(1, 0, 0, false),
                    &mut None::<fn(&str)>
                )
                .is_err()
        );
    }

    #[test]
    fn honors_default_workspace_members_and_excludes_dependency_binaries() {
        let f = Fixture::new("workspace");
        f.write("Cargo.toml", "[workspace]\nresolver='2'\nmembers=['first','second','extra','dep']\ndefault-members=['first','second']\n");
        for name in ["first", "second", "extra", "dep"] {
            let dependency = if name == "first" {
                "[dependencies]\ndep={path='../dep'}\n"
            } else {
                ""
            };
            f.write(
                &format!("{name}/Cargo.toml"),
                &format!("[package]\nname='{name}'\nversion='1.0.0'\nedition='2021'\n{dependency}"),
            );
            f.write(&format!("{name}/src/main.rs"), "fn main() {}\n");
        }
        f.write("dep/src/lib.rs", "pub fn library() {}\n");
        let binaries = RustProfile
            .run_build(&f.0, "ignored-name-hint", &mut None)
            .unwrap();
        let names = names(&binaries);
        assert!(names.contains(&"first".to_owned()));
        assert!(names.contains(&"second".to_owned()));
        assert!(!names.contains(&"extra".to_owned()));
        assert!(!names.contains(&"dep".to_owned()));
    }

    #[test]
    fn honors_custom_target_directory_and_never_returns_stale_binaries() {
        let f = Fixture::new("rebuild");
        f.write(
            "Cargo.toml",
            "[package]\nname='fixture'\nversion='1.0.0'\nedition='2021'\n",
        );
        f.write(
            ".cargo/config.toml",
            "[build]\ntarget-dir='custom-target'\n",
        );
        f.write("src/main.rs", "fn main() {}\n");
        f.write("src/bin/old.rs", "fn main() {}\n");
        let first = RustProfile.run_build(&f.0, "fixture", &mut None).unwrap();
        assert!(f.0.join("custom-target/release/old").is_file());
        fs::remove_file(f.0.join("src/bin/old.rs")).unwrap();
        f.write("src/bin/new.rs", "fn main() {}\n");
        let second = RustProfile.run_build(&f.0, "fixture", &mut None).unwrap();
        assert_ne!(first, second);
        let names = names(&second);
        assert!(names.contains(&"fixture".to_owned()));
        assert!(names.contains(&"new".to_owned()));
        assert!(!names.contains(&"old".to_owned()));
    }

    #[test]
    fn reports_no_enabled_binaries_and_duplicate_output_names() {
        let f = Fixture::new("library");
        f.write(
            "Cargo.toml",
            "[package]\nname='fixture'\nversion='1.0.0'\nedition='2021'\n",
        );
        f.write("src/lib.rs", "pub fn library() {}\n");
        assert!(
            RustProfile
                .run_build(&f.0, "fixture", &mut None)
                .unwrap_err()
                .to_string()
                .contains("no executable")
        );
        f.write("one/tool", "first");
        f.write("two/tool", "second");
        assert!(
            RustProfile::collect_binaries(vec![
                ("one".into(), f.0.join("one/tool")),
                ("two".into(), f.0.join("two/tool"))
            ])
            .unwrap_err()
            .to_string()
            .contains("same executable name")
        );
    }
}
