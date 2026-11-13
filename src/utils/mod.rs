pub mod filesystem;
pub mod human_units;
pub mod math;
pub mod name_match;
pub mod platform;

#[cfg(feature = "testing_donotuseinrelease")]
#[path = "../../tests/support/test_paths.rs"]
pub mod static_paths;

#[cfg(not(feature = "testing_donotuseinrelease"))]
pub mod static_paths;

/// shared test helpers
#[cfg(test)]
pub mod test_support;
