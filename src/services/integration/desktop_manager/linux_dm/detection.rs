use crate::models::{common::enums::Filetype, upstream::Package};

pub(super) fn should_auto_integrate(package: &Package) -> bool {
    // The format itself is the signal; detection must not extract it.
    package.filetype == Filetype::AppImage
        || super::super::archive_detection::package_has_desktop_data(package)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::common::enums::{Channel, Provider};

    #[test]
    fn appimages_are_detected_without_opening_or_extracting_them() {
        let mut package = Package::with_defaults(
            "app".into(),
            "owner/app".into(),
            Filetype::AppImage,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        package.install_path = Some(std::path::PathBuf::from(
            "/nonexistent/application.AppImage",
        ));
        assert!(should_auto_integrate(&package));
        package.filetype = Filetype::Binary;
        assert!(!should_auto_integrate(&package));
    }
}
