    use super::UpgradeOperation;
    use crate::models::common::enums::{Channel, Provider};

    #[test]
    fn truncate_error_adds_ellipsis_when_limit_exceeded() {
        let input = "this is a fairly long error string";
        let truncated = UpgradeOperation::truncate_error(input, 12);
        assert!(truncated.ends_with("..."));
        assert!(truncated.chars().count() <= 12);
    }

    #[test]
    fn format_transfer_handles_known_unknown_and_empty_sizes() {
        assert_eq!(UpgradeOperation::format_transfer(0, 0), "-");
        assert!(UpgradeOperation::format_transfer(42, 0).contains("42"));
        let known_total = UpgradeOperation::format_transfer(1024, 2048);
        assert!(known_total.contains('/'));
    }

    #[test]
    fn render_progress_row_includes_package_channel_provider_and_transfer() {
        let row = UpgradeOperation::render_progress_row(
            "ripgrep",
            &Channel::Stable,
            &Provider::Github,
            128,
            256,
        );
        assert!(row.contains("ripgrep"));
        assert!(row.contains("stable"));
        assert!(row.contains("github"));
        assert!(row.contains('/'));
    }
