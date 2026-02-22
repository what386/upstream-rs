use std::{collections::BTreeMap, path::Path};

#[derive(Debug, Default)]
pub struct DesktopEntry {
    pub name: Option<String>,
    pub comment: Option<String>,
    pub exec: Option<String>,
    pub icon: Option<String>,
    pub categories: Option<String>,
    pub terminal: bool,
    pub extras: BTreeMap<String, String>,
}

impl DesktopEntry {
    /// Merge another DesktopEntry into self, preferring `other` when present
    pub fn merge(self, other: DesktopEntry) -> DesktopEntry {
        let mut extras = self.extras;
        extras.extend(other.extras);

        DesktopEntry {
            name: other.name.or(self.name),
            comment: other.comment.or(self.comment),
            exec: other.exec.or(self.exec),
            icon: other.icon.or(self.icon),
            categories: other.categories.or(self.categories),
            terminal: other.terminal || self.terminal,
            extras,
        }
    }

    /// Set a parsed key/value pair, storing unknown keys in `extras`.
    pub fn set_field(&mut self, key: &str, value: String) {
        match key {
            "Name" => self.name = Some(value),
            "Comment" => self.comment = Some(value),
            "Exec" => self.exec = Some(value),
            "Icon" => self.icon = Some(value),
            "Categories" => self.categories = Some(value),
            "Terminal" => self.terminal = value.eq_ignore_ascii_case("true"),
            _ => {
                self.extras.insert(key.to_string(), value);
            }
        }
    }

    pub fn ensure_name(mut self, fallback: &str) -> DesktopEntry {
        if self.name.is_some() {
            return self;
        }

        if let Some(localized_name) = self
            .extras
            .iter()
            .find_map(|(key, value)| key.starts_with("Name[").then_some(value.as_str()))
        {
            self.name = Some(localized_name.to_string());
            return self;
        }

        self.name = Some(fallback.to_string());
        self
    }

    /// Sanitize fields that must always be overridden
    pub fn sanitize(mut self, exec: &Path, icon: Option<&Path>) -> DesktopEntry {
        self.exec = Some(exec.display().to_string());
        self.icon = Some(
            icon.map(|path| path.display().to_string())
                .unwrap_or_default(),
        );
        self.terminal = false;
        self
    }

    /// Render to XDG .desktop format
    pub fn to_desktop_file(&self) -> String {
        let mut out = String::from("[Desktop Entry]\nType=Application\nVersion=1.0\n");

        if let Some(name) = &self.name {
            out.push_str(&format!("Name={}\n", name));
        }

        if let Some(exec) = &self.exec {
            out.push_str(&format!("Exec={}\n", exec));
        }

        if let Some(icon) = &self.icon {
            out.push_str(&format!("Icon={}\n", icon));
        }

        if let Some(comment) = &self.comment {
            out.push_str(&format!("Comment={}\n", comment));
        }

        out.push_str(&format!(
            "Categories={}\n",
            self.categories.as_deref().unwrap_or("Application;")
        ));

        out.push_str(&format!("Terminal={}\n", self.terminal));

        for (key, value) in &self.extras {
            if matches!(
                key.as_str(),
                "Type"
                    | "Version"
                    | "Name"
                    | "Exec"
                    | "Icon"
                    | "Comment"
                    | "Categories"
                    | "Terminal"
            ) {
                continue;
            }
            out.push_str(&format!("{key}={value}\n"));
        }

        out
    }
}
