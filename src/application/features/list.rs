use crate::{
    models::upstream::Package, services::storage::package_storage::PackageStorage,
    utils::static_paths::UpstreamPaths,
};
use console::Term;
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
    let mut packages = storage.get_all_packages().to_vec();
    packages.sort_by_key(|p| p.name.to_lowercase());

    if packages.is_empty() {
        println!("No packages installed.");
        return Ok(());
    }

    print_package_table(&packages);
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

fn truncate_cell(value: &str, max: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max {
        return value.to_string();
    }
    if max <= 3 {
        return ".".repeat(max);
    }

    let mut out = String::new();
    for ch in value.chars().take(max - 3) {
        out.push(ch);
    }
    out.push_str("...");
    out
}

fn format_package_details(package: &Package) -> String {
    let base_url = package.base_url.as_deref().unwrap_or("-");
    let match_pattern = package.match_pattern.as_deref().unwrap_or("-");
    let exclude_pattern = package.exclude_pattern.as_deref().unwrap_or("-");

    format!(
        "Package        : {}\n\
         Repo           : {}\n\
         Version        : {}\n\
         Channel        : {}\n\
         Provider       : {}\n\
         Type           : {:?}\n\
         Pinned         : {}\n\
         Has Icon       : {}\n\
         Base URL       : {}\n\
         Match Pattern  : {}\n\
         Excl. Pattern  : {}\n\
         Install Path   : {}\n\
         Executable Path: {}\n\
         Last Upgraded  : {}",
        package.name,
        package.repo_slug,
        package.version,
        package.channel,
        package.provider,
        package.filetype,
        if package.is_pinned { "yes" } else { "no" },
        if package.icon_path.is_some() { "yes" } else { "no" },
        base_url,
        match_pattern,
        exclude_pattern,
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
    flags: usize,
    updated: usize,
    path: usize,
}

impl ColumnWidths {
    fn from_packages(packages: &[Package], term_width: usize) -> Self {
        let max_name = packages.iter().map(|p| p.name.chars().count()).max().unwrap_or(4);
        let max_repo = packages
            .iter()
            .map(|p| p.repo_slug.chars().count())
            .max()
            .unwrap_or(4);
        let max_version = packages
            .iter()
            .map(|p| p.version.to_string().chars().count())
            .max()
            .unwrap_or(7);
        let max_channel = packages
            .iter()
            .map(|p| p.channel.to_string().chars().count())
            .max()
            .unwrap_or(7);
        let max_provider = packages
            .iter()
            .map(|p| p.provider.to_string().chars().count())
            .max()
            .unwrap_or(8);

        let mut widths = Self {
            name: max_name.clamp("Name".len(), 24),
            repo: max_repo.clamp("Repo".len(), 28),
            version: max_version.clamp("Version".len(), 16),
            channel: max_channel.clamp("Channel".len(), 10),
            provider: max_provider.clamp("Provider".len(), 10),
            flags: "Flags".len(),
            updated: "Updated".len().max(10),
            path: 30,
        };

        let non_path_width = widths.name
            + widths.repo
            + widths.version
            + widths.channel
            + widths.provider
            + widths.flags
            + widths.updated
            + 7; // spaces between columns
        let min_path = 16;
        let max_path = 56;

        widths.path = if term_width > non_path_width + min_path {
            (term_width - non_path_width).clamp(min_path, max_path)
        } else {
            min_path
        };

        if widths.path < "Install Path".len() {
            widths.path = "Install Path".len();
        }

        widths
    }
}

fn print_package_table(packages: &[Package]) {
    let terminal_cols = Term::stdout().size().1 as usize;
    let term_width = terminal_cols.max(80);
    let widths = ColumnWidths::from_packages(packages, term_width);

    print_table_header(&widths);

    for package in packages {
        print_package_row(package, &widths);
    }

    println!();
    println!("Total: {} packages", packages.len());
    println!("Flags: I=icon present, P=pinned");
}

fn print_table_header(widths: &ColumnWidths) {
    println!(
        "{:<name$} {:<repo$} {:<ver$} {:<chan$} {:<prov$} {:<flags$} {:<updated$} {:<path$}",
        "Name",
        "Repo",
        "Version",
        "Channel",
        "Provider",
        "Flags",
        "Updated",
        "Install Path",
        name = widths.name,
        repo = widths.repo,
        ver = widths.version,
        chan = widths.channel,
        prov = widths.provider,
        flags = widths.flags,
        updated = widths.updated,
        path = widths.path
    );
}

fn print_package_row(package: &Package, widths: &ColumnWidths) {
    let install_path = truncate_cell(&format_path(package.install_path.as_ref(), "-"), widths.path);
    let icon_indicator = if package.icon_path.is_some() { "I" } else { "-" };
    let pin_indicator = if package.is_pinned { "P" } else { "-" };
    let flags = format!("{icon_indicator}{pin_indicator}");
    let last_updated = package.last_upgraded.format("%Y-%m-%d").to_string();

    println!(
        "{:<name$} {:<repo$} {:<ver$} {:<chan$} {:<prov$} {:<flags$} {:<updated$} {:<path$}",
        truncate_cell(&package.name, widths.name),
        truncate_cell(&package.repo_slug, widths.repo),
        truncate_cell(&package.version.to_string(), widths.version),
        truncate_cell(&package.channel.to_string(), widths.channel),
        truncate_cell(&package.provider.to_string(), widths.provider),
        flags,
        last_updated,
        install_path,
        name = widths.name,
        repo = widths.repo,
        ver = widths.version,
        chan = widths.channel,
        prov = widths.provider,
        flags = widths.flags,
        updated = widths.updated,
        path = widths.path
    );
}
