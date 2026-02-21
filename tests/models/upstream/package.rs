    use super::Package;
    use crate::models::common::enums::{Channel, Filetype, Provider};

    #[test]
    fn with_defaults_sets_expected_base_state() {
        let pkg = Package::with_defaults(
            "bat".to_string(),
            "sharkdp/bat".to_string(),
            Filetype::Auto,
            Some("linux".to_string()),
            Some("debug".to_string()),
            Channel::Stable,
            Provider::Github,
            None,
        );

        assert_eq!(pkg.version.major, 0);
        assert!(!pkg.is_pinned);
        assert!(pkg.install_path.is_none());
        assert!(pkg.exec_path.is_none());
        assert_eq!(pkg.match_pattern.as_deref(), Some("linux"));
        assert_eq!(pkg.exclude_pattern.as_deref(), Some("debug"));
    }

    #[test]
    fn is_same_as_uses_identity_fields_only() {
        let mut a = Package::with_defaults(
            "ripgrep".to_string(),
            "BurntSushi/ripgrep".to_string(),
            Filetype::Archive,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            Some("https://api.github.com".to_string()),
        );
        let mut b = a.clone();
        b.version.major = 99;
        b.is_pinned = true;
        b.match_pattern = Some("x86_64".to_string());
        assert!(a.is_same_as(&b));

        a.name = "rg".to_string();
        assert!(!a.is_same_as(&b));
    }
