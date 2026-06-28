use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};

use crate::models::common::{enums::Channel, version::Version};
use crate::providers::provider_manager::ProviderManager;
use crate::routines::build::determine::determine_profile;
use crate::routines::build::downloader::SourceDownloader;
use crate::routines::build::profiles::handlers;
use crate::routines::build::{BuildOutput, BuildRequest, scripts};
use crate::utils::static_paths::UpstreamPaths;

pub struct BuildWorker<'a> {
    provider_manager: &'a ProviderManager,
    paths: &'a UpstreamPaths,
}

impl<'a> BuildWorker<'a> {
    pub fn new(provider_manager: &'a ProviderManager, paths: &'a UpstreamPaths) -> Self {
        Self {
            provider_manager,
            paths,
        }
    }

    pub async fn build<H>(
        &self,
        request: BuildRequest,
        channel: Channel,
        line_callback: &mut Option<H>,
    ) -> Result<BuildOutput>
    where
        H: FnMut(&str),
    {
        Self::emit_status(line_callback, "Preparing source checkout ...");
        let downloader = SourceDownloader::new(self.provider_manager, self.paths)?;
        let source = {
            let mut status_callback = line_callback
                .as_mut()
                .map(|callback| callback as &mut dyn FnMut(&str));
            downloader
                .fetch_source(
                    &request.repo_slug,
                    &request.provider,
                    request.base_url.as_deref(),
                    &channel,
                    request.version_tag.as_deref(),
                    request.branch.as_deref(),
                    &mut status_callback,
                )
                .await?
        };

        Self::emit_status(line_callback, "Detecting build profile ...");
        let profile_handlers = handlers();
        let profile = determine_profile(
            &source.workspace_path,
            request.requested_profile,
            &profile_handlers,
        )
        .map_err(|err| anyhow!("{} (workspace: '{}')", err, source.workspace_path.display()))?;

        Self::emit_status(
            line_callback,
            format!("Building with {profile:?} profile ..."),
        );
        let (build_tx, mut build_rx) = tokio::sync::mpsc::unbounded_channel();
        let workspace_path = source.workspace_path.clone();
        let package_name = request.name.clone();
        let mut build_handle = tokio::task::spawn_blocking(move || {
            let handlers = handlers();
            let selected = handlers
                .iter()
                .find(|handler| handler.profile() == profile)
                .ok_or_else(|| anyhow!("Unsupported build profile"))?;
            let mut sender_callback = |line: &str| {
                let _ = build_tx.send(line.to_string());
            };
            let mut build_line_callback: Option<&mut dyn FnMut(&str)> = Some(&mut sender_callback);

            selected.run_build(&workspace_path, &package_name, &mut build_line_callback)
        });

        let artifact = loop {
            tokio::select! {
                Some(line) = build_rx.recv() => {
                    Self::emit_status(line_callback, line);
                }
                result = &mut build_handle => {
                    while let Ok(line) = build_rx.try_recv() {
                        Self::emit_status(line_callback, line);
                    }
                    break result.context("Build task failed")??;
                }
            }
        };
        if scripts::script_for(request.script_action, &source.workspace_path).is_some() {
            Self::emit_status(line_callback, "Running build scripts ...");
            let build_script_callback = line_callback
                .as_mut()
                .map(|callback| callback as &mut dyn FnMut(&str));
            scripts::run_build_script(
                request.script_action,
                &source.workspace_path,
                build_script_callback,
            )?;
        }
        Self::emit_status(line_callback, "Staging built artifact ...");
        let persisted_artifact = Self::persist_artifact(&artifact)?;

        let version = if source.release.version == Version::new(0, 0, 0, false) {
            Version::from_tag(&source.release.tag).unwrap_or_else(|_| Version::new(0, 0, 0, false))
        } else {
            source.release.version.clone()
        };

        Ok(BuildOutput {
            artifact_path: persisted_artifact,
            profile,
            release: source.release,
            version,
            branch: source.branch,
            commit: source.commit,
        })
    }

    fn emit_status<H>(line_callback: &mut Option<H>, status: impl AsRef<str>)
    where
        H: FnMut(&str),
    {
        if let Some(callback) = line_callback.as_mut() {
            callback(status.as_ref());
        }
    }

    fn persist_artifact(artifact_path: &Path) -> Result<PathBuf> {
        let file_name = artifact_path
            .file_name()
            .ok_or_else(|| anyhow!("Built artifact path has no filename"))?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let persist_dir = std::env::temp_dir().join(format!("upstream-artifact-{nonce}"));
        fs::create_dir_all(&persist_dir).context(format!(
            "Failed to create artifact staging directory '{}'",
            persist_dir.display()
        ))?;

        let persisted_path = persist_dir.join(file_name);
        fs::copy(artifact_path, &persisted_path).context(format!(
            "Failed to stage built artifact from '{}' to '{}'",
            artifact_path.display(),
            persisted_path.display()
        ))?;

        let perms = fs::metadata(artifact_path)
            .context(format!(
                "Failed to read built artifact metadata '{}'",
                artifact_path.display()
            ))?
            .permissions();
        fs::set_permissions(&persisted_path, perms).context(format!(
            "Failed to preserve artifact permissions on '{}'",
            persisted_path.display()
        ))?;

        Ok(persisted_path)
    }
}

#[cfg(test)]
mod tests {
    use super::BuildWorker;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, path::PathBuf};

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-worker-test-{name}-{nanos}"))
    }

    #[test]
    fn persist_artifact_copies_file_to_stable_temp_path() {
        let root = temp_root("persist-artifact");
        fs::create_dir_all(&root).expect("create temp root");
        let src = root.join("tool");
        let mut f = fs::File::create(&src).expect("create source artifact");
        f.write_all(b"binary-data").expect("write source artifact");

        let persisted = BuildWorker::persist_artifact(&src).expect("persist artifact");
        assert!(persisted.exists());
        assert_eq!(
            fs::read(&persisted).expect("read persisted"),
            b"binary-data"
        );
        assert_ne!(persisted, src);

        let _ = fs::remove_dir_all(&root);
        if let Some(parent) = persisted.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }
}
