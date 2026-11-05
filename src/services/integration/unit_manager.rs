use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::{
    models::upstream::Package, providers::assets::filenames::filesystem_name,
    utils::static_paths::UpstreamPaths,
};

pub struct UnitManager<'a> {
    paths: &'a UpstreamPaths,
}

impl<'a> UnitManager<'a> {
    pub fn new(paths: &'a UpstreamPaths) -> Self {
        Self { paths }
    }

    pub fn is_unit_asset(name: &str) -> bool {
        unit_filename(Path::new(name))
    }

    pub async fn stage_release_assets(
        release: &crate::models::provider::Release,
        manager: &crate::providers::provider_manager::ProviderManager,
        provider: &crate::models::common::enums::Provider,
        cache: &Path,
        destination: &Path,
    ) -> Result<usize> {
        if !cfg!(target_os = "linux") {
            return Ok(0);
        }

        for asset in release
            .assets
            .iter()
            .filter(|asset| Self::is_unit_asset(&asset.name))
        {
            let mut no_progress: Option<fn(u64, u64)> = None;
            manager
                .download_asset(asset, provider, cache, &mut no_progress)
                .await?;
        }

        Self::stage_from_root(cache, destination)
    }

    fn package_dir(&self, id: &str) -> PathBuf {
        self.paths
            .dirs
            .state_dir
            .join("units")
            .join(filesystem_name(id))
    }

    /// Copy only explicit user units, never system units or unrendered templates.
    pub fn stage_from_root(root: &Path, destination: &Path) -> Result<usize> {
        if !cfg!(target_os = "linux") || !root.is_dir() {
            return Ok(0);
        }

        let mut discovered = Vec::new();
        for entry in walkdir::WalkDir::new(root)
            .follow_links(false)
            .max_depth(12)
            .into_iter()
            .filter_entry(|entry| {
                !matches!(
                    entry.file_name().to_str(),
                    Some(".git" | "node_modules" | "target" | ".venv")
                )
            })
        {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }

            let relative = entry.path().strip_prefix(root)?;
            let parts = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>();

            // Locations such as lib/systemd/user and etc/linux-systemd/user.
            let user_directory = parts.windows(2).any(|pair| {
                (pair[0] == "systemd" || pair[0] == "linux-systemd") && pair[1] == "user"
            });

            let system_directory = parts.windows(2).any(|pair| {
                (pair[0] == "systemd" || pair[0] == "linux-systemd") && pair[1] == "system"
            });

            if system_directory || !unit_filename(entry.path()) {
                continue;
            }

            let content = fs::read_to_string(entry.path())?;
            // Flat assets can identify user units through their install target
            // or a home-relative executable search path.
            if !valid_unit(&content) {
                continue;
            }

            let explicit = user_directory || user_target(&content) || user_search_path(&content);
            discovered.push((entry.into_path(), content, explicit));
        }

        // An identified user unit also identifies companions with the same stem.
        let companion_groups = discovered
            .iter()
            .filter(|(_, _, explicit)| *explicit)
            .map(|(path, _, _)| (path.parent(), path.file_stem()))
            .collect::<Vec<_>>();

        let mut candidates = BTreeMap::new();
        for (path, content, explicit) in &discovered {
            if !explicit && !companion_groups.contains(&(path.parent(), path.file_stem())) {
                continue;
            }

            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();

            if let Some(previous) = candidates.insert(name.clone(), content.clone())
                && previous != *content
            {
                bail!(
                    "Conflicting user units named '{name}' in '{}'",
                    root.display()
                );
            }
        }

        if candidates.is_empty() {
            return Ok(0);
        }

        fs::create_dir_all(destination)?;
        let count = candidates.len();
        for (name, content) in &candidates {
            let path = destination.join(name);
            if path.exists() && fs::read_to_string(&path)? != *content {
                bail!("Conflicting staged user unit '{}'", path.display());
            }
        }

        for (name, content) in candidates {
            fs::write(destination.join(name), content)?;
        }

        Ok(count)
    }

    /// Install files into the user-unit load path; keep an ownership copy.
    pub fn install(&self, package: &Package, staged: &Path) -> Result<Vec<String>> {
        if !cfg!(target_os = "linux") {
            return Ok(Vec::new());
        }

        if !crate::storage::system::config::ConfigStorage::new(&self.paths.config.config_file)?
            .get_config()
            .integration
            .systemd_units
        {
            return Ok(Vec::new());
        }

        let mut units = Vec::new();
        for entry in fs::read_dir(staged)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() || !unit_filename(&entry.path()) {
                continue;
            }

            let name = entry.file_name().to_string_lossy().into_owned();
            let content = fs::read_to_string(entry.path())?;
            let content = adapt_commands(&content, package);
            let content = format!("{}\n{content}", owner_marker(&package.id));
            let target = self.paths.integration.user_units_dir.join(&name);
            if fs::symlink_metadata(&target).is_ok() {
                bail!(
                    "User unit '{}' already exists; leaving it unchanged",
                    target.display()
                );
            }

            units.push((name, content));
        }

        if units.is_empty() {
            return Ok(Vec::new());
        }

        units.sort_by(|a, b| a.0.cmp(&b.0));
        let owned = self.package_dir(&package.id);
        fs::create_dir_all(&owned)?;
        fs::create_dir_all(&self.paths.integration.user_units_dir)?;
        let mut created = Vec::new();
        let result = (|| {
            for (name, content) in &units {
                // Record ownership before touching the live path so partial activation is undoable.
                fs::write(owned.join(name), content)?;
                use std::io::Write;
                let target = self.paths.integration.user_units_dir.join(name);
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)
                    .with_context(|| {
                        format!("Failed to install user unit '{}'", target.display())
                    })?;

                created.push(target);
                file.write_all(content.as_bytes())?;
            }

            Ok::<(), anyhow::Error>(())
        })();

        if let Err(error) = result {
            for path in created {
                let _ = fs::remove_file(path);
            }

            return Err(error);
        }

        Ok(units.into_iter().map(|(name, _)| name).collect())
    }

    /// Only return files bearing this package's ownership marker.
    pub fn installed_paths(&self, id: &str) -> Result<Vec<PathBuf>> {
        if !cfg!(target_os = "linux") {
            return Ok(Vec::new());
        }

        let owned = self.package_dir(id);
        let entries = match fs::read_dir(&owned) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };

        let mut paths = Vec::new();
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_file() || !unit_filename(&entry.path()) {
                continue;
            }

            let target = self
                .paths
                .integration
                .user_units_dir
                .join(entry.file_name());

            if fs::symlink_metadata(&target)
                .is_ok_and(|meta| meta.is_file() && !meta.file_type().is_symlink())
                && fs::read_to_string(&target)?.lines().next() == Some(owner_marker(id).as_str())
            {
                paths.push(target);
            }
        }

        paths.sort();
        Ok(paths)
    }

    pub fn backup_paths(&self, id: &str) -> Result<Vec<PathBuf>> {
        let mut paths = self.installed_paths(id)?;
        if cfg!(target_os = "linux") {
            paths.push(self.package_dir(id));
        }

        Ok(paths)
    }

    pub fn remove(&self, id: &str) -> Result<bool> {
        let paths = self.installed_paths(id)?;
        let changed = !paths.is_empty();
        for path in paths {
            fs::remove_file(path)?;
        }

        let owned = self.package_dir(id);
        if cfg!(target_os = "linux") && owned.exists() {
            fs::remove_dir_all(owned)?;
        }

        Ok(changed)
    }

    pub fn availability_notice(&self, id: &str) -> Result<Option<String>> {
        use std::fmt::Write;
        let paths = self.installed_paths(id)?;
        if paths.is_empty() {
            return Ok(None);
        }

        let mut notice = String::from("systemd user units available:\n");
        let mut commands = Vec::new();
        for path in paths {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            writeln!(notice, "  {name}").expect("write unit notice");

            // Templates need an instance before they can be started.
            if !name.contains("@.") && has_install_rules(&fs::read_to_string(&path)?) {
                let quoted_name = name.replace('\'', "'\\''");
                commands.push(format!("  systemctl --user enable --now '{quoted_name}'"));
            }
        }

        if !commands.is_empty() {
            notice.push_str("Start with:\n");
            notice.push_str(&commands.join("\n"));
        } else {
            notice.pop();
        }

        Ok(Some(notice))
    }

    /// Emit a persistent post-install notice, even when transient progress is hidden.
    pub fn notify_available(&self, id: &str) {
        let Ok(Some(notice)) = self.availability_notice(id) else {
            return;
        };

        eprintln!("{notice}");
        self.reload_after_change();
    }

    /// Reloading is best effort, including on hosts without a running user manager.
    pub fn reload_after_change(&self) {
        match self.reload_if_enabled() {
            Ok(true) => {}
            Ok(false) => eprintln!("  Reload manually: systemctl --user daemon-reload"),
            Err(error) => eprintln!(
                "  User units changed; automatic reload failed ({error:#}). Run: systemctl --user daemon-reload"
            ),
        }
    }

    fn reload_if_enabled(&self) -> Result<bool> {
        self.reload_with(|| {
            let output = std::process::Command::new("systemctl")
                .args(["--user", "--no-ask-password", "daemon-reload"])
                .stdin(std::process::Stdio::null())
                .output()
                .context("Unable to run systemctl --user daemon-reload")?;

            if !output.status.success() {
                bail!(
                    "systemctl --user daemon-reload: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }

            Ok(())
        })
    }

    fn reload_with(&self, reload: impl FnOnce() -> Result<()>) -> Result<bool> {
        if !cfg!(target_os = "linux") {
            return Ok(false);
        }

        let config =
            crate::storage::system::config::ConfigStorage::new(&self.paths.config.config_file)?;

        if !config.get_config().integration.systemd_daemon_reload {
            return Ok(false);
        }

        reload()?;
        Ok(true)
    }
}

fn owner_marker(id: &str) -> String {
    format!("# Managed by upstream: {id}")
}

pub(crate) fn unit_filename(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("service" | "socket" | "timer" | "path" | "target")
    )
}

fn valid_unit(content: &str) -> bool {
    content.lines().any(|line| {
        matches!(
            line.trim(),
            "[Service]" | "[Socket]" | "[Timer]" | "[Path]" | "[Unit]"
        )
    }) && !content.contains("@CMAKE_")
        && !content.contains("@prefix@")
}

fn user_target(content: &str) -> bool {
    content
        .lines()
        .filter_map(|line| line.trim().split_once('='))
        .any(|(key, value)| {
            matches!(key.trim(), "WantedBy" | "RequiredBy")
                && value
                    .split_whitespace()
                    .any(|target| matches!(target, "default.target" | "graphical-session.target"))
        })
}

fn user_search_path(content: &str) -> bool {
    let mut in_service = false;
    let mut paths = Vec::new();
    for line in content.lines().map(str::trim) {
        if line.starts_with('[') {
            in_service = line == "[Service]";
            continue;
        }

        if in_service
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == "ExecSearchPath"
        {
            // Empty assignments reset paths supplied by earlier directives.
            if value.trim().is_empty() {
                paths.clear();
            } else {
                paths.extend(value.trim().split(':'));
            }
        }
    }

    paths.iter().any(|path| path.starts_with("%h/"))
}

fn has_install_rules(content: &str) -> bool {
    let mut in_install = false;
    let mut rules = BTreeMap::new();
    for line in content.lines().map(str::trim) {
        if line.starts_with(['#', ';']) {
            continue;
        }

        if line.starts_with('[') {
            in_install = line == "[Install]";
            continue;
        }

        if in_install && let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            if matches!(
                key,
                "WantedBy" | "RequiredBy" | "UpheldBy" | "Alias" | "Also"
            ) {
                // Empty assignments reset earlier values for the same directive.
                rules.insert(key, !value.trim().is_empty());
            }
        }
    }

    rules.values().any(|present| *present)
}

fn adapt_commands(content: &str, package: &Package) -> String {
    let mut output = String::new();
    for line in content.lines() {
        let Some((key, value)) = line.split_once('=') else {
            output.push_str(line);
            output.push('\n');
            continue;
        };

        if !matches!(
            key.trim(),
            "ExecStart"
                | "ExecStartPre"
                | "ExecStartPost"
                | "ExecStop"
                | "ExecStopPost"
                | "ExecReload"
        ) {
            output.push_str(line);
            output.push('\n');
            continue;
        }

        let value = value.trim_start();
        let command = value.trim_start_matches(['-', '@', ':', '+', '!']);
        let prefix = &value[..value.len() - command.len()];
        let (token, remainder) =
            if let Some(quote) = command.chars().next().filter(|c| matches!(c, '\'' | '"')) {
                match command[1..].find(quote) {
                    Some(end) => (&command[1..end + 1], &command[end + 2..]),
                    None => (command, ""),
                }
            } else {
                let end = command.find(char::is_whitespace).unwrap_or(command.len());
                (&command[..end], &command[end..])
            };

        let name = Path::new(token).file_name().and_then(|name| name.to_str());
        let executable = package.executables.iter().find(|exec| {
            name == Some(exec.name.as_str())
                || name == exec.path.file_name().and_then(|s| s.to_str())
        });

        if let Some(executable) = executable {
            let path = executable
                .path
                .to_string_lossy()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('%', "%%")
                .replace('$', "$$");

            output.push_str(&format!("{key}={prefix}\"{path}\"{remainder}\n"));
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }

    output
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::{
        models::{
            common::enums::{Channel, Filetype, Provider},
            upstream::PackageExecutable,
        },
        utils::test_support,
    };

    const SERVICE: &str = "[Unit]\nDescription=Tool\n[Service]\nExecStart=/usr/bin/tool --serve\n[Install]\nWantedBy=default.target\n";

    #[test]
    fn discovers_flat_upstream_release_units_and_installs_companion_timer() {
        let root = test_support::temp_root("upstream-unit-test", "flat-release");
        let paths = test_support::upstream_paths(&root);
        let source = root.join("release-assets");
        write(
            &source,
            "upstream-update-check.service",
            include_str!("../../../contrib/systemd/user/upstream-update-check.service"),
        );
        write(
            &source,
            "upstream-update-check.timer",
            include_str!("../../../contrib/systemd/user/upstream-update-check.timer"),
        );
        write(
            &source,
            "system-check.timer",
            "[Timer]\nOnCalendar=daily\n[Install]\nWantedBy=timers.target\n",
        );
        let stage = root.join("stage");
        assert_eq!(UnitManager::stage_from_root(&source, &stage).unwrap(), 2);
        assert!(!stage.join("system-check.timer").exists());

        let manager = UnitManager::new(&paths);
        assert_eq!(manager.install(&package(&paths), &stage).unwrap().len(), 2);
        let notice = manager.availability_notice("tool").unwrap().unwrap();
        assert!(notice.contains("systemctl --user enable --now 'upstream-update-check.timer'"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn user_search_paths_require_an_active_service_directive() {
        assert!(user_search_path(
            "[Service]\nExecSearchPath=%h/.local/bin:/usr/bin\n"
        ));

        for content in [
            "[Service]\n# ExecSearchPath=%h/.local/bin\n",
            "[Unit]\nExecSearchPath=%h/.local/bin\n",
            "[Service]\nExecSearchPath=/usr/bin\n",
            "[Service]\nExecSearchPath=%h/.local/bin\nExecSearchPath=\n",
            "[Service]\nExecSearchPath=%%h/bin\n",
        ] {
            assert!(!user_search_path(content), "{content}");
        }
    }

    fn package(paths: &UpstreamPaths) -> Package {
        let mut package = Package::with_defaults(
            "tool".into(),
            "owner/tool".into(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );

        package.install_path = Some(paths.install.binaries_dir.join("tool"));
        package.executables = vec![PackageExecutable {
            name: "tool".into(),
            path: package.install_path.clone().unwrap(),
        }];
        package
    }

    fn write(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn install_rules_require_nonempty_install_directives() {
        for key in ["WantedBy", "RequiredBy", "UpheldBy", "Alias", "Also"] {
            assert!(has_install_rules(&format!(
                "[Install]\n{key}=tool.target\n"
            )));

            assert!(!has_install_rules(&format!("[Unit]\n{key}=tool.target\n")));
            assert!(!has_install_rules(&format!(
                "[Install]\n{key}=tool.target\n{key}=\n"
            )));
        }

        assert!(!has_install_rules(
            "[Install]\n# WantedBy=default.target\n; Alias=tool.service\nDefaultInstance=main\n"
        ));

        assert!(has_install_rules(
            "[Install]\nWantedBy=default.target\nAlias=\n"
        ));
    }

    #[test]
    fn availability_notice_only_suggests_enabling_installable_units() {
        let root = test_support::temp_root("upstream-unit-test", "notice");
        let paths = test_support::upstream_paths(&root);
        let package = package(&paths);
        let stage = root.join("stage");
        write(
            &stage,
            "upstream-update-check.service",
            include_str!("../../../contrib/systemd/user/upstream-update-check.service"),
        );
        write(
            &stage,
            "upstream-update-check.timer",
            include_str!("../../../contrib/systemd/user/upstream-update-check.timer"),
        );
        write(&stage, "tool@.service", SERVICE);
        let manager = UnitManager::new(&paths);
        manager.install(&package, &stage).unwrap();
        assert_eq!(
            manager.availability_notice(&package.id).unwrap().unwrap(),
            "systemd user units available:\n  tool@.service\n  upstream-update-check.service\n  upstream-update-check.timer\nStart with:\n  systemctl --user enable --now 'upstream-update-check.timer'"
        );

        // With only static units, there is no enable instruction.
        fs::remove_file(
            paths
                .integration
                .user_units_dir
                .join("upstream-update-check.timer"),
        )
        .unwrap();
        let notice = manager.availability_notice(&package.id).unwrap().unwrap();
        assert!(notice.contains("upstream-update-check.service"));
        assert!(!notice.contains("Start with:"));
        assert!(!notice.contains("systemctl"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovers_user_directories_flat_assets_and_companions_but_excludes_system_units() {
        let root = test_support::temp_root("upstream-unit-test", "discovery");
        let source = root.join("source");
        write(&source, "etc/linux-systemd/user/tool@.service", SERVICE);
        write(
            &source,
            "usr/lib/systemd/user/tool.socket",
            "[Socket]\nListenStream=%t/tool\n",
        );
        write(&source, "lib/systemd/system/system.service", SERVICE);
        write(
            &source,
            "flat/check.timer",
            "[Timer]\nOnCalendar=daily\n[Install]\nWantedBy=default.target\n",
        );
        write(
            &source,
            "flat/check.service",
            "[Service]\nExecStart=/usr/bin/tool --check\n",
        );
        write(
            &source,
            "unknown.service",
            "[Service]\nExecStart=/usr/bin/unknown\n",
        );
        write(
            &source,
            "system.service",
            &SERVICE.replace("default.target", "multi-user.target"),
        );
        write(
            &source,
            "etc/linux-systemd/user/template.service.in",
            SERVICE,
        );
        let stage = root.join("stage");
        assert_eq!(UnitManager::stage_from_root(&source, &stage).unwrap(), 4);
        for name in [
            "tool@.service",
            "tool.socket",
            "check.timer",
            "check.service",
        ] {
            assert!(stage.join(name).is_file());
        }

        assert!(!stage.join("system.service").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn install_adapts_commands_preserves_names_and_removes_only_owned_files() {
        let root = test_support::temp_root("upstream-unit-test", "ownership");
        let paths = test_support::upstream_paths(&root);
        let package = package(&paths);
        let stage = root.join("stage");
        write(&stage, "tool.service", SERVICE);
        write(&stage, "tool.timer", "[Timer]\nOnCalendar=daily\n");
        let manager = UnitManager::new(&paths);
        assert_eq!(
            manager.install(&package, &stage).unwrap(),
            ["tool.service", "tool.timer"]
        );

        let content =
            fs::read_to_string(paths.integration.user_units_dir.join("tool.service")).unwrap();

        assert!(content.contains(&format!(
            "ExecStart=\"{}\" --serve",
            package.executables[0].path.display()
        )));

        assert!(
            !paths
                .dirs
                .user_dir
                .join(".config/systemd/user/default.target.wants")
                .exists()
        );

        write(
            &paths.integration.user_units_dir,
            "unrelated.service",
            SERVICE,
        );
        let notice = manager.availability_notice(&package.id).unwrap().unwrap();
        assert!(notice.contains("systemd user units available"));
        assert!(notice.contains("tool.service"));
        assert!(notice.contains("systemctl --user enable --now 'tool.service'"));
        assert!(!notice.contains("systemctl --user enable --now 'tool.timer'"));
        assert!(manager.remove(&package.id).unwrap());
        assert!(
            !paths
                .integration
                .user_units_dir
                .join("tool.service")
                .exists()
        );

        assert!(
            paths
                .integration
                .user_units_dir
                .join("unrelated.service")
                .exists()
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn collisions_are_checked_before_installing_any_units() {
        let root = test_support::temp_root("upstream-unit-test", "collision");
        let paths = test_support::upstream_paths(&root);
        let package = package(&paths);
        let stage = root.join("stage");
        write(&stage, "tool.service", SERVICE);
        write(&stage, "other.service", SERVICE);
        write(
            &paths.integration.user_units_dir,
            "tool.service",
            "user supplied",
        );
        let manager = UnitManager::new(&paths);
        assert!(manager.install(&package, &stage).is_err());
        assert_eq!(
            fs::read_to_string(paths.integration.user_units_dir.join("tool.service")).unwrap(),
            "user supplied"
        );

        assert!(
            !paths
                .integration
                .user_units_dir
                .join("other.service")
                .exists()
        );

        assert!(manager.installed_paths(&package.id).unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn replaced_files_and_symlinks_are_not_removed() {
        let root = test_support::temp_root("upstream-unit-test", "replaced");
        let paths = test_support::upstream_paths(&root);
        let package = package(&paths);
        let stage = root.join("stage");
        write(&stage, "tool.service", SERVICE);
        let manager = UnitManager::new(&paths);
        manager.install(&package, &stage).unwrap();
        let target = paths.integration.user_units_dir.join("tool.service");
        fs::remove_file(&target).unwrap();
        let external = root.join("external.service");
        fs::write(&external, "external").unwrap();
        std::os::unix::fs::symlink(&external, &target).unwrap();
        assert!(!manager.remove(&package.id).unwrap());
        assert!(target.is_symlink());
        assert_eq!(fs::read_to_string(external).unwrap(), "external");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn command_rewriting_preserves_prefixes_arguments_and_unrelated_helpers() {
        let root = test_support::temp_root("upstream-unit-test", "rewrite");
        let paths = test_support::upstream_paths(&root);
        let mut package = package(&paths);
        package.executables[0].path = PathBuf::from("/home/me/my apps/tool%1.AppImage");
        let content = "[Service]\nExecStart=-@\"/usr/bin/tool\" custom-name --serve\nExecStop=/usr/bin/helper stop\nEnvironment=EXEC=/usr/bin/tool\n";
        let actual = adapt_commands(content, &package);
        assert!(
            actual
                .contains("ExecStart=-@\"/home/me/my apps/tool%%1.AppImage\" custom-name --serve")
        );

        assert!(actual.contains("ExecStop=/usr/bin/helper stop"));
        assert!(actual.contains("Environment=EXEC=/usr/bin/tool"));
    }

    #[test]
    fn reload_policy_defaults_on_and_can_be_disabled_without_running_a_command() {
        let root = test_support::temp_root("upstream-unit-test", "reload-config");
        let paths = test_support::upstream_paths(&root);
        let manager = UnitManager::new(&paths);
        let mut calls = 0;
        assert!(
            manager
                .reload_with(|| {
                    calls += 1;
                    Ok(())
                })
                .unwrap()
        );

        assert_eq!(calls, 1);
        assert!(
            manager
                .reload_with(|| anyhow::bail!("no user manager"))
                .is_err()
        );

        write(
            paths.config.config_file.parent().unwrap(),
            "config.toml",
            "[integration]\nsystemd_daemon_reload = false\n",
        );
        assert!(
            !manager
                .reload_with(|| panic!("disabled reload must not execute systemctl"))
                .unwrap()
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disabling_units_skips_installation_but_still_removes_owned_units() {
        let root = test_support::temp_root("upstream-unit-test", "disabled-units");
        let paths = test_support::upstream_paths(&root);
        let package = package(&paths);
        let stage = root.join("stage");
        write(&stage, "tool.service", SERVICE);
        let manager = UnitManager::new(&paths);
        manager.install(&package, &stage).unwrap();
        write(
            paths.config.config_file.parent().unwrap(),
            "config.toml",
            "[integration]\nsystemd_units = false\n",
        );
        assert!(
            manager
                .install(&package, &root.join("not-staged"))
                .unwrap()
                .is_empty()
        );

        assert!(manager.remove(&package.id).unwrap());
        assert!(
            !paths
                .integration
                .user_units_dir
                .join("tool.service")
                .exists()
        );

        fs::remove_dir_all(root).unwrap();
    }
}
