use std::io::{self, Write};

use anyhow::Result;
use chrono::Utc;

use crate::{
    storage::updates::{self, UpdateSnapshot},
    utils::{human_units, static_paths::UpstreamPaths},
};

pub fn run(json: bool, notify: bool, paths: &UpstreamPaths) -> Result<()> {
    if notify {
        run_notification(paths);
        return Ok(());
    }

    let snapshot = updates::load(paths)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
        return Ok(());
    }

    let Some(snapshot) = snapshot else {
        println!("No cached update results. Run 'upstream upgrade --check' to check for updates.");
        return Ok(());
    };

    if snapshot.updates.is_empty() {
        println!("No cached available updates.");
        return Ok(());
    }

    let header = format!(
        "{:<32} {:<20} {:<20} Checked",
        "Package", "Installed", "Available"
    );

    println!("{header}");
    println!("{}", crate::output::divider(header.len()));

    let now = Utc::now();
    for (index, (id, entry)) in snapshot.updates.into_iter().enumerate() {
        println!(
            "{}",
            crate::output::list_row(
                format!(
                    "{:<32} {:<20} {:<20} {}",
                    id,
                    entry.current,
                    entry.latest,
                    human_units::age(entry.checked_at, now)
                ),
                index
            )
        );
    }

    Ok(())
}

pub(crate) fn notification_message(
    snapshot: &UpdateSnapshot,
    now: chrono::DateTime<Utc>,
) -> Option<String> {
    let last_checked = snapshot
        .updates
        .values()
        .map(|entry| entry.checked_at)
        .max()?;

    let count = snapshot.updates.len();
    let noun = if count == 1 { "update" } else { "updates" };
    let age = human_units::compact_age(last_checked, now);
    Some(format!(
        "upstream: {count} pending software {noun} (last checked {age}).\nRun 'upstream updates' to view them.\n"
    ))
}

pub fn run_notification(paths: &UpstreamPaths) {
    if let Ok(Some(snapshot)) = updates::load(paths)
        && let Some(message) = notification_message(&snapshot, Utc::now())
    {
        // Shell startup must also tolerate a closed output pipe.
        let _ = io::stdout().lock().write_all(message.as_bytes());
    }
}
