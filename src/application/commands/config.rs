use crate::{
    application::operations::config_operation::ConfigUpdater, application::output,
    services::storage::config_storage::ConfigStorage, utils::static_paths::UpstreamPaths,
};
use anyhow::{Result, anyhow};

pub fn run_set(set_keys: Vec<String>) -> Result<()> {
    if set_keys.is_empty() {
        return Err(anyhow!("At least one configuration assignment is required"));
    }

    let paths = UpstreamPaths::new()?;
    let mut config_storage = ConfigStorage::new(&paths.config.config_file)?;
    let mut config_updater = ConfigUpdater::new(&mut config_storage);

    let mut message_callback = Some(move |msg: &str| {
        println!("{}", msg);
    });

    if set_keys.len() > 1 {
        config_updater.set_bulk(&set_keys, &mut message_callback)?;
    } else {
        config_updater.set_key(&set_keys[0], &mut message_callback)?;
    }

    println!("Configuration saved!");
    Ok(())
}

pub fn run_get(get_keys: Vec<String>) -> Result<()> {
    if get_keys.is_empty() {
        return Err(anyhow!("At least one configuration key is required"));
    }

    let paths = UpstreamPaths::new()?;
    let mut config_storage = ConfigStorage::new(&paths.config.config_file)?;
    let config_updater = ConfigUpdater::new(&mut config_storage);

    let mut message_callback = Some(move |msg: &str| {
        println!("{}", msg);
    });

    if get_keys.len() > 1 {
        let results = config_updater.get_bulk(&get_keys, &mut message_callback)?;

        if results.is_empty() {
            println!("No values found");
        }
    } else {
        config_updater.get_key(&get_keys[0], &mut message_callback)?;
    }

    Ok(())
}

pub fn run_list(show_secrets: bool) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let config_storage = ConfigStorage::new(&paths.config.config_file)?;

    let flattened = config_storage.get_flattened_config();

    if flattened.is_empty() {
        println!("No configuration found");
        return Ok(());
    }

    println!("Current configuration:");
    println!();

    let mut keys: Vec<_> = flattened.keys().collect();
    keys.sort();

    for key in keys {
        if let Some(value) = flattened.get(key) {
            let display_value = format_config_value(key, value, show_secrets);
            println!("  {} = {}", key, display_value);
        }
    }

    Ok(())
}

fn format_config_value(key: &str, value: &str, show_secrets: bool) -> String {
    if !show_secrets && output::is_sensitive_key(key) {
        return output::redact_secret(value);
    }
    value.to_string()
}

pub fn run_reset() -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let mut config_storage = ConfigStorage::new(&paths.config.config_file)?;

    output::confirm_or_cancel("Reset all configuration to defaults?")?;
    config_storage.reset_to_defaults()?;
    println!("Configuration reset to defaults!");

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

    println!("Opening config file with {}...", editor);

    let status = std::process::Command::new(&editor)
        .arg(&paths.config.config_file)
        .status()?;

    if status.success() {
        println!("Config file closed");

        // Validate the config can still be loaded
        match ConfigStorage::new(&paths.config.config_file) {
            Ok(_) => println!("Configuration is valid"),
            Err(e) => {
                eprintln!("Warning: Configuration file may have errors: {}", e);
                eprintln!("You may need to fix it manually or run 'config reset'");
            }
        }
    } else {
        eprintln!("Editor exited with error");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::format_config_value;

    #[test]
    fn config_list_redacts_sensitive_values_by_default() {
        assert_eq!(
            format_config_value("github.api_token", "ghp_abcdefghijklmnopqrstuvwxyz", false),
            "ghp_...wxyz"
        );
        assert_eq!(
            format_config_value("github.api_token", "ghp_abcdefghijklmnopqrstuvwxyz", true),
            "ghp_abcdefghijklmnopqrstuvwxyz"
        );
        assert_eq!(format_config_value("github.enabled", "true", false), "true");
    }
}
