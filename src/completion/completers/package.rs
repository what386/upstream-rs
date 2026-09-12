use anyhow::Result;

use super::super::installed;
use super::super::{CompletionCandidate, CompletionRequest, startup::CompletionStartup};

pub fn complete(
    request: &CompletionRequest,
    startup: &CompletionStartup,
) -> Result<Vec<CompletionCandidate>> {
    if request.cursor != 1 {
        return Ok(Vec::new());
    }

    if request.words.first().is_some_and(|word| word == "rename") {
        return installed::executable_aliases(request, startup);
    }

    installed::package_names(request, startup)
}
