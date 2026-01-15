use anyhow::Result;

use crate::{
    application::operations::init_operation::{cleanup, initialize},
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
