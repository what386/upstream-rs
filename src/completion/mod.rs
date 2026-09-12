pub mod completers;
pub mod startup;

use anyhow::Result;

use self::startup::CompletionStartup;

/// The shell-independent input supplied to a command completer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionRequest {
    /// Command-line words after the executable name.
    pub words: Vec<String>,
    /// Index of the word currently being completed.
    pub cursor: usize,
}

impl CompletionRequest {
    pub fn new(words: Vec<String>, cursor: usize) -> Result<Self> {
        if cursor > words.len() {
            anyhow::bail!(
                "completion cursor {} is outside the {} supplied words",
                cursor,
                words.len()
            );
        }

        Ok(Self { words, cursor })
    }

    pub fn current_word(&self) -> &str {
        self.words
            .get(self.cursor)
            .map(String::as_str)
            .unwrap_or("")
    }
}

/// A value returned to the shell. Descriptions are optional so the same
/// completer can serve shells with different candidate-display capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionCandidate {
    pub value: String,
    pub description: Option<String>,
}

impl CompletionCandidate {
    pub fn value(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            description: None,
        }
    }
}

/// Dispatch point for command-specific completers.
pub fn complete(
    command: &str,
    request: &CompletionRequest,
    startup: &CompletionStartup,
) -> Result<Vec<CompletionCandidate>> {
    match command {
        "changelog" => completers::changelog::complete(request, startup),
        "docs" => completers::docs::complete(request, startup),
        "doctor" => completers::doctor::complete(request, startup),
        "history" => completers::history::complete(request, startup),
        "info" => completers::info::complete(request, startup),
        "list" => completers::list::complete(request, startup),
        "package" => completers::package::complete(request, startup),
        "reinstall" => completers::reinstall::complete(request, startup),
        "remove" => completers::remove::complete(request, startup),
        "rollback" => completers::rollback::complete(request, startup),
        "upgrade" => completers::upgrade::complete(request, startup),
        _ => Ok(Vec::new()),
    }
}

/// Run the native completion protocol used by generated shell hooks.
///
/// The wire format is `__complete COMMAND CURSOR -- WORD...`, where `CURSOR`
/// is relative to the supplied words and points at the word being completed.
pub fn run(args: &[String]) -> Result<()> {
    let [command, cursor, rest @ ..] = args else {
        anyhow::bail!("usage: upstream __complete COMMAND CURSOR -- WORD...");
    };

    let cursor = cursor
        .parse::<usize>()
        .map_err(|_| anyhow::anyhow!("invalid completion cursor '{cursor}'"))?;
    let words = rest
        .strip_prefix(&["--".to_string()])
        .unwrap_or(rest)
        .to_vec();
    let request = CompletionRequest::new(words, cursor)?;
    let startup = CompletionStartup::new()?;

    for candidate in complete(command, &request, &startup)? {
        println!("{}", candidate.value);
    }

    Ok(())
}
