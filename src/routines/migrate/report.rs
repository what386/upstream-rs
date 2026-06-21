#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub created_dirs: usize,
    pub moved_entries: usize,
    pub updated_packages: usize,
    pub updated_rollback_records: usize,
    pub migrated_trusted_keys: usize,
    pub deduped_trusted_keys: usize,
    pub refreshed_symlinks: usize,
    pub skipped_symlinks: usize,
}
