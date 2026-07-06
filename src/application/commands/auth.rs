use crate::output::Status;
use crate::{
    application::operations::auth_op::AuthUpdater,
    output, output::pager,
    storage::system::auth::AuthStorage,
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Result, anyhow};

pub fn run_set(set_keys: Vec<String>) -> Result<()> {
    if set_keys.is_empty() {
        return Err(anyhow!("At least one auth assignment is required"));
    }

    let paths = UpstreamPaths::new()?;
    let mut auth_storage = AuthStorage::new(&paths.config.auth_file)?;
    let mut auth_updater = AuthUpdater::new(&mut auth_storage);

    println!("{}", output::title("Auth set"));

    if set_keys.len() > 1 {
        let results = auth_updater.set_bulk(&set_keys);
        for applied in &results.applied {
            output::status_line(
                Status::Ok,
                &applied.key,
                format!("set to '{}'", applied.display_value),
            );
        }
        for (key, err) in &results.failures {
            output::status_line(Status::Fail, key, err);
        }
    } else {
        let applied = auth_updater.set_key(&set_keys[0])?;
        output::status_line(
            Status::Ok,
            &applied.key,
            format!("set to '{}'", applied.display_value),
        );
    }

    println!("{}", output::success("Auth saved."));
    Ok(())
}

pub fn run_get(get_keys: Vec<String>) -> Result<()> {
    if get_keys.is_empty() {
        return Err(anyhow!("At least one auth key is required"));
    }

    let paths = UpstreamPaths::new()?;
    let mut auth_storage = AuthStorage::new(&paths.config.auth_file)?;
    let auth_updater = AuthUpdater::new(&mut auth_storage);

    println!("{}", output::title("Auth get"));

    if get_keys.len() > 1 {
        let results = auth_updater.get_bulk(&get_keys);

        if results.values.is_empty() {
            println!("{}", output::warning("No values found."));
        } else {
            for (key, value) in results.values {
                output::kv(&key, value);
            }
        }
        for (key, err) in results.failures {
            output::status_line(Status::Fail, key, err);
        }
    } else {
        let value = auth_updater.get_key(&get_keys[0])?;
        output::kv(&get_keys[0], value);
    }

    Ok(())
}

pub fn run_list() -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let auth_storage = AuthStorage::new(&paths.config.auth_file)?;

    let flattened = auth_storage.get_flattened_auth();

    if flattened.is_empty() {
        println!("{}", output::warning("No auth tokens configured."));
        return Ok(());
    }

    let mut keys: Vec<_> = flattened.keys().collect();
    keys.sort();
    let mut auth_output = String::new();

    for key in keys {
        if let Some(value) = flattened.get(key) {
            auth_output.push_str(&format!("  {} = {}\n", key, value));
        }
    }

    pager::page_text(Some("Current auth"), &auth_output)?;
    Ok(())
}

pub fn run_reset() -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let mut auth_storage = AuthStorage::new(&paths.config.auth_file)?;

    output::confirm_or_cancel("Reset all auth tokens to empty?", false)?;
    auth_storage.reset_to_defaults()?;
    println!("{}", output::success("Auth reset to defaults."));

    Ok(())
}
