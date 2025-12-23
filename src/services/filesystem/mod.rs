pub mod permission_handler;
pub mod compression_handler;

mod desktop_handler;
mod shell_handler;
mod symlink_handler;
mod icon_handler;

pub use shell_handler::ShellManager;
pub use symlink_handler::SymlinkManager;
pub use icon_handler::IconManager;
pub use desktop_handler::DesktopManager;
