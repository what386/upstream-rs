use crate::{
    models::upstream::{InstallType, Package},
    output,
    output::pager,
    storage::database::PackageDatabase,
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Result, anyhow};
use console::Term;
use std::{fmt::Write as _, path::Path};

pub fn run(package_name: Option<String>, json: bool) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let package_database = PackageDatabase::open(&paths.config.packages_database_file)?;

    if json {
        return match package_name {
            Some(name) => print_single_json(&package_database, &name),
            None => print_all_json(&package_database),
        };
    }

    match package_name {
        Some(name) => display_single_package(&package_database, &name),
        None => display_all_packages(&package_database),
    }
}

fn print_single_json(storage: &PackageDatabase, name: &str) -> Result<()> {
    let package = storage
        .get_package(name)?
        .ok_or_else(|| anyhow!("Package '{}' is not installed.", name))?;
    println!("{}", serde_json::to_string_pretty(&package)?);
    Ok(())
}

fn print_all_json(storage: &PackageDatabase) -> Result<()> {
    let packages = storage.list_packages()?;
    println!("{}", serde_json::to_string_pretty(&packages)?);
    Ok(())
}

fn display_single_package(storage: &PackageDatabase, name: &str) -> Result<()> {
    let package = storage
        .get_package(name)?
        .ok_or_else(|| anyhow!("Package '{}' is not installed.", name))?;

    pager::page_text(None, &format_package_details(&package))?;
    Ok(())
}

fn display_all_packages(storage: &PackageDatabase) -> Result<()> {
    let mut packages = storage.list_packages()?;
    packages.sort_by_key(|p| p.name.to_lowercase());

    if packages.is_empty() {
        println!("{}", output::warning("No packages installed."));
        return Ok(());
    }

    let title = format!("Packages ({})  Flags: D=desktop, P=pinned", packages.len());
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

fn shorten_upstream_package_path(path: &Path) -> Option<String> {
    let packages_dir = dirs::home_dir()?.join(".upstream").join("packages");
    let suffix = path.strip_prefix(packages_dir).ok()?;
    let suffix = suffix.to_string_lossy();
    if suffix.is_empty() {
        None
    } else {
        Some(suffix.into_owned())
    }
}

fn format_path(path: Option<&std::path::PathBuf>, default: &str) -> String {
    path.map(|p| {
        shorten_upstream_package_path(p)
            .unwrap_or_else(|| shorten_home_path(&p.display().to_string()))
    })
    .unwrap_or_else(|| default.to_string())
}

fn write_detail_field(out: &mut String, label: &str, value: impl AsRef<str>) {
    writeln!(out, "{label:<10} {}", value.as_ref()).expect("write package detail field");
}

fn package_detail_heading(package: &Package) -> String {
    format!(
        "{} {} ({})",
        package.name,
        package_ref_label(package),
        package.repo_slug
    )
}

fn format_package_details(package: &Package) -> String {
    let mut out = String::new();
    let heading = package_detail_heading(package);

    writeln!(out, "{heading}").expect("write package heading");
    writeln!(out, "{}", output::divider(heading.chars().count())).expect("write package divider");
    write_detail_field(&mut out, "Provider", package.provider.to_string());
    write_detail_field(
        &mut out,
        "Channel",
        package.channel.to_string().to_ascii_lowercase(),
    );
    write_detail_field(&mut out, "Kind", package_kind_label(package));
    write_detail_field(
        &mut out,
        "Updated",
        package
            .last_upgraded
            .format("%Y-%m-%d %H:%M UTC")
            .to_string(),
    );

    if let Some(base_url) = package.base_url.as_deref() {
        write_detail_field(&mut out, "Base URL", base_url);
    }

    if matches!(package.install_type, InstallType::Build)
        || package.build_branch.is_some()
        || package.build_commit.is_some()
    {
        out.push('\n');
        writeln!(out, "Build").expect("write build section");
        if let Some(branch) = package.build_branch.as_deref() {
            write_detail_field(&mut out, "Branch", branch);
        }
        if let Some(commit) = package.build_commit.as_deref() {
            write_detail_field(&mut out, "Commit", commit);
        }
    }

    out.push('\n');
    writeln!(out, "Install").expect("write install section");
    write_detail_field(
        &mut out,
        "Type",
        format!("{:?}", package.filetype).to_ascii_lowercase(),
    );
    write_detail_field(
        &mut out,
        "Path",
        format_path(package.install_path.as_ref(), "-"),
    );
    write_detail_field(
        &mut out,
        "Command",
        format_path(package.exec_path.as_ref(), "-"),
    );
    write_detail_field(
        &mut out,
        "Desktop",
        if package.icon_path.is_some() {
            "yes"
        } else {
            "no"
        },
    );
    write_detail_field(
        &mut out,
        "Pinned",
        if package.is_pinned { "yes" } else { "no" },
    );

    if !package.match_pattern.is_empty() || !package.exclude_pattern.is_empty() {
        out.push('\n');
        writeln!(out, "Selection").expect("write selection section");
        if !package.match_pattern.is_empty() {
            write_detail_field(&mut out, "Match", package.match_pattern.to_string());
        }
        if !package.exclude_pattern.is_empty() {
            write_detail_field(&mut out, "Exclude", package.exclude_pattern.to_string());
        }
    }

    out
}

fn short_commit(commit: &str) -> String {
    commit.chars().take(7).collect()
}

fn package_kind_label(package: &Package) -> &'static str {
    match package.install_type {
        InstallType::Release => "release",
        InstallType::Build => "build",
    }
}

fn package_ref_label(package: &Package) -> String {
    match package.install_type {
        InstallType::Release => package.version.to_string(),
        InstallType::Build => {
            let label = package
                .build_branch
                .as_deref()
                .map(str::to_string)
                .unwrap_or_else(|| package.version.to_string());
            match package.build_commit.as_deref() {
                Some(commit) if !commit.is_empty() => format!("{label}@{}", short_commit(commit)),
                _ => label,
            }
        }
    }
}

struct ColumnWidths {
    name: usize,
    repo: usize,
    kind: usize,
    reference: usize,
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
        let max_kind = packages
            .iter()
            .map(|p| package_kind_label(p).chars().count())
            .max()
            .unwrap_or("Kind".len());
        let max_ref = packages
            .iter()
            .map(|p| package_ref_label(p).chars().count())
            .max()
            .unwrap_or("Ref".len());
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
            kind: max_kind.clamp("Kind".len(), "release".len()),
            reference: max_ref.clamp("Ref".len(), 18),
            channel: max_channel.clamp("Channel".len(), 10),
            provider: max_provider.clamp("Provider".len(), 10),
            flags: "Flags".len(),
            updated: "Updated".len().max(10),
            path: 30,
        };

        let non_path_width = widths.name
            + widths.repo
            + widths.kind
            + widths.reference
            + widths.channel
            + widths.provider
            + widths.flags
            + widths.updated
            + 8; // spaces between columns
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
        + widths.kind
        + widths.reference
        + widths.channel
        + widths.provider
        + widths.flags
        + widths.updated
        + widths.path
        + 8
}

fn write_table_header(out: &mut String, widths: &ColumnWidths) {
    writeln!(
        out,
        "{:<name$} {:<repo$} {:<kind$} {:<reference$} {:<chan$} {:<prov$} {:<flags$} {:<updated$} {:<path$}",
        "Name",
        "Repo",
        "Kind",
        "Ref",
        "Channel",
        "Provider",
        "Flags",
        "Updated",
        "Install Path",
        name = widths.name,
        repo = widths.repo,
        kind = widths.kind,
        reference = widths.reference,
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
    let package_ref = package_ref_label(package);

    writeln!(
        out,
        "{:<name$} {:<repo$} {:<kind$} {:<reference$} {:<chan$} {:<prov$} {:<flags$} {:<updated$} {:<path$}",
        output::truncate_end(&package.name, widths.name),
        output::truncate_end(&package.repo_slug, widths.repo),
        package_kind_label(package),
        output::truncate_end(&package_ref, widths.reference),
        output::truncate_end(&package.channel.to_string(), widths.channel),
        output::truncate_end(&package.provider.to_string(), widths.provider),
        flags,
        last_updated,
        install_path,
        name = widths.name,
        repo = widths.repo,
        kind = widths.kind,
        reference = widths.reference,
        chan = widths.channel,
        prov = widths.provider,
        flags = widths.flags,
        updated = widths.updated,
        path = widths.path
    )
    .expect("write package row");
}
