mod completion_manager;
mod desktop_manager;
mod icon_manager;
mod shell_manager;
mod symlink_manager;

pub use completion_manager::{CompletionManager, CompletionShell};
pub use desktop_manager::DesktopManager;
pub use icon_manager::IconManager;
pub use shell_manager::ShellManager;
#[cfg(unix)]
pub use shell_manager::{nushell_paths_file_contains_path, render_nushell_paths_file};
pub use symlink_manager::SymlinkManager;
