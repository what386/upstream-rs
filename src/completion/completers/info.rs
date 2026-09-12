use anyhow::Result;

use super::super::{CompletionCandidate, CompletionRequest};
use crate::completion::startup::CompletionStartup;

use super::installed;

/// Completes the installed package query accepted by `upstream info`.
pub fn complete(
    request: &CompletionRequest,
    startup: &CompletionStartup,
) -> Result<Vec<CompletionCandidate>> {
    installed::package_names(request, startup)
}

#[cfg(test)]
mod tests {
    use super::complete;
    use crate::completion::{CompletionRequest, startup::CompletionStartup};
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::storage::database::PackageDatabase;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn database() -> PackageDatabase {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("upstream-completion-info-{unique}/packages.db"));
        let mut database = PackageDatabase::open(&path).expect("open package database");
        database
            .upsert_package(&Package::with_defaults(
                "github:BurntSushi/ripgrep".into(),
                "BurntSushi/ripgrep".into(),
                Filetype::Binary,
                None,
                None,
                Channel::Stable,
                Provider::Github,
                None,
            ))
            .expect("insert package");
        database
    }

    #[test]
    fn completes_friendly_name_by_prefix() {
        let database = database();
        let startup = CompletionStartup::from_database(database);
        let request = CompletionRequest::new(vec!["ripg".into()], 0).expect("request");

        let candidates = complete(&request, &startup).expect("complete");

        assert_eq!(
            candidates
                .iter()
                .map(|c| c.value.as_str())
                .collect::<Vec<_>>(),
            ["ripgrep"]
        );
    }

    #[test]
    fn returns_canonical_id_too() {
        let database = database();
        let startup = CompletionStartup::from_database(database);
        let request = CompletionRequest::new(vec!["github:".into()], 0).expect("request");

        let candidates = complete(&request, &startup).expect("complete");

        assert_eq!(candidates[0].value, "github:BurntSushi/ripgrep");
    }
}
