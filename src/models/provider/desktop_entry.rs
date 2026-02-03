use std::path::Path;

#[derive(Debug, Default)]
pub struct DesktopEntry {
    pub name: Option<String>,
    pub comment: Option<String>,
    pub exec: Option<String>,
    pub icon: Option<String>,
    pub categories: Option<String>,
    pub terminal: bool,
}

impl DesktopEntry {
    /// Merge another DesktopEntry into self, preferring `other` when present
    pub fn merge(self, other: DesktopEntry) -> DesktopEntry {
        DesktopEntry {
            name: other.name.or(self.name),
            comment: other.comment.or(self.comment),
            exec: other.exec.or(self.exec),
            icon: other.icon.or(self.icon),
            categories: other.categories.or(self.categories),
            terminal: other.terminal || self.terminal,
        }
    }

    /// Sanitize fields that must always be overridden
    pub fn sanitize(mut self, exec: &Path, icon: &Path) -> DesktopEntry {
        self.exec = Some(exec.display().to_string());
        self.icon = Some(icon.display().to_string());
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

        out
    }
}
