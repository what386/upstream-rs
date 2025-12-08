use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use crate::models::upstream::Repository;

pub struct RepositoryStorage {
    repositories_file_path: String,
    repositories: Vec<Repository>,
}

impl RepositoryStorage {
    pub fn new(repositories_file_path: String) -> io::Result<Self> {
        let mut storage = Self {
            repositories_file_path,
            repositories: Vec::new(),
        };
        storage.load_repositories()?;
        Ok(storage)
    }

    /// Load all repositories from the repositories.json file.
    pub fn load_repositories(&mut self) -> io::Result<()> {
        if !Path::new(&self.repositories_file_path).exists() {
            self.repositories = Vec::new();
            return Ok(());
        }

        let json = fs::read_to_string(&self.repositories_file_path)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to load repositories: {}", e)))?;

        self.repositories = serde_json::from_str(&json)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to parse repositories: {}", e)))?;

        Ok(())
    }

    /// Save all repositories to the repositories.json file.
    pub fn save_repositories(&self) -> io::Result<()> {
        let json = serde_json::to_string_pretty(&self.repositories)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to serialize: {}", e)))?;

        fs::write(&self.repositories_file_path, json)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to save repositories: {}", e)))
    }

    pub fn deduplicate_repositories(&mut self) -> io::Result<()> {
        let mut seen = HashMap::new();

        // Keep the most recently updated repository for each slug
        for repository in &self.repositories {
            let entry = seen.entry(repository.slug.clone())
                .or_insert_with(|| repository.clone());

            if repository.last_updated > entry.last_updated {
                *entry = repository.clone();
            }
        }

        self.repositories = seen.into_values().collect();
        self.save_repositories()
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
