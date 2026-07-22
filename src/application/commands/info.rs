use crate::{
    models::upstream::{InstallType, Package},
    output,
    output::pager,
    storage::database::PackageDatabase,
    utils::{name_match, static_paths::UpstreamPaths},
};
use anyhow::{Result, anyhow};
use std::{fmt::Write as _, path::Path};

pub fn run(query: String, json: bool, paths: &UpstreamPaths) -> Result<()> {
    let package_database = PackageDatabase::open(&paths.config.packages_database_file)?;

    if json {
        return print_info_json(&package_database, &query);
    }

    display_package_info(&package_database, &query)
}

fn print_info_json(storage: &PackageDatabase, query: &str) -> Result<()> {
    let packages = storage.list_packages()?;
    let resolved = resolve_package_query(&packages, query)?;
    println!("{}", serde_json::to_string_pretty(resolved)?);
    Ok(())
}

fn display_package_info(storage: &PackageDatabase, query: &str) -> Result<()> {
    let packages = storage.list_packages()?;
    let resolved = resolve_package_query(&packages, query)?;
    let header = format!("Exact match: {}", resolved.name);

    pager::page_text(Some(&header), &format_package_details(resolved))?;
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

fn resolve_package_query<'a>(packages: &'a [Package], query: &str) -> Result<&'a Package> {
    if let Some(package) = packages
        .iter()
        .find(|package| package.name.eq_ignore_ascii_case(query))
    {
        return Ok(package);
    }

    let suggestions = name_match::suggestions(
        packages.iter().map(|package| package.name.as_str()),
        query,
        3,
    );
    Err(anyhow!(
        "No installed package matches '{}'.{}",
        query,
        name_match::did_you_mean(&suggestions)
    ))
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

#[cfg(test)]
mod tests {
    use super::resolve_package_query;
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
    fn package_query_suggests_unique_substring_without_selecting_it() {
        let packages = vec![package("codex"), package("ripgrep")];
        let error = resolve_package_query(&packages, "code").expect_err("must remain exact");
        assert_eq!(
            error.to_string(),
            "No installed package matches 'code'. Did you mean: codex?"
        );
    }

    #[test]
    fn package_query_prefers_exact_match_over_substring_matches() {
        let packages = vec![package("code"), package("codex")];

        let resolved = resolve_package_query(&packages, "code").expect("resolve package");

        assert_eq!(resolved.name, "code");
    }

    #[test]
    fn package_query_lists_multiple_suggestions() {
        let packages = vec![package("codex"), package("vscode")];
        let error = resolve_package_query(&packages, "code").expect_err("ambiguous query");

        assert_eq!(
            error.to_string(),
            "No installed package matches 'code'. Did you mean: codex, vscode?"
        );
    }
}
