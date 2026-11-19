use crate::models::common::enums::{Channel, Filetype, Provider};
use clap::Args;
use jiff::civil::Date;

#[derive(Args, Debug, Default)]
pub struct ProviderOptions {
    /// Source provider hosting the repository
    #[arg(short = 'p', long)]
    pub provider: Option<Provider>,

    /// Custom base URL
    #[arg(long, requires = "provider")]
    pub base_url: Option<String>,
}

#[derive(Args, Debug, Default)]
pub struct DesktopOptions {
    /// Create or disable a desktop launcher
    #[arg(short, long, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true", value_name = "BOOL")]
    pub desktop: Option<bool>,
}

#[derive(Args, Debug, Default)]
pub struct AssetOptions {
    /// Asset kind to install
    #[arg(short, long, value_enum)]
    pub kind: Option<Filetype>,

    /// Match pattern for asset selection
    #[arg(short = 'm', long, name = "match")]
    pub match_pattern: Option<String>,

    /// Exclude pattern for asset selection
    #[arg(short = 'e', long, name = "exclude")]
    pub exclude_pattern: Option<String>,
}

/// Options for installing packages.
#[derive(Args, Debug, Default)]
pub struct InstallOptions {
    /// Release tag to install
    #[arg(short, long)]
    pub tag: Option<String>,

    /// Semantic version to resolve to a release tag
    #[arg(short = 'v', long, conflicts_with = "tag")]
    pub semver: Option<String>,

    #[command(flatten)]
    pub asset_options: AssetOptions,

    #[command(flatten)]
    pub provider_options: ProviderOptions,

    /// Release channel to track for upgrades
    #[arg(short, long, value_enum)]
    pub channel: Option<Channel>,

    #[command(flatten)]
    pub desktop_options: DesktopOptions,
}

#[derive(Args, Debug)]
pub struct SearchOptions {
    /// Maximum number of results to display
    #[arg(short = 'l', long, default_value_t = 10)]
    pub limit: u32,

    /// Restrict results to repositories with this primary language
    #[arg(long)]
    pub language: Option<String>,

    /// Restrict results to repositories tagged with this topic
    #[arg(long)]
    pub topic: Option<String>,

    /// Restrict results to repositories with at least this many stars
    #[arg(long, value_name = "N")]
    pub min_stars: Option<u64>,

    /// Restrict results to repositories with at most this many stars
    #[arg(long, value_name = "N")]
    pub max_stars: Option<u64>,

    /// Restrict results to repositories pushed on or after YYYY-MM-DD
    #[arg(long, value_name = "YYYY-MM-DD", value_parser = parse_search_date)]
    pub pushed_after: Option<Date>,

    /// Include forked repositories in provider search results
    #[arg(long, default_value_t = false)]
    pub include_forks: bool,

    /// Include archived repositories in provider search results
    #[arg(long, default_value_t = false)]
    pub include_archived: bool,
}

fn parse_search_date(raw: &str) -> Result<Date, String> {
    Date::strptime("%Y-%m-%d", raw)
        .map_err(|_| format!("expected date in YYYY-MM-DD format, got '{raw}'"))
}
