pub mod compression_handler;
pub mod dotslash_parser;
pub mod permission_handler;
pub mod zsync_handler;

#[cfg(target_os = "linux")]
mod appimage_extractor;

#[cfg(target_os = "linux")]
pub use appimage_extractor::AppImageExtractor;
