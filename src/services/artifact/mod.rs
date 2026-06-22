pub mod compression_handler;
pub mod permission_handler;

#[cfg(target_os = "linux")]
mod appimage_extractor;

#[cfg(target_os = "linux")]
pub use appimage_extractor::AppImageExtractor;
