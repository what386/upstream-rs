pub mod compression_handler;
pub mod permission_handler;

#[cfg(target_os = "linux")]
mod appimage_extractor;
mod completion_manager;
mod desktop_manager;
mod icon_manager;
mod shell_manager;
mod symlink_manager;

#[cfg(target_os = "linux")]
pub use appimage_extractor::AppImageExtractor;
pub use completion_manager::{
    CompletionCacheMismatch, CompletionCacheMismatchKind, CompletionManager, CompletionShell,
};
pub use desktop_manager::DesktopManager;
pub use icon_manager::IconManager;
pub use shell_manager::ShellManager;
#[cfg(unix)]
pub use shell_manager::{nushell_paths_file_contains_path, render_nushell_paths_file};
pub use symlink_manager::SymlinkManager;
