use crate::{
    models::upstream::Package, services::storage::package_storage::PackageStorage,
    utils::static_paths::UpstreamPaths,
};

use anyhow::{Result, anyhow};

pub fn run(package_name: Option<String>) -> Result<()> {
    let paths = UpstreamPaths::new();
    let package_storage = PackageStorage::new(&paths.config.packages_file)?;

    match package_name {
        Some(name) => display_single_package(&package_storage, &name),
        None => display_all_packages(&package_storage),
    }
}

fn display_single_package(storage: &PackageStorage, name: &str) -> Result<()> {
    let package = storage
        .get_package_by_name(name)
        .ok_or_else(|| anyhow!("Package '{}' is not installed.", name))?;

    println!("{}", format_package_details(package));
    Ok(())
}

fn display_all_packages(storage: &PackageStorage) -> Result<()> {
    let packages = storage.get_all_packages();

    if packages.is_empty() {
        println!("No packages installed.");
        return Ok(());
    }

    print_package_table(packages);
    Ok(())
}

fn shorten_home_path(path: &str) -> String {
    if let Some(home) = dirs::home_dir()
        && let Some(home_str) = home.to_str()
        && path.starts_with(home_str)
    {
        return path.replacen(home_str, "~", 1);
    }
    path.to_string()
}

fn format_path(path: Option<&std::path::PathBuf>, default: &str) -> String {
    path.map(|p| shorten_home_path(&p.display().to_string()))
        .unwrap_or_else(|| default.to_string())
}

fn format_package_details(package: &Package) -> String {
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
        format_path(package.icon_path.as_ref(), "None"),
        format_path(package.install_path.as_ref(), "-"),
        format_path(package.exec_path.as_ref(), "-"),
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

impl ColumnWidths {
    fn from_packages(packages: &[Package]) -> Self {
        let mut widths = Self {
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
}

fn print_package_table(packages: &[Package]) {
    let widths = ColumnWidths::from_packages(packages);

    print_table_header(&widths);

    for package in packages {
        print_package_row(package, &widths);
    }

    println!("\nTotal: {} packages", packages.len());
}

fn print_table_header(widths: &ColumnWidths) {
    println!(
        "{:<name$} {:<repo$} {:<ver$} {:<chan$} {:<prov$} {:<3} {:<3} {:<12} {}",
        "Name",
        "Repo",
        "Version",
        "Channel",
        "Provider",
        "I",
        "P",
        "Last Updated",
        "Install Path",
        name = widths.name,
        repo = widths.repo,
        ver = widths.version,
        chan = widths.channel,
        prov = widths.provider
    );
}

fn print_package_row(package: &Package, widths: &ColumnWidths) {
    let install_path = format_path(package.install_path.as_ref(), "-");
    let icon_indicator = if package.icon_path.is_some() {
        "✓"
    } else {
        "-"
    };
    let pin_indicator = if package.is_pinned { "P" } else { "-" };
    let last_updated = package.last_upgraded.format("%Y-%m-%d").to_string();

    println!(
        "{:<name$} {:<repo$} {:<ver$} {:<chan$} {:<prov$} {:<3} {:<3} {:<12} {}",
        package.name,
        package.repo_slug,
        package.version.to_string(),
        package.channel.to_string(),
        package.provider.to_string(),
        icon_indicator,
        pin_indicator,
        last_updated,
        install_path,
        name = widths.name,
        repo = widths.repo,
        ver = widths.version,
        chan = widths.channel,
        prov = widths.provider
    );
}
