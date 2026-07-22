use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

use crate::models::common::enums::TrustMode;
use crate::models::upstream::Package;

use super::packages::PackageConnection;
use super::settings::PackageSettings;

#[derive(Debug, Clone)]
pub struct PackageDatabase {
    database_file: PathBuf,
}

impl PackageDatabase {
    pub fn open(package_database_path: &Path) -> Result<Self> {
        let database_file = Self::database_path_for(package_database_path);
        PackageConnection::open(&database_file)?;
        Ok(Self { database_file })
    }

    pub fn database_path_for(package_database_path: &Path) -> PathBuf {
        match package_database_path
            .extension()
            .and_then(|extension| extension.to_str())
        {
            Some("db") => package_database_path.to_path_buf(),
            _ => package_database_path.with_extension("db"),
        }
    }

    pub fn schema_version(&self) -> Result<u32> {
        self.connection()?.schema_version()
    }

    pub fn package_exists(&self, name: &str) -> Result<bool> {
        self.connection()?.package_exists(name)
    }

    pub fn get_package(&self, name: &str) -> Result<Option<Package>> {
        self.connection()?.get_package(name)
    }

    pub fn list_packages(&self) -> Result<Vec<Package>> {
        self.connection()?.list_packages()
    }

    pub fn list_path_entries(&self) -> Result<Vec<PathBuf>> {
        self.connection()?.list_path_entries()
    }

    pub fn get_package_settings(&self, package_name: &str) -> Result<Option<PackageSettings>> {
        self.connection()?.get_package_settings(package_name)
    }

    pub fn effective_trust_mode(
        &self,
        package_name: &str,
        operation_override: Option<TrustMode>,
    ) -> Result<TrustMode> {
        if let Some(mode) = operation_override {
            return Ok(mode);
        }
        Ok(self
            .get_package_settings(package_name)?
            .and_then(|settings| settings.trust_mode)
            .unwrap_or(TrustMode::BestEffort))
    }

    pub fn upsert_package_settings(&mut self, settings: &PackageSettings) -> Result<()> {
        self.connection()?.upsert_package_settings(settings)
    }

    pub fn upsert_package_with_settings(
        &mut self,
        package: &Package,
        settings: &PackageSettings,
    ) -> Result<()> {
        self.connection()?
            .upsert_package_with_settings(package, settings)
    }

    pub fn upsert_package(&mut self, package: &Package) -> Result<()> {
        self.connection()?.upsert_package(package)
    }

    pub fn add_path_entry(&mut self, package_name: &str, path: &Path) -> Result<bool> {
        self.connection()?.add_path_entry(package_name, path)
    }

    pub fn remove_path_entry(&mut self, package_name: &str) -> Result<bool> {
        self.connection()?.remove_path_entry(package_name)
    }

    pub fn replace_all_path_entries(&mut self, entries: &[(String, PathBuf)]) -> Result<()> {
        self.connection()?.replace_all_path_entries(entries)
    }

    pub fn replace_all_packages(&mut self, packages: &[Package]) -> Result<()> {
        self.connection()?.replace_all_packages(packages)
    }

    pub fn remove_package(&mut self, name: &str) -> Result<bool> {
        self.connection()?.remove_package(name)
    }

    pub fn update_package<F>(&mut self, name: &str, update: F) -> Result<bool>
    where
        F: FnOnce(&mut Package) -> Result<bool>,
    {
        let mut package = self
            .get_package(name)?
            .ok_or_else(|| anyhow!("Package '{}' not found", name))?;
        let changed = update(&mut package)?;
        if !changed {
            return Ok(false);
        }
        if package.name != name {
            return Err(anyhow!(
                "Package update changed '{}' to '{}'; use rename_package for package renames",
                name,
                package.name
            ));
        }

        self.upsert_package(&package)?;
        Ok(true)
    }

    pub fn rename_package(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        if self.package_exists(new_name)? {
            return Err(anyhow!("Package '{}' already exists", new_name));
        }

        self.connection()?.update_package(old_name, |package| {
            package.name = new_name.to_string();
            Ok(())
        })
    }

    fn connection(&self) -> Result<PackageConnection> {
        PackageConnection::open(&self.database_file)
    }
}

#[cfg(test)]
mod tests {
    use super::PackageDatabase;
    use crate::models::common::Version;
    use crate::models::common::enums::{Channel, Filetype, Provider, TrustMode};
    use crate::models::upstream::Package;
    use crate::storage::database::PackageSettings;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_database_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("upstream-packages-test-{name}-{nanos}"))
            .join("packages.db")
    }

    fn test_package(name: &str) -> Package {
        Package::with_defaults(
            name.to_string(),
            format!("owner/{name}"),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        )
    }

    fn legacy_packages_file(database_path: &Path) -> PathBuf {
        database_path.with_extension("json")
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::remove_dir_all(parent)?;
        }
        Ok(())
    }

    #[test]
    fn open_starts_empty_when_file_missing() {
        let path = temp_database_path("missing");
        let db = PackageDatabase::open(&path).expect("open database");
        assert!(db.list_packages().expect("list packages").is_empty());
        assert!(path.exists());
        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn open_ignores_adjacent_legacy_json() {
        let path = temp_database_path("legacy-json-ignored");
        let legacy_path = legacy_packages_file(&path);
        if let Some(parent) = legacy_path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(&legacy_path, "{not-json").expect("write invalid legacy json");

        let db = PackageDatabase::open(&path).expect("open database");

        assert!(path.exists());
        assert!(db.list_packages().expect("list packages").is_empty());

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn upsert_replaces_existing_package_name() {
        let path = temp_database_path("update");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        let mut db = PackageDatabase::open(&path).expect("open database");

        let mut first = test_package("tool");
        first.version = Version::new(1, 0, 0, false);
        db.upsert_package(&first).expect("store first");

        let mut second = first.clone();
        second.version = Version::new(2, 0, 0, false);
        second.repo_slug = "owner/renamed-repo".to_string();
        db.upsert_package(&second).expect("store update");

        let package = db
            .get_package("tool")
            .expect("load package")
            .expect("stored package");
        assert_eq!(package.version, Version::new(2, 0, 0, false));
        assert_eq!(package.repo_slug, "owner/renamed-repo");

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn update_package_upserts_changed_package() {
        let path = temp_database_path("update-package");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        let mut db = PackageDatabase::open(&path).expect("open database");
        db.upsert_package(&test_package("tool"))
            .expect("store package");

        let changed = db
            .update_package("tool", |package| {
                package.version = Version::new(3, 0, 0, false);
                Ok(true)
            })
            .expect("update package");

        assert!(changed);
        let reloaded = PackageDatabase::open(&path).expect("reload database");
        assert_eq!(
            reloaded
                .get_package("tool")
                .expect("load package")
                .expect("updated package")
                .version,
            Version::new(3, 0, 0, false)
        );

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn update_package_skips_unchanged_package() {
        let path = temp_database_path("unchanged-package");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        let mut db = PackageDatabase::open(&path).expect("open database");
        db.upsert_package(&test_package("tool"))
            .expect("store package");

        let changed = db
            .update_package("tool", |_package| Ok(false))
            .expect("update package");

        assert!(!changed);
        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn rename_package_updates_database_primary_key() {
        let path = temp_database_path("rename-package");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        let mut db = PackageDatabase::open(&path).expect("open database");
        db.upsert_package(&test_package("old"))
            .expect("store package");

        db.rename_package("old", "new").expect("rename package");

        let reloaded = PackageDatabase::open(&path).expect("reload database");
        assert!(reloaded.get_package("old").expect("load old").is_none());
        assert!(reloaded.get_package("new").expect("load new").is_some());

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn remove_package_returns_expected_status() {
        let path = temp_database_path("remove");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        let mut db = PackageDatabase::open(&path).expect("open database");
        db.upsert_package(&test_package("one"))
            .expect("store package");

        assert!(db.remove_package("one").expect("remove"));
        assert!(!db.remove_package("one").expect("second remove"));

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn upsert_creates_missing_parent_dirs() {
        let path = temp_database_path("missing-parent");
        let mut db = PackageDatabase::open(&path).expect("open database");
        db.upsert_package(&test_package("tool"))
            .expect("save package");

        assert!(path.exists());
        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn upsert_writes_database_and_can_reload() {
        let path = temp_database_path("reload");
        let mut db = PackageDatabase::open(&path).expect("open database");
        db.upsert_package(&test_package("tool"))
            .expect("save package");

        let reloaded = PackageDatabase::open(&path).expect("reload database");
        assert_eq!(reloaded.list_packages().expect("list packages").len(), 1);
        assert!(
            reloaded
                .get_package("tool")
                .expect("load package")
                .is_some()
        );

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn upsert_overwrites_visible_result() {
        let path = temp_database_path("overwrite");
        let mut db = PackageDatabase::open(&path).expect("open database");

        let mut first = test_package("tool");
        first.version = Version::new(1, 0, 0, false);
        db.upsert_package(&first).expect("save first");

        let mut second = test_package("tool");
        second.version = Version::new(2, 0, 0, false);
        db.upsert_package(&second).expect("save second");

        let reloaded = PackageDatabase::open(&path).expect("reload database");
        let packages = reloaded.list_packages().expect("list packages");
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].version, Version::new(2, 0, 0, false));

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn effective_trust_mode_uses_override_then_stored_then_default() {
        let path = temp_database_path("trust-precedence");
        let mut db = PackageDatabase::open(&path).expect("open database");
        db.upsert_package(&test_package("tool"))
            .expect("store package");

        assert_eq!(
            db.effective_trust_mode("tool", None).expect("default"),
            TrustMode::BestEffort
        );

        let mut settings = PackageSettings::new("tool");
        settings.trust_mode = Some(TrustMode::Checksum);
        db.upsert_package_settings(&settings)
            .expect("store settings");
        assert_eq!(
            db.effective_trust_mode("tool", None).expect("stored"),
            TrustMode::Checksum
        );
        assert_eq!(
            db.effective_trust_mode("tool", Some(TrustMode::Signature))
                .expect("override"),
            TrustMode::Signature
        );

        cleanup(&path).expect("cleanup");
    }
}
