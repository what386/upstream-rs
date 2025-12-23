use anyhow::Result;

use crate::{
    application::{
        features::upstream_init::{initialize, cleanup},
    },
    utils::static_paths::UpstreamPaths,
};

pub fn run(
    cleanup_option: bool,
) -> Result<()> {
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
