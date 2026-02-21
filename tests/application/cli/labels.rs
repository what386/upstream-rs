    use crate::application::cli::arguments::{Commands, ConfigAction, PackageAction};

    #[test]
    fn command_to_string_labels_are_stable() {
        assert_eq!(Commands::List { name: None }.to_string(), "list");
        assert_eq!(
            Commands::Config {
                action: ConfigAction::List
            }
            .to_string(),
            "config list"
        );
        assert_eq!(
            Commands::Package {
                action: PackageAction::Metadata {
                    name: "pkg".to_string()
                }
            }
            .to_string(),
            "package metadata"
        );
        assert_eq!(
            Commands::Package {
                action: PackageAction::Rename {
                    old_name: "old".to_string(),
                    new_name: "new".to_string()
                }
            }
            .to_string(),
            "package rename"
        );
    }
