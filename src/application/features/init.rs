use anyhow::{Result, anyhow};

use crate::{
    application::operations::init_operation::{check, cleanup, initialize},
    utils::static_paths::UpstreamPaths,
};

pub fn run(cleanup_option: bool, check_option: bool) -> Result<()> {
    let paths = UpstreamPaths::new();

    if check_option {
        let report = check(&paths)?;
        for line in &report.messages {
            println!("{}", line);
        }

        if report.ok {
            println!("Initialization check passed.");
            return Ok(());
        }

        return Err(anyhow!("Initialization check failed"));
    }

    if cleanup_option {
        cleanup(&paths)?;
        println!("Cleared upstream data.")
    } else {
        initialize(&paths)?;
        println!("Initialized upstream.")
    }

    Ok(())
}
