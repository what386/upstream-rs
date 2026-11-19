use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};

use crate::application::cancellation;
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
        cancellation::check()?;
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
        cancellation::check()?;
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
        cancellation::check()?;
        let (build_tx, mut build_rx) = tokio::sync::mpsc::unbounded_channel();
        let workspace_path = source.workspace_path.clone();
        let target_hint = request.target_hint.clone();
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

            selected.run_build(&workspace_path, &target_hint, &mut build_line_callback)
        });

        let artifact = loop {
            if cancellation::is_requested() {
                // A blocking build cannot be stopped by aborting its JoinHandle.
                // Let the cancellable subprocess runner terminate and reap the
                // child, then wait for the blocking closure to finish.
                let _ = (&mut build_handle).await;
                return Err(crate::application::cancellation::Cancelled.into());
            }

            tokio::select! {
                _ = cancellation::cancelled() => {
                    let _ = (&mut build_handle).await;
                    return Err(crate::application::cancellation::Cancelled.into());
                }
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
            cancellation::check()?;
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
        cancellation::check()?;
        let persisted_binaries = Self::persist_binaries(&artifact)?;
        let integration =
            crate::storage::system::config::ConfigStorage::new(&self.paths.config.config_file)?
                .get_config()
                .integration;

        if integration.systemd_units
            && let Some(parent) = persisted_binaries.first().and_then(|path| path.parent())
            && let Err(error) = crate::services::integration::UnitManager::stage_from_root(
                &source.workspace_path,
                &parent.join("systemd/user"),
            )
        {
            Self::emit_status(
                line_callback,
                format!("User unit discovery skipped: {error:#}"),
            );
        }

        let version = if source.release.version == Version::new(0, 0, 0, false) {
            Version::from_tag(&source.release.tag).unwrap_or_else(|_| Version::new(0, 0, 0, false))
        } else {
            source.release.version.clone()
        };

        Ok(BuildOutput {
            binaries: persisted_binaries,
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

    fn persist_binaries(binaries: &[PathBuf]) -> Result<Vec<PathBuf>> {
        anyhow::ensure!(!binaries.is_empty(), "Build produced no executable files");
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let directory =
            std::env::temp_dir().join(format!("upstream-artifact-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&directory)?;
        let result = (|| {
            let mut staged = Vec::new();
            for binary in binaries {
                anyhow::ensure!(
                    binary.is_file(),
                    "Built binary '{}' is not a file",
                    binary.display()
                );
                let filename = binary
                    .file_name()
                    .ok_or_else(|| anyhow!("Built binary has no filename"))?;
                let destination = directory.join(filename);
                anyhow::ensure!(
                    !destination.exists(),
                    "Duplicate built binary: {}",
                    filename.to_string_lossy()
                );
                fs::copy(binary, &destination)
                    .with_context(|| format!("Failed to stage '{}'", binary.display()))?;
                staged.push(destination);
            }
            Ok(staged)
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&directory);
        }
        result
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

        let persisted = BuildWorker::persist_binaries(std::slice::from_ref(&src))
            .expect("persist artifact")
            .remove(0);
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
