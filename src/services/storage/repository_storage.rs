use std::fs;
use std::io;

use crate::models::upstream::Repository;
use crate::utils::upstream_paths::PATHS;

pub struct RepositoryStorage {
    repositories: Vec<Repository>,
}

impl RepositoryStorage {
    /// Creates a new RepositoryStorage and loads existing repositories.
    pub fn new() -> io::Result<Self> {
        let mut storage = Self {
            repositories: Vec::new(),
        };
        storage.load_repositories()?;
        Ok(storage)
    }

    /// Load all repositories from the repositories.json file.
    fn load_repositories(&mut self) -> io::Result<()> {
        let path = &PATHS.repositories_file;

        if !path.exists() {
            self.repositories = Vec::new();
            return Ok(());
        }

        let json = fs::read_to_string(path)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        self.repositories = serde_json::from_str(&json)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        Ok(())
    }

    /// Save all repositories to the repositories.json file.
    fn save_repositories(&self) -> io::Result<()> {
        let json = serde_json::to_string_pretty(&self.repositories)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        fs::write(&PATHS.repositories_file, json)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }

    /// Get all stored repositories.
    pub fn get_all_repositories(&self) -> &[Repository] {
        &self.repositories
    }

    /// Get a repository by repository slug.
    pub fn get_repository_by_slug(&self, slug: &str) -> Option<&Repository> {
        self.repositories.iter().find(|r| r.slug == slug)
    }

    /// Add or update a repository in the storage.
    pub fn add_or_update_repository(&mut self, repository: Repository) -> io::Result<()> {
        // Remove any existing repository with the same slug
        self.repositories.retain(|r| r.slug != repository.slug);
        self.repositories.push(repository);
        self.save_repositories()
    }

    /// Remove a repository from the storage.
    pub fn remove_repository(&mut self, repository: &Repository) -> io::Result<bool> {
        self.remove_repository_by_slug(&repository.slug)
    }

    /// Remove a repository from the storage by slug.
    pub fn remove_repository_by_slug(&mut self, slug: &str) -> io::Result<bool> {
        let initial_len = self.repositories.len();
        self.repositories.retain(|r| r.slug != slug);

        if self.repositories.len() < initial_len {
            self.save_repositories()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

// Implement Default for convenience
impl Default for RepositoryStorage {
    fn default() -> Self {
        Self::new().expect("Failed to initialize RepositoryStorage")
    }
}
