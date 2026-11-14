use crate::models::{common::enums::Filetype, upstream::Package};
use std::{fs, io::Read, path::Path};

/// Bounded discovery shared by the platform desktop handlers.
pub(super) fn package_has_desktop_data(package: &Package) -> bool {
    package.filetype == Filetype::Archive
        && package
            .install_path
            .as_deref()
            .is_some_and(|root| archive_has_desktop_data(root, package))
}

fn archive_has_desktop_data(root: &Path, package: &Package) -> bool {
    let app_name = package
        .repo_slug
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_lowercase();

    let mut icon = false;
    let mut terminal_entry = false;
    // One bounded pass; avoid following directory symlinks and skip common
    // documentation/dependency trees where unrelated artwork is common.
    let files = walkdir::WalkDir::new(root)
        .max_depth(8)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || !entry.file_type().is_dir()
                || !matches!(
                    entry.file_name().to_str(),
                    Some("node_modules" | ".git" | "docs" | "doc" | "tests" | "test")
                )
        });

    for entry in files.take(4096).flatten() {
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default();

        if extension.eq_ignore_ascii_case("desktop") {
            // Read a limited amount even if metadata lies or changes.
            let mut content = String::new();
            if fs::File::open(path)
                .and_then(|f| f.take(64 * 1024).read_to_string(&mut content))
                .is_err()
            {
                continue;
            }

            let mut entry = crate::models::common::DesktopEntry::default();
            let mut in_entry = false;
            for line in content.lines().map(str::trim) {
                if line.starts_with('[') {
                    in_entry = line == "[Desktop Entry]";
                } else if in_entry && let Some((key, value)) = line.split_once('=') {
                    entry.set_field(key.trim(), value.trim().to_string());
                }
            }

            if entry.exec.as_deref().is_none_or(|s| s.is_empty())
                || entry.extras.get("Type").is_none_or(|s| s != "Application")
                || entry
                    .extras
                    .get("Hidden")
                    .is_some_and(|s| s.eq_ignore_ascii_case("true"))
            {
                continue;
            }

            if !entry.terminal {
                return true;
            }

            terminal_entry = true;
        } else if ["png", "svg", "xpm", "ico"]
            .iter()
            .any(|ext| extension.eq_ignore_ascii_case(ext))
        {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_lowercase();

            let named = stem == app_name
                || package
                    .executables
                    .iter()
                    .any(|exec| stem == exec.name.to_lowercase());

            let icon_directory = path.strip_prefix(root).ok().is_some_and(|relative| {
                relative.parent().is_some_and(|parent| {
                    parent.components().any(|part| part.as_os_str() == "icons")
                })
            });

            if (entry.depth() == 1 || icon_directory)
                && (named || matches!(stem.as_str(), "icon" | "app-icon" | "app_icon"))
            {
                icon = true;
            }
        }
    }

    icon && !terminal_entry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::common::enums::{Channel, Provider};

    fn package(root: &Path) -> Package {
        let mut package = Package::with_defaults(
            "tool".into(),
            "owner/tool".into(),
            Filetype::Archive,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );

        package.install_path = Some(root.to_path_buf());
        package
    }

    #[test]
    fn archive_signals_exclude_artwork_and_terminal_entries() {
        let root = crate::utils::test_support::temp_root("desktop-detection", "archive");
        fs::create_dir_all(root.join("docs")).unwrap();
        let package = package(&root);
        fs::write(root.join("screenshot.png"), b"image").unwrap();
        fs::write(root.join("docs/icon.png"), b"image").unwrap();
        assert!(!archive_has_desktop_data(&root, &package));
        fs::write(root.join("icon.png"), b"image").unwrap();
        assert!(archive_has_desktop_data(&root, &package));
        fs::write(
            root.join("tool.desktop"),
            "[Desktop Entry]\nType=Application\nExec=tool\nTerminal=true\n",
        )
        .unwrap();
        assert!(!archive_has_desktop_data(&root, &package));
        fs::write(
            root.join("tool.desktop"),
            "[Desktop Entry]\nType=Application\nExec=tool\nTerminal=false\n",
        )
        .unwrap();
        assert!(archive_has_desktop_data(&root, &package));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn desktop_metadata_must_describe_an_application() {
        let root = crate::utils::test_support::temp_root("desktop-detection", "metadata");
        fs::create_dir_all(&root).unwrap();
        let package = package(&root);
        for contents in [
            "[Desktop Entry]\nType=Link\nExec=tool\n",
            "[Desktop Entry]\nType=Application\n",
            "[Desktop Entry]\nType=Application\nExec=tool\nHidden=true\n",
            "[Other]\nType=Application\nExec=tool\n",
        ] {
            fs::write(root.join("tool.desktop"), contents).unwrap();
            assert!(!archive_has_desktop_data(&root, &package), "{contents}");
        }

        fs::write(
            root.join("tool.desktop"),
            "[Desktop Entry]\nType=Application\nExec=tool\n",
        )
        .unwrap();
        assert!(archive_has_desktop_data(&root, &package));
        fs::remove_dir_all(root).unwrap();
    }
}
