use anyhow::Result;

use super::super::{CompletionCandidate, CompletionRequest, startup::CompletionStartup};
use super::installed;

pub fn complete(
    request: &CompletionRequest,
    startup: &CompletionStartup,
) -> Result<Vec<CompletionCandidate>> {
    installed::package_names(request, startup)
}
