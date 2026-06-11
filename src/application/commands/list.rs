use crate::{
    models::upstream::Package, output, output::pager,
    services::storage::package_storage::PackageStorage, utils::static_paths::UpstreamPaths,
};
use anyhow::{Result, anyhow};
use console::Term;
use std::fmt::Write as _;

pub fn run(package_name: Option<String>, json: bool) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let package_storage = PackageStorage::new(&paths.config.packages_file)?;

    if json {
        return match package_name {
            Some(name) => print_single_json(&package_storage, &name),
            None => print_all_json(&package_storage),
        };
    }

    match package_name {
        Some(name) => display_single_package(&package_storage, &name),
        None => display_all_packages(&package_storage),
    }
}

fn print_single_json(storage: &PackageStorage, name: &str) -> Result<()> {
    let package = storage
        .get_package_by_name(name)
        .ok_or_else(|| anyhow!("Package '{}' is not installed.", name))?;
    println!("{}", serde_json::to_string_pretty(package)?);
    Ok(())
}

fn print_all_json(storage: &PackageStorage) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(storage.get_all_packages())?
    );
    Ok(())
}

fn display_single_package(storage: &PackageStorage, name: &str) -> Result<()> {
    let package = storage
        .get_package_by_name(name)
        .ok_or_else(|| anyhow!("Package '{}' is not installed.", name))?;

    pager::page_text(None, &format_package_details(package))?;
    Ok(())
}

fn display_all_packages(storage: &PackageStorage) -> Result<()> {
    let mut packages = storage.get_all_packages().to_vec();
    packages.sort_by_key(|p| p.name.to_lowercase());

    if packages.is_empty() {
        println!("{}", output::warning("No packages installed."));
        return Ok(());
    }

    let title = format!(
        "Packages ({})  Flags: D=desktop present, P=pinned",
        packages.len()
    );
    pager::page_text(Some(&title), &format_package_table(&packages))?;
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
    let base_url = package.base_url.as_deref().unwrap_or("-");
    let build_branch = package.build_branch.as_deref().unwrap_or("-");
    let build_commit = package.build_commit.as_deref().unwrap_or("-");
    let match_pattern = package.match_pattern.as_deref().unwrap_or("-");
    let exclude_pattern = package.exclude_pattern.as_deref().unwrap_or("-");

    format!(
        "Package        : {}\n\
         Repo           : {}\n\
         Version        : {}\n\
         Channel        : {}\n\
         Provider       : {}\n\
         Install Type   : {:?}\n\
         Type           : {:?}\n\
         Pinned         : {}\n\
         Has Icon       : {}\n\
         Base URL       : {}\n\
         Build Branch   : {}\n\
         Build Commit   : {}\n\
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
        package.install_type,
        package.filetype,
        if package.is_pinned { "yes" } else { "no" },
        if package.icon_path.is_some() {
            "yes"
        } else {
            "no"
        },
        base_url,
        build_branch,
        build_commit,
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
        let max_name = packages
            .iter()
            .map(|p| p.name.chars().count())
            .max()
            .unwrap_or(4);
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

fn format_package_table(packages: &[Package]) -> String {
    let terminal_cols = Term::stdout().size().1 as usize;
    let term_width = terminal_cols.max(80);
    let widths = ColumnWidths::from_packages(packages, term_width);
    let mut out = String::new();

    write_table_header(&mut out, &widths);
    writeln!(out, "{}", output::divider(table_width(&widths))).expect("write table divider");

    for package in packages {
        write_package_row(&mut out, package, &widths);
    }

    out.push('\n');
    out
}

fn table_width(widths: &ColumnWidths) -> usize {
    widths.name
        + widths.repo
        + widths.version
        + widths.channel
        + widths.provider
        + widths.flags
        + widths.updated
        + widths.path
        + 7
}

fn write_table_header(out: &mut String, widths: &ColumnWidths) {
    writeln!(
        out,
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
    )
    .expect("write table header");
}

fn write_package_row(out: &mut String, package: &Package, widths: &ColumnWidths) {
    let install_path = output::truncate_middle(
        &format_path(package.install_path.as_ref(), "-"),
        widths.path,
    );
    let desktop_indicator = if package.icon_path.is_some() {
        "D"
    } else {
        "-"
    };
    let pin_indicator = if package.is_pinned { "P" } else { "-" };
    let flags = format!("{desktop_indicator}{pin_indicator}");
    let last_updated = package.last_upgraded.format("%Y-%m-%d").to_string();

    writeln!(
        out,
        "{:<name$} {:<repo$} {:<ver$} {:<chan$} {:<prov$} {:<flags$} {:<updated$} {:<path$}",
        output::truncate_end(&package.name, widths.name),
        output::truncate_end(&package.repo_slug, widths.repo),
        output::truncate_end(&package.version.to_string(), widths.version),
        output::truncate_end(&package.channel.to_string(), widths.channel),
        output::truncate_end(&package.provider.to_string(), widths.provider),
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
    )
    .expect("write package row");
}
