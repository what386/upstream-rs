use std::path::Path;

pub fn shorten_upstream_package_path(path: &Path) -> Option<String> {
    let packages_dir = dirs::home_dir()?.join(".upstream").join("packages");
    let suffix = path.strip_prefix(packages_dir).ok()?;
    let suffix = suffix.to_string_lossy();
    if suffix.is_empty() {
        None
    } else {
        Some(suffix.into_owned())
    }
}

pub fn progress_name_width<'a>(names: impl IntoIterator<Item = &'a str>) -> usize {
    names
        .into_iter()
        .map(|name| name.chars().count())
        .max()
        .unwrap_or(0)
        .saturating_add(1)
}
