use crate::storage::database::PackageDatabase;
#[cfg(unix)]
use crate::utils::filesystem::atomic_ops::write_atomic;
use anyhow::{Context, Result};
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
#[cfg(unix)]
use std::sync::{Mutex, OnceLock};

/// Process-global lock used to serialize reads/writes to the shared PATH file.
#[cfg(unix)]
fn paths_file_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(windows)]
fn normalize_windows_path(path: &str) -> String {
    let mut normalized = path.replace('/', "\\").trim().to_ascii_lowercase();
    while normalized.ends_with('\\') {
        normalized.pop();
    }
    normalized
}

pub struct ShellManager<'a> {
    paths_file: &'a Path,
    #[cfg(unix)]
    paths_nu_file: PathBuf,
}

impl<'a> ShellManager<'a> {
    pub fn new(paths_file: &'a Path) -> Self {
        Self {
            paths_file,
            #[cfg(unix)]
            paths_nu_file: paths_file.with_extension("nu"),
        }
    }

    #[cfg(unix)]
    pub fn regenerate_paths(
        &self,
        package_database: &mut PackageDatabase,
        paths: &crate::utils::static_paths::UpstreamPaths,
    ) -> Result<()> {
        let _guard = paths_file_lock()
            .lock()
            .ok()
            .ok_or_else(|| anyhow::anyhow!("Failed to lock PATH file for writing"))?;
        let path_entries = derive_path_entries(package_database, paths)?;
        package_database.replace_all_path_entries(&path_entries)?;

        let rendered_paths = render_path_entries(&paths.state.symlinks_dir, &path_entries);
        let posix_content = render_posix_paths_file(&rendered_paths);
        write_atomic(self.paths_file, posix_content.as_bytes())
            .context("Failed to write paths file")?;

        let nushell_paths = rendered_paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        let nushell_content = render_nushell_paths_file(&nushell_paths);
        write_atomic(&self.paths_nu_file, nushell_content.as_bytes())
            .context("Failed to write Nushell paths file")?;
        Ok(())
    }

    #[cfg(not(unix))]
    pub fn regenerate_paths(
        &self,
        _package_database: &mut PackageDatabase,
        _paths: &crate::utils::static_paths::UpstreamPaths,
    ) -> Result<()> {
        Ok(())
    }

    /// Adds a package's installation path to PATH
    pub fn add_to_paths(
        &self,
        package_database: &mut PackageDatabase,
        package_name: &str,
        install_path: &Path,
    ) -> Result<()> {
        #[cfg(not(unix))]
        let _ = (package_database, package_name);

        if !install_path.is_dir() {
            anyhow::bail!(
                "Package install directory not found: {}",
                install_path.to_string_lossy()
            );
        }

        #[cfg(unix)]
        {
            let _guard = paths_file_lock()
                .lock()
                .ok()
                .ok_or_else(|| anyhow::anyhow!("Failed to lock PATH file for writing"))?;
            package_database.add_path_entry(package_name, install_path)?;
            self.regenerate_paths_files(package_database)?;
        }

        #[cfg(windows)]
        {
            let _ = self.paths_file;
            self.add_to_windows_registry(install_path)?;
        }

        Ok(())
    }

    /// Removes a package's PATH entry
    pub fn remove_from_paths(
        &self,
        package_database: &mut PackageDatabase,
        package_name: &str,
        install_path: &Path,
    ) -> Result<()> {
        #[cfg(not(unix))]
        let _ = (package_database, package_name);
        #[cfg(unix)]
        let _ = install_path;

        #[cfg(unix)]
        {
            let _guard = paths_file_lock()
                .lock()
                .ok()
                .ok_or_else(|| anyhow::anyhow!("Failed to lock PATH file for writing"))?;
            package_database.remove_path_entry(package_name)?;
            self.regenerate_paths_files(package_database)?;
        }

        #[cfg(windows)]
        {
            let _ = self.paths_file;
            self.remove_from_windows_registry(install_path)?;
        }

        Ok(())
    }

    #[cfg(unix)]
    pub fn regenerate_paths_files(&self, package_database: &PackageDatabase) -> Result<()> {
        let paths = package_database.list_path_entries()?;
        let posix_content = render_posix_paths_file(&paths);
        write_atomic(self.paths_file, posix_content.as_bytes())
            .context("Failed to write paths file")?;

        let nushell_paths = paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        let nushell_content = render_nushell_paths_file(&nushell_paths);
        write_atomic(&self.paths_nu_file, nushell_content.as_bytes())
            .context("Failed to write Nushell paths file")?;
        Ok(())
    }

    #[cfg(not(unix))]
    pub fn regenerate_paths_files(&self, _package_database: &PackageDatabase) -> Result<()> {
        Ok(())
    }

    #[cfg(windows)]
    fn add_to_windows_registry(&self, path: &Path) -> Result<()> {
        use winreg::RegKey;
        use winreg::enums::*;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let env_key = hkcu
            .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
            .context("Failed to open registry Environment key")?;

        let target_path = path.to_string_lossy().to_string();
        let target_norm = normalize_windows_path(&target_path);

        // Get current PATH
        let current_path: String = env_key.get_value("Path").unwrap_or_else(|_| String::new());

        // Check if path is already in PATH
        let path_entries: Vec<&str> = current_path.split(';').collect();
        if path_entries
            .iter()
            .any(|&p| normalize_windows_path(p) == target_norm)
        {
            return Ok(()); // Already in PATH
        }

        // Add path to the beginning
        let new_path = if current_path.is_empty() {
            target_path
        } else {
            format!("{};{}", target_path, current_path)
        };

        env_key
            .set_value("Path", &new_path)
            .context("Failed to set PATH in registry")?;

        // Broadcast environment change
        Self::broadcast_environment_change();

        Ok(())
    }

    #[cfg(windows)]
    fn remove_from_windows_registry(&self, path: &Path) -> Result<()> {
        use winreg::RegKey;
        use winreg::enums::*;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let env_key = hkcu
            .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
            .context("Failed to open registry Environment key")?;

        let target_path = path.to_string_lossy().to_string();
        let target_norm = normalize_windows_path(&target_path);

        // Get current PATH
        let current_path: String = env_key.get_value("Path").unwrap_or_else(|_| String::new());

        // Remove target path from PATH
        let path_entries: Vec<&str> = current_path
            .split(';')
            .filter(|&p| normalize_windows_path(p) != target_norm)
            .collect();

        let new_path = path_entries.join(";");

        env_key
            .set_value("Path", &new_path)
            .context("Failed to set PATH in registry")?;

        // Broadcast environment change
        Self::broadcast_environment_change();

        Ok(())
    }

    #[cfg(windows)]
    fn broadcast_environment_change() {
        use std::ptr;
        use winapi::shared::minwindef::LPARAM;
        use winapi::um::winuser::{
            HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
        };

        unsafe {
            let env_string: Vec<u16> = "Environment\0".encode_utf16().collect();
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                env_string.as_ptr() as LPARAM,
                SMTO_ABORTIFHUNG,
                5000,
                ptr::null_mut(),
            );
        }
    }
}

#[cfg(unix)]
fn escape_nushell_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(unix)]
fn escape_posix_path(value: &str) -> String {
    value.replace('$', "\\$").replace('"', "\\\"")
}

#[cfg(unix)]
fn derive_path_entries(
    package_database: &mut PackageDatabase,
    paths: &crate::utils::static_paths::UpstreamPaths,
) -> Result<Vec<(String, PathBuf)>> {
    let mut packages = package_database.list_packages()?;
    packages.sort_by(|left, right| {
        right
            .last_upgraded
            .cmp(&left.last_upgraded)
            .then_with(|| left.name.cmp(&right.name))
    });

    let mut entries = Vec::new();

    for package in &packages {
        if let Some(path_entry) = derive_package_path_entry(paths, package) {
            push_unique_package_path(&mut entries, package.name.clone(), path_entry);
        }
    }

    Ok(entries)
}

#[cfg(unix)]
fn derive_package_path_entry(
    paths: &crate::utils::static_paths::UpstreamPaths,
    package: &crate::models::upstream::Package,
) -> Option<PathBuf> {
    let install_path = package.install_path.as_ref()?;

    if package.filetype != crate::models::common::enums::Filetype::Archive
        || !install_path.starts_with(&paths.install.archives_dir)
    {
        return None;
    }

    if install_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("app"))
        .unwrap_or(false)
    {
        return None;
    }

    package
        .exec_path
        .as_ref()
        .and_then(|exec_path| exec_path.parent().map(Path::to_path_buf))
        .or_else(|| Some(install_path.to_path_buf()))
}

#[cfg(unix)]
fn push_unique_package_path(
    entries: &mut Vec<(String, PathBuf)>,
    package_name: String,
    path: PathBuf,
) {
    if !entries.iter().any(|(_, entry)| entry == &path) {
        entries.push((package_name, path));
    }
}

#[cfg(unix)]
fn render_path_entries(symlinks_dir: &Path, package_entries: &[(String, PathBuf)]) -> Vec<PathBuf> {
    let mut entries = vec![symlinks_dir.to_path_buf()];
    for (_, path) in package_entries {
        if !entries.iter().any(|entry| entry == path) {
            entries.push(path.clone());
        }
    }
    entries
}

#[cfg(unix)]
pub fn render_posix_paths_file(paths: &[PathBuf]) -> String {
    let mut content = String::from("#!/bin/bash\n# Upstream managed PATH additions\n");
    for path in paths.iter().rev() {
        let escaped = escape_posix_path(&path.to_string_lossy());
        content.push_str(&format!("export PATH=\"{escaped}:$PATH\"\n"));
    }
    content
}

#[cfg(unix)]
pub fn render_nushell_paths_file(paths: &[String]) -> String {
    let mut content = String::from("# Upstream managed PATH additions\n\nlet upstream_paths = [\n");
    for path in paths {
        content.push_str(&format!("    \"{}\"\n", escape_nushell_string(path)));
    }
    content.push_str("]\n\n$env.PATH = ($upstream_paths ++ $env.PATH)\n");
    content
}

#[cfg(unix)]
pub fn nushell_paths_file_contains_path(content: &str, path: &str) -> bool {
    parse_nushell_paths_file(content)
        .iter()
        .any(|entry| entry == path)
}

#[cfg(unix)]
fn parse_nushell_paths_file(content: &str) -> Vec<String> {
    let mut list_entries = Vec::new();
    let mut prepend_entries = Vec::new();
    let mut in_list = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "let upstream_paths = [" {
            in_list = true;
            continue;
        }
        if in_list && trimmed == "]" {
            in_list = false;
            continue;
        }

        if in_list {
            if let Some((path, _)) = parse_nushell_string_literal(trimmed) {
                list_entries.push(path);
            }
            continue;
        }

        if let Some(prepend_index) = trimmed.find("| prepend ") {
            let rest = trimmed[prepend_index + "| prepend ".len()..].trim_start();
            if let Some((path, _)) = parse_nushell_string_literal(rest) {
                prepend_entries.push(path);
            }
        }
    }

    if list_entries.is_empty() {
        prepend_entries.reverse();
        dedupe_preserving_order(prepend_entries)
    } else {
        prepend_entries.reverse();
        list_entries.extend(prepend_entries);
        dedupe_preserving_order(list_entries)
    }
}

#[cfg(unix)]
fn parse_nushell_string_literal(input: &str) -> Option<(String, usize)> {
    let mut chars = input.char_indices();
    let (_, first) = chars.next()?;
    if first != '"' {
        return None;
    }

    let mut output = String::new();
    let mut escaped = false;
    for (index, ch) in chars {
        if escaped {
            match ch {
                '"' | '\\' => output.push(ch),
                other => {
                    output.push('\\');
                    output.push(other);
                }
            }
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => return Some((output, index + ch.len_utf8())),
            other => output.push(other),
        }
    }

    None
}

#[cfg(unix)]
fn dedupe_preserving_order(paths: Vec<String>) -> Vec<String> {
    let mut unique = Vec::new();
    for path in paths {
        if !unique.contains(&path) {
            unique.push(path);
        }
    }
    unique
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::{ShellManager, parse_nushell_paths_file};
    #[cfg(unix)]
    use crate::models::common::enums::{Channel, Filetype, Provider};
    #[cfg(unix)]
    use crate::models::upstream::Package;
    #[cfg(unix)]
    use crate::storage::database::PackageDatabase;
    #[cfg(unix)]
    use crate::utils::test_support;
    #[cfg(unix)]
    use std::path::{Path, PathBuf};
    #[cfg(unix)]
    use std::time::{SystemTime, UNIX_EPOCH};
    #[cfg(unix)]
    use std::{fs, io};

    #[cfg(unix)]
    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-shell-test-{name}-{nanos}"))
    }

    #[cfg(unix)]
    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    #[cfg(unix)]
    fn seed_package(package_database: &mut PackageDatabase, name: &str) {
        let package = Package::with_defaults(
            name.to_string(),
            format!("owner/{name}"),
            Filetype::Archive,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        package_database
            .upsert_package(&package)
            .expect("seed package");
    }

    #[cfg(unix)]
    #[test]
    fn add_to_paths_is_idempotent_and_escapes_special_characters() {
        let root = temp_root("add-idempotent");
        let install_path = root.join("tool\"dir$");
        let paths_file = root.join("paths.sh");
        let paths_nu_file = root.join("paths.nu");
        fs::create_dir_all(&install_path).expect("create install dir");
        fs::write(&paths_file, "#!/usr/bin/env sh\n").expect("create paths file");
        fs::write(&paths_nu_file, "# Upstream managed PATH additions\n").expect("create paths.nu");
        let manager = ShellManager::new(&paths_file);
        let mut package_database =
            PackageDatabase::open(&root.join("packages.db")).expect("open package db");
        seed_package(&mut package_database, "tool");

        manager
            .add_to_paths(&mut package_database, "tool", &install_path)
            .expect("first add");
        manager
            .add_to_paths(&mut package_database, "tool", &install_path)
            .expect("second add");

        let content = fs::read_to_string(&paths_file).expect("read paths file");
        assert_eq!(content.matches("export PATH=").count(), 1);
        assert!(content.contains("\\\""));
        assert!(content.contains("\\$"));

        let nushell_content = fs::read_to_string(&paths_nu_file).expect("read paths.nu");
        assert!(nushell_content.contains("let upstream_paths = ["));
        assert!(nushell_content.contains("$env.PATH = ($upstream_paths ++ $env.PATH)"));
        assert_eq!(
            parse_nushell_paths_file(&nushell_content),
            vec![install_path.to_string_lossy().to_string()]
        );
        assert!(nushell_content.contains("\\\""));
        assert!(nushell_content.contains("$"));

        cleanup(&root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn remove_from_paths_removes_existing_export_line() {
        let root = temp_root("remove");
        let install_path = root.join("pkg/bin");
        let paths_file = root.join("paths.sh");
        let paths_nu_file = root.join("paths.nu");
        fs::create_dir_all(&install_path).expect("create install dir");
        fs::write(&paths_file, "").expect("create paths file");
        fs::write(&paths_nu_file, "# Upstream managed PATH additions\n").expect("create paths.nu");
        let manager = ShellManager::new(&paths_file);
        let mut package_database =
            PackageDatabase::open(&root.join("packages.db")).expect("open package db");
        seed_package(&mut package_database, "tool");

        manager
            .add_to_paths(&mut package_database, "tool", &install_path)
            .expect("add path");
        manager
            .remove_from_paths(&mut package_database, "tool", &install_path)
            .expect("remove path");

        let content = fs::read_to_string(&paths_file).expect("read paths file");
        assert!(!content.contains("export PATH="));

        let nushell_content = fs::read_to_string(&paths_nu_file).expect("read paths.nu");
        assert!(parse_nushell_paths_file(&nushell_content).is_empty());

        cleanup(&root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn add_to_paths_renders_database_order() {
        let root = temp_root("render-order");
        let first_path = root.join("first/bin");
        let second_path = root.join("second/bin");
        let new_path = root.join("new/bin");
        let paths_file = root.join("paths.sh");
        let paths_nu_file = root.join("paths.nu");
        fs::create_dir_all(&first_path).expect("create first dir");
        fs::create_dir_all(&second_path).expect("create second dir");
        fs::create_dir_all(&new_path).expect("create new dir");
        fs::write(&paths_file, "#!/usr/bin/env sh\n").expect("create paths file");
        fs::write(&paths_nu_file, "# Upstream managed PATH additions\n").expect("create paths.nu");
        let manager = ShellManager::new(&paths_file);
        let mut package_database =
            PackageDatabase::open(&root.join("packages.db")).expect("open package db");
        seed_package(&mut package_database, "first");
        seed_package(&mut package_database, "second");
        seed_package(&mut package_database, "new");

        manager
            .add_to_paths(&mut package_database, "first", &first_path)
            .expect("add first path");
        manager
            .add_to_paths(&mut package_database, "second", &second_path)
            .expect("add second path");
        manager
            .add_to_paths(&mut package_database, "new", &new_path)
            .expect("add new path");

        let nushell_content = fs::read_to_string(&paths_nu_file).expect("read paths.nu");
        assert!(!nushell_content.contains(" | prepend "));
        assert_eq!(
            parse_nushell_paths_file(&nushell_content),
            vec![
                new_path.to_string_lossy().to_string(),
                second_path.to_string_lossy().to_string(),
                first_path.to_string_lossy().to_string(),
            ]
        );

        cleanup(&root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn regenerate_paths_renders_from_packages_database() {
        let root = temp_root("regen-db");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.state.symlinks_dir).expect("create symlinks dir");
        fs::create_dir_all(&paths.install.archives_dir).expect("create archives dir");
        fs::create_dir_all(paths.config.paths_file.parent().expect("paths parent"))
            .expect("create paths parent");
        fs::write(&paths.config.paths_file, "").expect("create paths file");
        fs::write(&paths.config.paths_nu_file, "").expect("create paths nu");

        let mut package_database =
            PackageDatabase::open(&paths.config.packages_database_file).expect("open db");

        let older_install = paths.install.archives_dir.join("older/bin");
        let newer_install = paths.install.archives_dir.join("newer/bin");
        fs::create_dir_all(&older_install).expect("create older");
        fs::create_dir_all(&newer_install).expect("create newer");

        let mut older = Package::with_defaults(
            "older".to_string(),
            "owner/older".to_string(),
            Filetype::Archive,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        older.install_path = Some(paths.install.archives_dir.join("older"));
        older.exec_path = Some(older_install.clone());
        older.last_upgraded = chrono::Utc::now() - chrono::Duration::days(1);

        let mut newer = Package::with_defaults(
            "newer".to_string(),
            "owner/newer".to_string(),
            Filetype::Archive,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        newer.install_path = Some(paths.install.archives_dir.join("newer"));
        newer.exec_path = Some(newer_install.clone());
        newer.last_upgraded = chrono::Utc::now();

        package_database.upsert_package(&older).expect("seed older");
        package_database.upsert_package(&newer).expect("seed newer");

        let manager = ShellManager::new(&paths.config.paths_file);
        manager
            .regenerate_paths(&mut package_database, &paths)
            .expect("regenerate paths");

        let nushell_content =
            fs::read_to_string(&paths.config.paths_nu_file).expect("read paths.nu");
        assert_eq!(
            parse_nushell_paths_file(&nushell_content),
            vec![
                paths.state.symlinks_dir.display().to_string(),
                paths
                    .install
                    .archives_dir
                    .join("newer")
                    .to_string_lossy()
                    .to_string(),
                paths
                    .install
                    .archives_dir
                    .join("older")
                    .to_string_lossy()
                    .to_string(),
            ]
        );

        cleanup(&root).expect("cleanup");
    }
}
