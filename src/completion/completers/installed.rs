use anyhow::Result;

use crate::{
    completion::{CompletionCandidate, CompletionRequest},
    providers::discovery::friendly_name,
};

use super::super::startup::CompletionStartup;

pub fn package_names(
    request: &CompletionRequest,
    startup: &CompletionStartup,
) -> Result<Vec<CompletionCandidate>> {
    let prefix = request.current_word().to_ascii_lowercase();
    let Some(database) = startup.package_database()? else {
        return Ok(Vec::new());
    };

    let mut values = database
        .list_packages()?
        .into_iter()
        .flat_map(|package| {
            let friendly = friendly_name(
                &package.provider,
                &package.repo_slug,
                package.base_url.as_deref(),
            );

            std::iter::once(package.id).chain(friendly)
        })
        .filter(|value| value.to_ascii_lowercase().starts_with(&prefix))
        .collect::<Vec<_>>();

    values.sort_unstable();
    values.dedup();
    Ok(values.into_iter().map(CompletionCandidate::value).collect())
}

pub fn executable_aliases(
    request: &CompletionRequest,
    startup: &CompletionStartup,
) -> Result<Vec<CompletionCandidate>> {
    let prefix = request.current_word().to_ascii_lowercase();
    let Some(database) = startup.package_database()? else {
        return Ok(Vec::new());
    };

    let mut values = database
        .list_packages()?
        .into_iter()
        .flat_map(|package| {
            package
                .executables
                .into_iter()
                .map(|executable| executable.name)
        })
        .filter(|value| value.to_ascii_lowercase().starts_with(&prefix))
        .collect::<Vec<_>>();

    values.sort_unstable();
    values.dedup();
    Ok(values.into_iter().map(CompletionCandidate::value).collect())
}
