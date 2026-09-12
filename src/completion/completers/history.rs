use anyhow::Result;

use super::super::installed;
use super::super::{CompletionCandidate, CompletionRequest, startup::CompletionStartup};

pub fn complete(
    request: &CompletionRequest,
    startup: &CompletionStartup,
) -> Result<Vec<CompletionCandidate>> {
    installed::package_names(request, startup)
}
