pub mod compression_handler;
pub mod permission_handler;

mod appimage_extractor;
mod desktop_handler;
mod icon_handler;
mod shell_handler;
mod symlink_handler;

pub use appimage_extractor::AppImageExtractor;
pub use desktop_handler::DesktopManager;
pub use icon_handler::IconManager;
pub use shell_handler::ShellManager;
pub use symlink_handler::SymlinkManager;
