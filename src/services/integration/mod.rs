pub mod compression_handler;
pub mod permission_handler;

#[cfg(target_os = "linux")]
mod appimage_extractor;
mod desktop_manager;
mod icon_manager;
mod shell_manager;
mod symlink_manager;

#[cfg(target_os = "linux")]
pub use appimage_extractor::AppImageExtractor;
pub use desktop_manager::DesktopManager;
pub use icon_manager::IconManager;
pub use shell_manager::ShellManager;
pub use symlink_manager::SymlinkManager;
