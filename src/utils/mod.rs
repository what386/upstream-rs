pub mod filename_parser;
pub mod filesystem;
pub mod math;
pub mod platform;

#[cfg(feature = "testing_donotuseinrelease")]
#[path = "../../tests/metadata/test_paths.rs"]
pub mod static_paths;

#[cfg(not(feature = "testing_donotuseinrelease"))]
pub mod static_paths;

/// shared test helpers
#[cfg(test)]
pub mod test_support;
