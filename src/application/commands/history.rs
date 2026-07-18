use anyhow::Result;

use crate::{output, utils::static_paths::UpstreamPaths};

pub fn run(
    package: Option<String>,
    action: Option<String>,
    status: Option<String>,
    limit: usize,
    json: bool,
) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let mut events = output::read_log_events(&paths.dirs.data_dir.join("log.jsonl"))?;

    events.retain(|event| {
        package.as_ref().is_none_or(|package| {
            event
                .subject
                .as_deref()
                .is_some_and(|subject| subject.eq_ignore_ascii_case(package))
        }) && action.as_ref().is_none_or(|action| {
            event
                .command
                .as_deref()
                .is_some_and(|command| command.starts_with(action))
        }) && status.as_ref().is_none_or(|status| {
            event
                .status
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(status))
                || event.success.is_some_and(|success| {
                    (success && status.eq_ignore_ascii_case("success"))
                        || (!success && status.eq_ignore_ascii_case("failed"))
                })
        })
    });

    let first = events.len().saturating_sub(limit);
    let events = &events[first..];
    if json {
        println!("{}", serde_json::to_string_pretty(events)?);
        return Ok(());
    }

    if events.is_empty() {
        println!("{}", output::warning("No matching history records."));
        return Ok(());
    }

    let mut text = String::new();
    for event in events.iter().rev() {
        let command = event.command.as_deref().unwrap_or("-");
        let subject = event.subject.as_deref().unwrap_or("-");
        let state = event
            .status
            .as_deref()
            .or_else(|| {
                event
                    .success
                    .map(|ok| if ok { "success" } else { "failed" })
            })
            .unwrap_or(event.level.as_str());
        let message = event.message.as_deref().unwrap_or("");
        text.push_str(&format!(
            "{}  {:<16} {:<20} {:<8} {}\n",
            event.timestamp, command, subject, state, message
        ));
    }

    output::pager::page_text(Some("Upstream history"), text.trim_end())?;
    Ok(())
}
