use anyhow::Result;

use crate::{
    application::operations::history_op::{self, HistoryFilter},
    output,
    utils::static_paths::UpstreamPaths,
};

pub fn run(
    package: Option<String>,
    action: Option<String>,
    status: Option<String>,
    limit: usize,
    since: Option<String>,
    today: bool,
    json: bool,
) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let since = since
        .as_deref()
        .map(history_op::parse_since)
        .transpose()
        .map_err(anyhow::Error::msg)?;
    let filter = HistoryFilter {
        package,
        action,
        status,
        since,
        today,
        limit,
    };
    let events = history_op::filter_records(
        output::read_log_events(&paths.dirs.data_dir.join("log.jsonl"))?,
        &filter,
    );
    if json {
        println!("{}", serde_json::to_string_pretty(&events)?);
        return Ok(());
    }

    if events.is_empty() {
        println!("{}", output::warning("No matching history records."));
        return Ok(());
    }

    let text = history_op::render_records(&events);
    output::pager::page_text(Some("Upstream history"), text.trim_end())?;
    Ok(())
}
