use anyhow::Result;

use crate::{
    application::operations::upstream_init::{cleanup, initialize},
    utils::static_paths::UpstreamPaths,
};

pub fn run(cleanup_option: bool) -> Result<()> {
    let paths = UpstreamPaths::new();

    if cleanup_option {
        cleanup(&paths)?;
        println!("Cleared upstream data.")
    } else {
        initialize(&paths)?;
        println!("Initialized upstream.")
    }

    Ok(())
}
