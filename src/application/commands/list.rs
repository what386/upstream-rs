use crate::{
    models::upstream::{InstallType, Package},
    output,
    output::pager,
    storage::database::PackageDatabase,
    utils::static_paths::UpstreamPaths,
};
use anyhow::Result;
use console::Term;
use std::{fmt::Write as _, path::Path};

pub fn run(filter: Option<String>, json: bool) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let package_database = PackageDatabase::open(&paths.config.packages_database_file)?;

    if json {
        return print_list_json(&package_database, filter.as_deref());
    }

    display_package_list(&package_database, filter.as_deref())
}

fn print_list_json(storage: &PackageDatabase, filter: Option<&str>) -> Result<()> {
    let packages = storage.list_packages()?;
    let packages = filter_packages_by_name(packages, filter);
    println!("{}", serde_json::to_string_pretty(&packages)?);
    Ok(())
}

fn display_package_list(storage: &PackageDatabase, filter: Option<&str>) -> Result<()> {
    let packages = storage.list_packages()?;
    let mut packages = filter_packages_by_name(packages, filter);

    if packages.is_empty() {
        match filter {
            Some(filter) => println!(
                "{}",
                output::warning(format!("No installed packages match '{}'.", filter))
            ),
            None => println!("{}", output::warning("No packages installed.")),
        }
        return Ok(());
    }

    packages.sort_by_key(|p| p.name.to_lowercase());

    let title = match filter {
        Some(filter) => format!(
            "Packages matching '{}' ({})  Flags: D=desktop, P=pinned",
            filter,
            packages.len()
        ),
        None => format!("Packages ({})  Flags: D=desktop, P=pinned", packages.len()),
    };
    pager::page_text(Some(&title), &format_package_table(&packages))?;
    Ok(())
}

fn filter_packages_by_name(packages: Vec<Package>, filter: Option<&str>) -> Vec<Package> {
    let Some(filter) = filter else {
        return packages;
    };
    let filter = filter.to_lowercase();

    packages
        .into_iter()
        .filter(|package| package.name.to_lowercase().contains(&filter))
        .collect()
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

#[cfg(test)]
mod tests {
    use super::filter_packages_by_name;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;

    fn package(name: &str) -> Package {
        Package::with_defaults(
            name.to_string(),
            format!("owner/{name}"),
            Filetype::Archive,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        )
    }

    #[test]
    fn package_list_filter_keeps_name_substring_matches() {
        let packages = vec![package("codex"), package("ripgrep"), package("vscode")];
        let filtered = filter_packages_by_name(packages, Some("code"));
        let names = filtered
            .iter()
            .map(|package| package.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["codex", "vscode"]);
    }
}
