use crate::output::Status;
use crate::{
    application::operations::config_op::ConfigUpdater, output, output::pager,
    storage::config::ConfigStorage, utils::static_paths::UpstreamPaths,
};
use anyhow::{Result, anyhow};

pub fn run_set(set_keys: Vec<String>) -> Result<()> {
    if set_keys.is_empty() {
        return Err(anyhow!("At least one configuration assignment is required"));
    }

    let paths = UpstreamPaths::new()?;
    let mut config_storage = ConfigStorage::new(&paths.config.config_file)?;
    let mut config_updater = ConfigUpdater::new(&mut config_storage);

    println!("{}", output::title("Config set"));

    if set_keys.len() > 1 {
        let results = config_updater.set_bulk(&set_keys);
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
        let applied = config_updater.set_key(&set_keys[0])?;
        output::status_line(
            Status::Ok,
            &applied.key,
            format!("set to '{}'", applied.display_value),
        );
    }

    println!("{}", output::success("Configuration saved."));
    Ok(())
}

pub fn run_get(get_keys: Vec<String>) -> Result<()> {
    if get_keys.is_empty() {
        return Err(anyhow!("At least one configuration key is required"));
    }

    let paths = UpstreamPaths::new()?;
    let mut config_storage = ConfigStorage::new(&paths.config.config_file)?;
    let config_updater = ConfigUpdater::new(&mut config_storage);

    println!("{}", output::title("Config get"));

    if get_keys.len() > 1 {
        let results = config_updater.get_bulk(&get_keys);

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
        let value = config_updater.get_key(&get_keys[0])?;
        output::kv(&get_keys[0], value);
    }

    Ok(())
}

pub fn run_list() -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let config_storage = ConfigStorage::new(&paths.config.config_file)?;

    let flattened = config_storage.get_flattened_config();

    if flattened.is_empty() {
        println!("{}", output::warning("No configuration found."));
        return Ok(());
    }

    let mut keys: Vec<_> = flattened.keys().collect();
    keys.sort();
    let mut config_output = String::new();

    for key in keys {
        if let Some(value) = flattened.get(key) {
            config_output.push_str(&format!("  {} = {}\n", key, value));
        }
    }

    pager::page_text(Some("Current configuration"), &config_output)?;
    Ok(())
}

pub fn run_reset() -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let mut config_storage = ConfigStorage::new(&paths.config.config_file)?;

    output::confirm_or_cancel("Reset all configuration to defaults?", false)?;
    config_storage.reset_to_defaults()?;
    println!("{}", output::success("Configuration reset to defaults."));

    Ok(())
}

pub fn run_edit() -> Result<()> {
    let paths = UpstreamPaths::new()?;

    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| {
            if cfg!(target_os = "windows") {
                "notepad".to_string()
            } else {
                "nano".to_string()
            }
        });

    println!("{}", output::title("Config edit"));
    output::action_note(format!("Opening with {}", editor));

    let status = std::process::Command::new(&editor)
        .arg(&paths.config.config_file)
        .status()?;

    if status.success() {
        println!("{}", output::success("Editor closed."));

        // Validate the config can still be loaded
        match ConfigStorage::new(&paths.config.config_file) {
            Ok(_) => println!("{}", output::success("Configuration is valid.")),
            Err(e) => {
                println!(
                    "{}",
                    output::warning(format!("Configuration may have errors: {}", e))
                );
                output::action_note("Fix manually or run 'upstream config reset'.");
            }
        }
    } else {
        println!("{}", output::warning("Editor exited with error."));
    }

    Ok(())
}
