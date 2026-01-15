pub mod compression_handler;
pub mod permission_handler;

mod desktop_handler;
mod icon_handler;
mod shell_handler;
mod symlink_handler;

pub use desktop_handler::DesktopManager;
pub use icon_handler::IconManager;
pub use shell_handler::ShellManager;
pub use symlink_handler::SymlinkManager;
