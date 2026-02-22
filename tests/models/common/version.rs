
use super::Version;

#[test]
fn parse_supports_short_and_full_versions() {
    assert_eq!(
        Version::parse("1").expect("parse 1"),
        Version::new(1, 0, 0, false)
    );
    assert_eq!(
        Version::parse("1.2").expect("parse 1.2"),
        Version::new(1, 2, 0, false)
    );
    assert_eq!(
        Version::parse("1.2.3").expect("parse 1.2.3"),
        Version::new(1, 2, 3, false)
    );
}

#[test]
fn parse_rejects_invalid_versions() {
    assert!(Version::parse("").is_err());
    assert!(Version::parse("1.2.3.4").is_err());
    assert!(Version::parse("v1.2.3").is_err());
    assert!(Version::parse("1.a.3").is_err());
}

#[test]
fn from_filename_extracts_triplet_when_present() {
    let parsed = Version::from_filename("tool-v2.15.9-linux-x86_64.tar.gz")
        .expect("version extracted from filename");
    assert_eq!(parsed, Version::new(2, 15, 9, false));
}

#[test]
fn from_tag_handles_common_prefixes() {
    assert_eq!(
        Version::from_tag("v1.2.3").expect("v-prefixed tag"),
        Version::new(1, 2, 3, false)
    );
    assert_eq!(
        Version::from_tag("release-7.8.9").expect("release-prefixed tag"),
        Version::new(7, 8, 9, false)
    );
    assert_eq!(
        Version::from_tag("VERSION-10.11.12").expect("case-insensitive prefix"),
        Version::new(10, 11, 12, false)
    );
}

#[test]
fn comparison_prefers_stable_over_prerelease_for_same_numbers() {
    let stable = Version::new(1, 0, 0, false);
    let prerelease = Version::new(1, 0, 0, true);

    assert!(stable > prerelease);
    assert!(stable.is_newer_than(&prerelease));
    assert!(!prerelease.is_newer_than(&stable));
}

#[test]
fn display_formats_prerelease_suffix() {
    assert_eq!(Version::new(1, 2, 3, false).to_string(), "1.2.3");
    assert_eq!(Version::new(1, 2, 3, true).to_string(), "1.2.3-pre");
}
