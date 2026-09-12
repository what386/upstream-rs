use anyhow::Result;
use std::collections::HashMap;

use crate::{
    completion::{CompletionCandidate, CompletionRequest},
    providers::discovery::friendly_name,
};

use super::startup::CompletionStartup;

pub fn package_names(
    request: &CompletionRequest,
    startup: &CompletionStartup,
) -> Result<Vec<CompletionCandidate>> {
    let prefix = request.current_word().to_ascii_lowercase();
    let Some(database) = startup.package_database()? else {
        return Ok(Vec::new());
    };

    let packages = database.list_packages()?;
    let package_names = packages
        .iter()
        .map(|package| {
            (
                package.id.as_str(),
                friendly_name(
                    &package.provider,
                    &package.repo_slug,
                    package.base_url.as_deref(),
                ),
            )
        })
        .collect::<Vec<_>>();
    let mut friendly_name_counts = HashMap::new();
    for (_, friendly) in &package_names {
        if let Some(friendly) = friendly {
            *friendly_name_counts
                .entry(friendly.to_ascii_lowercase())
                .or_insert(0_usize) += 1;
        }
    }

    let mut values = package_names
        .into_iter()
        .flat_map(|(id, friendly)| {
            let Some(friendly) = friendly else {
                return vec![id.to_string()];
            };

            let is_ambiguous = friendly_name_counts
                .get(&friendly.to_ascii_lowercase())
                .is_some_and(|count| *count > 1);
            if is_ambiguous {
                vec![friendly, id.to_string()]
            } else {
                vec![friendly]
            }
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
