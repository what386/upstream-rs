use crate::{
    models::upstream::Package, services::storage::package_storage::PackageStorage,
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Result, anyhow};
use std::path::PathBuf;

pub fn run(package_name: Option<String>) -> Result<()> {
    let paths = UpstreamPaths::new();
    let package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let pkgfile = &paths.config.packages_file.display();
    println!("{}", pkgfile);

    if let Some(name) = package_name.as_ref() {
        let package = package_storage
            .get_package_by_name(name)
            .ok_or_else(|| anyhow!("Package '{}' is not installed.", name))?;
        println!("{}", display_all(package));
    } else {
        let packages = package_storage.get_all_packages();
        display_compact_table(&packages);
    }
    Ok(())
}

fn shorten_home_path(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Some(home_str) = home.to_str() {
            if path.starts_with(home_str) {
                return path.replacen(home_str, "~", 1);
            }
        }
    }
    path.to_string()
}

fn display_all(package: &Package) -> String {
    let install_path = package
        .install_path
        .as_ref()
        .map(|p| shorten_home_path(&p.display().to_string()))
        .unwrap_or_else(|| "-".to_string());
    let exec_path = package
        .exec_path
        .as_ref()
        .map(|p| shorten_home_path(&p.display().to_string()))
        .unwrap_or_else(|| "-".to_string());
    let icon_path = package
        .icon_path
        .as_ref()
        .map(|p| shorten_home_path(&p.display().to_string()))
        .unwrap_or_else(|| "None".to_string());

    format!(
        "Package: {} ({})\n\
         Version: {}\n\
         Channel: {}\n\
         Provider: {}\n\
         Type: {:?}\n\
         Pinned: {}\n\
         Icon: {}\n\
         Install Path: {}\n\
         Executable Path: {}\n\
         Last Upgraded: {}",
        package.name,
        package.repo_slug,
        package.version,
        package.channel,
        package.provider,
        package.filetype,
        if package.is_pinned { "Yes" } else { "No" },
        icon_path,
        install_path,
        exec_path,
        package.last_upgraded.format("%Y-%m-%d %H:%M:%S UTC")
    )
}

struct ColumnWidths {
    name: usize,
    repo: usize,
    version: usize,
    channel: usize,
    provider: usize,
}

fn calculate_column_widths(packages: &[Package]) -> ColumnWidths {
    let mut widths = ColumnWidths {
        name: "Name".len(),
        repo: "Repo".len(),
        version: "Version".len(),
        channel: "Channel".len(),
        provider: "Provider".len(),
    };

    for package in packages {
        widths.name = widths.name.max(package.name.len());
        widths.repo = widths.repo.max(package.repo_slug.len());
        widths.version = widths.version.max(package.version.to_string().len());
        widths.channel = widths.channel.max(package.channel.to_string().len());
        widths.provider = widths.provider.max(package.provider.to_string().len());
    }

    widths
}

fn display_compact_table(packages: &[Package]) {
    if packages.is_empty() {
        return;
    }

    let widths = calculate_column_widths(packages);

    // Print header with proper padding
    println!(
        "{:<width_name$} {:<width_repo$} {:<width_ver$} {:<width_chan$} {:<width_prov$} {:<3} {:<3} {:<}",
        "Name",
        "Repo",
        "Version",
        "Channel",
        "Provider",
        "I",
        "P",
        "Install Path",
        width_name = widths.name,
        width_repo = widths.repo,
        width_ver = widths.version,
        width_chan = widths.channel,
        width_prov = widths.provider
    );

    // Print rows
    for package in packages {
        let install_path = package
            .install_path
            .as_ref()
            .map(|p| shorten_home_path(&p.display().to_string()))
            .unwrap_or_else(|| "-".to_string());

        let icon_indicator = if package.icon_path.is_some() {
            "✓"
        } else {
            "-"
        };
        let pin_indicator = if package.is_pinned { "P" } else { "-" };

        let width_name = widths.name;
        let width_repo = widths.repo;
        let width_ver = widths.version;
        let width_chan = widths.channel;
        let width_prov = widths.provider;

        println!(
            "{:<width_name$} {:<width_repo$} {:<width_ver$} {:<width_chan$} {:<width_prov$} {:<3} {:<3} {:<}",
            package.name,
            package.repo_slug,
            package.version.to_string(),
            package.channel.to_string(),
            package.provider.to_string(),
            icon_indicator,
            pin_indicator,
            install_path,
        );
    }
}
