
use super::PackageInstaller;

#[test]
fn package_cache_key_sanitizes_disallowed_characters() {
    let key = PackageInstaller::package_cache_key("my/pkg v1.0");
    assert!(key.starts_with("my_pkg_v1_0-"));
    assert!(
        key.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    );
}
