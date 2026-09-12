use anyhow::Result;

use crate::{storage::database::PackageDatabase, utils::static_paths::UpstreamPaths};

/// Minimal application state for completion requests.
///
/// This intentionally does not load config, run migrations, acquire locks,
/// initialize logging, or record history. Completion must be cheap and
/// side-effect free apart from reading already created metadata
pub struct CompletionStartup {
    paths: Option<UpstreamPaths>,
    database: Option<PackageDatabase>,
}

impl CompletionStartup {
    pub fn new() -> Result<Self> {
        Ok(Self {
            paths: Some(UpstreamPaths::new()?),
            database: None,
        })
    }

    pub fn from_database(database: PackageDatabase) -> Self {
        Self {
            paths: None,
            database: Some(database),
        }
    }

    pub fn package_database(&self) -> Result<Option<PackageDatabase>> {
        if let Some(database) = &self.database {
            return Ok(Some(database.clone()));
        }

        let Some(paths) = &self.paths else {
            return Ok(None);
        };

        if !paths.metadata.packages_database_file.is_file() {
            return Ok(None);
        }

        PackageDatabase::open(&paths.metadata.packages_database_file).map(Some)
    }
}
