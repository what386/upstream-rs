use anyhow::{Result, anyhow};

use crate::{
    models::upstream::Package, services::storage::package_storage::PackageStorage,
    utils::static_paths::UpstreamPaths,
};

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
        println!("{}", display_compact_header());

        for package in package_storage.get_all_packages() {
            println!("{}", display_compact(package));
        }
    }

    Ok(())
}

fn display_all(package: &Package) -> String {
    let install_path = package
        .install_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "-".to_string());

    let exec_path = package
        .exec_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "-".to_string());

    format!(
        "Package: {} ({})\n\
         Version: {}\n\
         Channel: {}\n\
         Provider: {}\n\
         Type: {:?}\n\
         Paused: {}\n\
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
        // TODO: display icon path
        if package.icon_path.is_some() {
            "Yes"
        } else {
            "None"
        },
        if package.is_pinned { "Yes" } else { "No" },
        install_path,
        exec_path,
        package.last_upgraded.format("%Y-%m-%d %H:%M:%S UTC")
    )
}

fn display_compact(package: &Package) -> String {
    let install_path = package
        .install_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "-".to_string());

    let icon_path = package
        .icon_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "✗".to_string());

    format!(
        "{:<20} {:<15} {:<10} {:<8} {:<8} {:<3} {:<3} {:<}",
        package.name,
        package.repo_slug,
        package.version,
        package.channel,
        package.provider,
        icon_path,
        if package.is_pinned { "P" } else { "-" },
        install_path
    )
}

fn display_compact_header() -> String {
    format!(
        "{:<20} {:<15} {:<10} {:<8} {:<8} {:<3} {:<3} {:<}",
        "Name", "Repo", "Version", "Channel", "Provider", "Icon", "P", "Install Path"
    )
}
