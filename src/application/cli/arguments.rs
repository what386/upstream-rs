use crate::models::common::enums::{Channel, Filetype, Provider, TrustMode};
use chrono::NaiveDate;
use clap::{Parser, Subcommand};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum BuildProfile {
    Rust,
    Dotnet,
    Go,
    Zig,
    Cmake,
}

#[derive(Parser)]
#[command(name = "upstream")]
#[command(about = "Install and manage apps from upstream releases.")]
#[command(
    long_about = "Upstream installs applications from provider releases, direct \
    download pages, and source repositories, then tracks them for upgrades, \
    rollback, trust verification, documentation lookup, and shell/desktop integration.\n\n\
    EXAMPLES:\n  \
    upstream install BurntSushi/ripgrep rg -k binary\n  \
    upstream find terminal emulator --limit 20\n  \
    upstream probe neovim/neovim\n  \
    upstream upgrade --check\n  \
    upstream config set github.api_token=ghp_xxx"
)]
#[command(
    version,
    long_version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("GIT_HASH"), ")")
)]
pub struct Cli {
    /// Accept confirmation prompts automatically
    #[arg(short = 'y', long, global = true, default_value_t = false)]
    pub yes: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Install a release asset or direct download
    #[command(long_about = "Install a release asset or direct download.\n\n\
        Resolves a compatible asset from the selected provider, channel, and tag, \
        downloads it, verifies it according to the selected trust mode, installs it, \
        and records the package for future upgrades. If the name is omitted for a \
        git repository, upstream uses the repository name. Direct HTTP sources may \
        require an explicit name.\n\n\
        EXAMPLES:\n  \
        upstream install BurntSushi/ripgrep rg -k binary\n  \
        upstream install bootandy/dust       # name inferred as dust\n  \
        upstream install neovim/neovim nvim --desktop\n  \
        upstream install sharkdp/bat bat --tag v0.25.0")]
    Install {
        /// Repository identifier or direct download URL
        repo_slug: String,

        /// Name to register the application under (falls back to git repository name when omitted)
        name: Option<String>,

        /// Release tag to install (defaults to latest matching the channel)
        #[arg(short, long)]
        tag: Option<String>,

        /// Asset kind to install
        #[arg(short, long, value_enum, default_value_t = Filetype::Auto)]
        kind: Filetype,

        /// Source provider hosting the repository. Defaults to auto-detection.
        #[arg(short = 'p', long)]
        provider: Option<Provider>,

        /// Custom base URL. Defaults to provider's root
        #[arg(long, requires = "provider")]
        base_url: Option<String>,

        /// Release channel to track for upgrades
        #[arg(short, long, value_enum, default_value_t = Channel::Stable)]
        channel: Channel,

        /// Match pattern to use as a hint for which asset to prefer
        #[arg(short = 'm', long, name = "match")]
        match_pattern: Option<String>,

        /// Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")
        #[arg(short = 'e', long, name = "exclude")]
        exclude_pattern: Option<String>,

        /// Create a desktop launcher entry for GUI applications
        #[arg(short, long, default_value_t = false)]
        desktop: bool,

        /// Trust verification mode for downloaded assets
        #[arg(long = "trust", value_enum, default_value_t = TrustMode::BestEffort)]
        trust_mode: TrustMode,

        /// Preview install resolution without downloading, installing, or writing metadata
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Build and install a package from source
    #[command(long_about = "Build and install a package from source.\n\n\
        Clones or updates a cached source checkout, selects a tag or branch, runs the \
        detected or requested build profile, installs the produced artifact, and records \
        the package for future rebuilds/upgrades. Use this when release artifacts are \
        unavailable or unsuitable. If the name is omitted for a git repository, upstream \
        uses the repository name.\n\n\
        EXAMPLES:\n  \
        upstream build BurntSushi/ripgrep rg\n  \
        upstream build BurntSushi/ripgrep       # name inferred as ripgrep\n  \
        upstream build BurntSushi/ripgrep rg --branch main\n  \
        upstream build BurntSushi/ripgrep rg --build-profile rust\n  \
        upstream build owner/repo app --build-profile dotnet --tag v1.2.3")]
    Build {
        /// Repository identifier or git URL
        repo_slug: String,

        /// Name to register the application under (falls back to git repository name when omitted)
        name: Option<String>,

        /// Release tag to build (defaults to latest matching the channel)
        #[arg(short, long, conflicts_with = "branch")]
        tag: Option<String>,

        /// Branch to build from instead of a release tag
        #[arg(long, conflicts_with = "tag")]
        branch: Option<String>,

        /// Source provider hosting the repository. Defaults to auto-detection.
        #[arg(short = 'p', long)]
        provider: Option<Provider>,

        /// Custom base URL. Defaults to provider's root
        #[arg(long, requires = "provider")]
        base_url: Option<String>,

        /// Release channel to track for future builds
        #[arg(short, long, value_enum, default_value_t = Channel::Stable)]
        channel: Channel,

        /// Create a desktop launcher entry for GUI applications
        #[arg(short, long, default_value_t = false)]
        desktop: bool,

        /// Build profile used to compile/install from source (auto-detected when omitted)
        #[arg(long, value_enum)]
        build_profile: Option<BuildProfile>,

        /// Preview build resolution without compiling, installing, or writing metadata
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Remove installed package files and metadata
    #[command(
        visible_alias = "uninstall",
        long_about = "Remove installed package files and metadata.\n\n\
        By default, removes active package files, shell/desktop integration, and package \
        records while preserving reusable cache data and rollback artifacts. Use --purge \
        to remove package-owned cache data too. Use --force to remove metadata even when \
        file cleanup fails.\n\n\
        EXAMPLES:\n  \
        upstream remove nvim\n  \
        upstream remove rg fd bat --purge\n  \
        upstream remove rg --force\n  \
        upstream remove rg --dry-run"
    )]
    Remove {
        /// Names of packages to remove
        names: Vec<String>,

        /// Remove package-owned cached data as well as active files
        #[arg(long, default_value_t = false)]
        purge: bool,

        /// Remove metadata even when uninstall cleanup fails
        #[arg(long, default_value_t = false)]
        force: bool,

        /// Preview removal actions without deleting files or metadata
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Restore or prune stored rollback artifacts
    #[command(long_about = "Restore or prune stored rollback artifacts.\n\n\
        Provide package names to restore their latest rollback artifacts. Use --list to \
        inspect available artifacts. Use --prune to delete all rollback data, \
        --prune <names...> to delete selected packages, or --prune all to make the \
        all-packages intent explicit.\n\n\
        EXAMPLES:\n  \
        upstream rollback rg\n  \
        upstream rollback rg fd --dry-run\n  \
        upstream rollback --list\n  \
        upstream rollback --prune\n  \
        upstream rollback --prune rg")]
    Rollback {
        /// Package names to restore
        #[arg(num_args(0..), value_name = "NAMES")]
        names: Vec<String>,

        /// List available rollback artifacts
        #[arg(long, default_value_t = false)]
        list: bool,

        /// Delete rollback artifacts for all packages or selected package names
        #[arg(long, num_args(0..), value_name = "NAMES")]
        prune: Option<Vec<String>>,

        /// Preview rollback restore or prune actions without modifying files or metadata
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Reinstall packages from their stored source metadata
    #[command(
        long_about = "Reinstall packages from their stored source metadata.\n\n\
        Release packages are removed and installed again from the currently recorded \
        version tag. Build packages are removed and rebuilt from their recorded source. \
        Use --dry-run to preview resolution without touching files.\n\n\
        EXAMPLES:\n  \
        upstream reinstall nvim\n  \
        upstream reinstall rg fd\n  \
        upstream reinstall rg --force\n  \
        upstream reinstall rg --trust none"
    )]
    Reinstall {
        /// Installed package names to reinstall
        names: Vec<String>,

        /// Trust verification mode for release-asset reinstalls
        #[arg(long = "trust", value_enum, default_value_t = TrustMode::BestEffort)]
        trust_mode: TrustMode,

        /// Continue reinstalling after uninstall cleanup errors
        #[arg(long, default_value_t = false)]
        force: bool,

        /// Preview reinstall resolution without removing, installing, or writing metadata
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Check for or install package updates
    #[command(long_about = "Check for or install package updates.\n\n\
        Without package names, checks or upgrades every installed package. With names, \
        limits the operation to those packages. Use --check for a read-only update \
        report, --json for structured check output, or --machine-readable for one \
        script-friendly line per available update. At the interactive upgrade prompt, \
        enter c to view release notes before deciding.\n\n\
        EXAMPLES:\n  \
        upstream upgrade              # Upgrade all\n  \
        upstream upgrade nvim rg      # Upgrade specific packages\n  \
        upstream upgrade --check      # Check for updates\n  \
        upstream upgrade --check --json # Check for updates as JSON\n  \
        upstream upgrade --check --machine-readable # Script-friendly output\n  \
        upstream upgrade nvim --force # Force reinstall\n  \
        upstream upgrade --trust none")]
    Upgrade {
        /// Installed package names to upgrade (all packages if omitted)
        names: Option<Vec<String>>,

        /// Reinstall even when the selected version is already installed
        #[arg(long, default_value_t = false)]
        force: bool,

        /// Check for available upgrades without applying them
        #[arg(long, default_value_t = false)]
        check: bool,

        /// Print one line per available update: "name oldver newver"
        #[arg(long, default_value_t = false, requires = "check")]
        machine_readable: bool,

        /// Print check results as JSON
        #[arg(
            long,
            default_value_t = false,
            requires = "check",
            conflicts_with = "machine_readable"
        )]
        json: bool,

        /// Trust verification mode for downloaded assets
        #[arg(long = "trust", value_enum, default_value_t = TrustMode::BestEffort)]
        trust_mode: TrustMode,

        /// Preview upgrade resolution without downloading, installing, or writing metadata
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// List installed packages
    #[command(long_about = "List installed packages.\n\n\
        Without a filter, shows every installed package. Provide a filter to show only \
        package names containing that string. Use --json for the full structured package \
        records.\n\n\
        EXAMPLES:\n  \
        upstream list       # List all packages\n  \
        upstream list code  # List installed packages whose names contain code")]
    List {
        /// Package name substring to filter the list
        filter: Option<String>,

        /// Print package list as JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Show details for one installed package
    #[command(long_about = "Show details for one installed package.\n\n\
        The query can be an exact package name or a unique substring. Exact names \
        take precedence over substring matches. Use --json to print the raw stored \
        package record.\n\n\
        EXAMPLES:\n  \
        upstream info nvim  # Show details for nvim\n  \
        upstream info code  # Show details when exactly one package contains code")]
    Info {
        /// Package name or unique substring for detailed information
        query: String,

        /// Print raw package metadata as JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Show release notes for an installed package
    #[command(long_about = "Show release notes for an installed package.\n\n\
        By default, prints release bodies newer than the installed version through \
        the latest release on the package's tracked channel. Use --for to show one \
        tag. Use --from and --to to override range endpoints with release tags, \
        current, or latest. If glow is installed, changelog Markdown is rendered \
        with glow's terminal styling.\n\n\
        EXAMPLES:\n  \
        upstream changelog nvim\n  \
        upstream changelog nvim --for v0.11.0\n  \
        upstream changelog nvim --from current --to latest\n  \
        upstream changelog nvim --from v0.10.0\n  \
        upstream changelog nvim --from v0.10.0 --to v0.11.0")]
    Changelog {
        /// Installed package name
        name: String,

        /// Starting release tag, or "current"
        #[arg(long = "from", conflicts_with = "for_tag")]
        from_tag: Option<String>,

        /// Ending release tag, "current", or "latest"
        #[arg(long = "to", conflicts_with = "for_tag")]
        to_tag: Option<String>,

        /// Show release notes for exactly one release tag
        #[arg(long = "for", conflicts_with_all = ["from_tag", "to_tag"])]
        for_tag: Option<String>,
    },

    /// Search cached or fetched package README docs
    #[command(long_about = "Search package README documentation.\n\n\
        Fetches the installed package's upstream README, caches it for offline use, \
        parses Markdown sections, and opens ranked keyword matches in an interactive \
        picker with a live preview. If no keywords are provided, sections are shown \
        in README order. If fetching fails and a cached README exists, \
        upstream falls back to the cached copy. Use --offline to skip fetching and \
        search only cached documentation. If glow is installed, Markdown previews \
        and selected sections are rendered with glow's terminal styling. Use \
        --fetch [names...] to refresh cached READMEs without opening the picker; \
        omitting names refreshes all installed packages.\n\n\
        EXAMPLES:\n  \
        upstream docs rg\n  \
        upstream docs rg usage\n  \
        upstream docs rg --offline usage\n  \
        upstream docs --fetch\n  \
        upstream docs --fetch rg bat\n  \
        upstream docs ripgrep configuration file\n  \
        upstream docs bat themes syntax")]
    Docs {
        /// Installed package name to search, unless --fetch is refreshing all docs
        name: Option<String>,

        /// Use only the cached README and skip network fetching
        #[arg(long, default_value_t = false, conflicts_with = "fetch")]
        offline: bool,

        /// Refresh cached README docs for named packages, or all installed packages when empty
        #[arg(long, num_args = 0.., value_name = "NAME")]
        fetch: Option<Vec<String>>,

        /// Optional search keywords (joined with spaces)
        #[arg(num_args(0..), value_delimiter = ' ')]
        keywords: Vec<String>,
    },
    /// Inspect releases, choose an asset, and install it
    #[command(long_about = "Inspect releases, choose an asset, and install it.\n\n\
        Probe lists compatible release assets for a repository or scraped download page, \
        opens an interactive asset picker, prompts for a package name when needed, and \
        installs the selected asset. In AUTO kind mode, probe shows installable asset \
        kinds for the current platform instead of forcing one kind. Use --dry-run to \
        inspect parsed releases without selecting, downloading, or installing.\n\n\
        EXAMPLES:\n  \
        upstream probe neovim/neovim\n  \
        upstream probe https://ziglang.org/download/ -p scraper --limit 20\n  \
        upstream probe owner/repo tool --desktop\n  \
        upstream probe owner/repo --include-incompatible\n  \
        upstream probe owner/repo --limit 20\n  \
        upstream probe owner/repo --tag v1.2.3\n  \
        upstream probe owner/repo -k archive\n  \
        upstream probe owner/repo --dry-run\n  \
        upstream probe owner/repo --json")]
    Probe {
        /// Repository identifier or download page URL to inspect
        repo_slug: String,

        /// Name to register the application under (prompts with inferred default when omitted)
        name: Option<String>,

        /// Source provider (defaults to GitHub, or scraper for plain URLs)
        #[arg(short = 'p', long)]
        provider: Option<Provider>,

        /// Custom base URL for self-hosted providers
        #[arg(long)]
        base_url: Option<String>,

        /// Release channel to display and track
        #[arg(short, long, value_enum, default_value_t = Channel::Stable)]
        channel: Channel,

        /// Number of releases to inspect instead of only one tag/latest release
        #[arg(long)]
        limit: Option<u32>,

        /// Release tag to inspect exactly
        #[arg(long, value_name = "TAG")]
        tag: Option<String>,

        /// Asset kind to show and install
        #[arg(short, long, value_enum, default_value_t = Filetype::Auto)]
        kind: Filetype,

        /// Include assets that do not match the current OS/architecture or selected file type
        #[arg(long, default_value_t = false)]
        include_incompatible: bool,

        /// Print probe results as JSON and exit
        #[arg(long, default_value_t = false)]
        json: bool,

        /// Create a desktop launcher entry for GUI applications
        #[arg(short, long, default_value_t = false)]
        desktop: bool,

        /// Trust verification mode for downloaded assets
        #[arg(long = "trust", value_enum, default_value_t = TrustMode::BestEffort)]
        trust_mode: TrustMode,

        /// Show parsed releases without selecting, downloading, or installing
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Search provider repositories without installing
    #[command(long_about = "Search provider repositories without installing.\n\n\
        Search is for discovery and inspection: it prints matching repositories, applies \
        optional metadata filters, and exits. Use `upstream find` when you want the \
        interactive search-and-install flow. Defaults to GitHub when provider is omitted.\n\n\
        EXAMPLES:\n  \
        upstream search\n  \
        upstream search ripgrep\n  \
        upstream search editor --language Rust --min-stars 100 --max-stars 50000\n  \
        upstream search rip grep --limit 5\n  \
        upstream search my tool -p github\n  \
        upstream search cli --topic terminal\n  \
        upstream search ripgrep --json")]
    Search {
        /// Optional query words
        #[arg(num_args(0..), value_delimiter = ' ')]
        query_words: Vec<String>,

        /// Source provider to search (defaults to GitHub)
        #[arg(short = 'p', long)]
        provider: Option<Provider>,

        /// Custom base URL for self-hosted providers
        #[arg(long, requires = "provider")]
        base_url: Option<String>,

        /// Maximum number of results to display
        #[arg(long, default_value_t = 10)]
        limit: u32,

        /// Restrict results to repositories with this primary language
        #[arg(long)]
        language: Option<String>,

        /// Restrict results to repositories tagged with this topic
        #[arg(long)]
        topic: Option<String>,

        /// Restrict results to repositories with at least this many stars
        #[arg(long, value_name = "N")]
        min_stars: Option<u64>,

        /// Restrict results to repositories with at most this many stars
        #[arg(long, value_name = "N")]
        max_stars: Option<u64>,

        /// Restrict results to repositories pushed on or after YYYY-MM-DD
        #[arg(long, value_name = "YYYY-MM-DD", value_parser = parse_search_date)]
        pushed_after: Option<NaiveDate>,

        /// Include forked repositories in provider search results
        #[arg(long, default_value_t = false)]
        include_forks: bool,

        /// Include archived repositories in provider search results
        #[arg(long, default_value_t = false)]
        include_archived: bool,

        /// Print repository search results as JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Search repositories interactively and install one
    #[command(long_about = "Search repositories interactively and install one.\n\n\
        Find runs provider search, opens a result picker, prompts for the package name \
        using the selected repository name as the default, then installs the selected \
        repository through the normal release-asset flow. Use --name to skip the name \
        prompt. Defaults to GitHub when provider is omitted.\n\n\
        EXAMPLES:\n  \
        upstream find ripgrep\n  \
        upstream find terminal emulator --limit 20\n  \
        upstream find cli --language Rust --topic cli\n  \
        upstream find ripgrep --name rg -k binary\n  \
        upstream find app -p github --desktop --trust none")]
    Find {
        /// Query words
        #[arg(required = true, num_args(1..), value_delimiter = ' ')]
        query_words: Vec<String>,

        /// Source provider to search (defaults to GitHub)
        #[arg(short = 'p', long)]
        provider: Option<Provider>,

        /// Custom base URL for self-hosted providers
        #[arg(long, requires = "provider")]
        base_url: Option<String>,

        /// Maximum number of results to display
        #[arg(long, default_value_t = 10)]
        limit: u32,

        /// Restrict results to repositories with this primary language
        #[arg(long)]
        language: Option<String>,

        /// Restrict results to repositories tagged with this topic
        #[arg(long)]
        topic: Option<String>,

        /// Restrict results to repositories with at least this many stars
        #[arg(long, value_name = "N")]
        min_stars: Option<u64>,

        /// Restrict results to repositories with at most this many stars
        #[arg(long, value_name = "N")]
        max_stars: Option<u64>,

        /// Restrict results to repositories pushed on or after YYYY-MM-DD
        #[arg(long, value_name = "YYYY-MM-DD", value_parser = parse_search_date)]
        pushed_after: Option<NaiveDate>,

        /// Include forked repositories in provider search results
        #[arg(long, default_value_t = false)]
        include_forks: bool,

        /// Include archived repositories in provider search results
        #[arg(long, default_value_t = false)]
        include_archived: bool,

        /// Package name to register without prompting
        #[arg(long)]
        name: Option<String>,

        /// Asset kind to install
        #[arg(short, long, value_enum, default_value_t = Filetype::Auto)]
        kind: Filetype,

        /// Release channel to track for upgrades
        #[arg(short, long, value_enum, default_value_t = Channel::Stable)]
        channel: Channel,

        /// Match pattern to use as a hint for which asset to prefer
        #[arg(short = 'm', long, name = "match")]
        match_pattern: Option<String>,

        /// Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")
        #[arg(short = 'e', long, name = "exclude")]
        exclude_pattern: Option<String>,

        /// Create a desktop launcher entry for GUI applications
        #[arg(short, long, default_value_t = false)]
        desktop: bool,

        /// Trust verification mode for downloaded assets
        #[arg(long = "trust", value_enum, default_value_t = TrustMode::BestEffort)]
        trust_mode: TrustMode,

        /// Preview install resolution without downloading, installing, or writing metadata
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// View, edit, and validate config.toml
    #[command(long_about = "View, edit, and validate config.toml.\n\n\
        Config values are stored in TOML under upstream's data directory. Missing keys \
        use built-in defaults. Use verify to find missing default-backed keys and unused \
        keys left behind by old versions or manual edits.\n\n\
        EXAMPLES:\n  \
        upstream config set github.api_token=ghp_xxx\n  \
        upstream config get download.high_threads\n  \
        upstream config list\n  \
        upstream config verify\n  \
        upstream config edit")]
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Manage installed package records and launcher entries
    #[command(
        long_about = "Manage installed package records and launcher entries.\n\n\
        Use these commands to update upstream's stored package metadata. Pin or \
        unpin a package's upgrade state, rename a local package alias, or manually \
        add/remove an upstream-managed desktop launcher entry.\n\n\
        EXAMPLES:\n  \
        upstream package pin nvim\n  \
        upstream package unpin nvim\n  \
        upstream package rename nvim neovim\n  \
        upstream package add-entry nvim\n  \
        upstream package rm-entry nvim"
    )]
    Package {
        #[command(subcommand)]
        action: PackageAction,
    },

    /// Manage shell PATH hooks and local upstream data
    #[command(long_about = "Manage upstream shell integration hooks.\n\n\
        Use these commands to add, verify, or remove shell PATH hooks and required \
        upstream directories. Purge is destructive: it removes shell hooks and deletes \
        the local upstream data directory, including installed package files and metadata.\n\n\
        EXAMPLES:\n  \
        upstream hooks init\n  \
        upstream hooks check\n  \
        upstream hooks clean\n  \
        upstream --yes hooks purge")]
    Hooks {
        #[command(subcommand)]
        action: HooksAction,
    },

    /// Import config, trust keys, packages, or a profile
    #[command(long_about = "Import config, trust keys, packages, or a profile.\n\n\
        Package and profile imports reinstall release packages from exported references. \
        They do not contain installed artifacts, rollback data, or cache contents.\n\n\
        EXAMPLES:\n  \
        upstream import config ./config.toml\n  \
        upstream import keys ./minisign.pub\n  \
        upstream import keys ./cosign.pub\n  \
        upstream import packages ./packages.json\n  \
        upstream import packages ./packages.json --latest\n  \
        upstream import profile ./profile.json")]
    Import {
        #[command(subcommand)]
        action: ImportAction,
    },

    /// Export config, trust keys, packages, or a profile
    #[command(long_about = "Export config, trust keys, packages, or a profile.\n\n\
        Package exports contain reinstallable release-package references, not installed \
        artifacts. Profile exports combine config, trust keys, and package references for \
        backup or transfer.\n\n\
        EXAMPLES:\n  \
        upstream export config ./config.toml\n  \
        upstream export keys ./keys.json\n  \
        upstream export packages ./packages.json\n  \
        upstream export profile ./profile.json")]
    Export {
        #[command(subcommand)]
        action: ExportAction,
    },

    /// Run diagnostics to detect installation and integration issues
    #[command(
        long_about = "Inspect upstream installation health and package state.\n\n\
        Checks package paths, symlinks, shell PATH integration, completion directories, \
        desktop/icon files, and metadata. \
        Reports a compact summary by default and includes actionable hints. \
        Use --verbose to print each individual check result. Use --fix to repair \
        supported issues such as PATH hooks, missing symlinks, executable bits, \
        executable metadata, and unused config keys. Use --migrate after \
        upgrading across breaking local data changes when diagnostics or release notes \
        ask for a data migration.\n\n\
        EXAMPLES:\n  \
        upstream doctor\n  \
        upstream doctor --verbose\n  \
        upstream doctor --fix\n  \
        upstream doctor --migrate\n  \
        upstream doctor nvim ripgrep\n  \
        upstream doctor --json"
    )]
    Doctor {
        /// Package names to check (all installed packages if omitted)
        names: Vec<String>,

        /// Print each check result line in addition to summary output
        #[arg(long, default_value_t = false)]
        verbose: bool,

        /// Attempt automatic repairs for detected issues
        #[arg(long, default_value_t = false)]
        fix: bool,

        /// Migrate local upstream data after breaking layout or metadata changes
        #[arg(
            long,
            default_value_t = false,
            conflicts_with_all = ["names", "verbose", "fix", "json"]
        )]
        migrate: bool,

        /// Print diagnostic report as JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

fn parse_search_date(raw: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .map_err(|_| format!("expected date in YYYY-MM-DD format, got '{raw}'"))
}

impl Commands {
    pub fn requires_lock(&self) -> bool {
        match self {
            Commands::List { .. } => false,
            Commands::Info { .. } => false,
            Commands::Changelog { .. } => false,
            Commands::Docs { .. } => false,
            Commands::Doctor { fix, migrate, .. } => *fix || *migrate,
            Commands::Search { .. } => false,
            Commands::Find { .. } => true,
            Commands::Rollback { list: true, .. } => false,
            Commands::Hooks { action } => !matches!(action, HooksAction::Check),
            Commands::Package { .. } => true,
            Commands::Config { action } => !matches!(
                action,
                ConfigAction::Get { .. } | ConfigAction::List | ConfigAction::Verify
            ),
            Commands::Install { .. }
            | Commands::Build { .. }
            | Commands::Remove { .. }
            | Commands::Rollback { .. }
            | Commands::Reinstall { .. }
            | Commands::Upgrade { .. }
            | Commands::Probe { .. }
            | Commands::Import { .. }
            | Commands::Export { .. } => true,
        }
    }
}

#[derive(Subcommand)]
pub enum ImportAction {
    /// Replace config.toml from an export
    #[command(long_about = "Replace config.toml from an exported TOML file.\n\n\
        The imported file is parsed with the current config schema before it replaces \
        the active config. Missing keys continue to use built-in defaults.\n\n\
        EXAMPLE:\n  \
        upstream import config ./config.toml")]
    Config {
        /// Path to an upstream config TOML file
        path: std::path::PathBuf,
    },

    /// Import trusted minisign or cosign public keys
    #[command(long_about = "Import trusted minisign or cosign public keys.\n\n\
        Imported keys are merged into upstream's trust store and can satisfy future \
        release-asset verification.\n\n\
        EXAMPLES:\n  \
        upstream import keys ./minisign.pub\n  \
        upstream import keys ./cosign.pub")]
    Keys {
        /// Path to a minisign or cosign public key file
        path: std::path::PathBuf,
    },

    /// Install packages from an exported package list
    #[command(long_about = "Install packages from an exported package list.\n\n\
        Package exports contain release-package references, not installed files. By \
        default, upstream installs each package at the version tag recorded in the export. \
        Use --latest to ignore recorded tags and install current releases instead.\n\n\
        EXAMPLES:\n  \
        upstream import packages ./packages.json\n  \
        upstream import packages ./packages.json --latest")]
    Packages {
        /// Path to an upstream packages export
        path: std::path::PathBuf,

        /// Continue installing remaining packages after a package import fails
        #[arg(long, default_value_t = false)]
        skip_failed: bool,

        /// Ignore exported version tags and install latest releases
        #[arg(long, default_value_t = false)]
        latest: bool,
    },

    /// Import config, keys, and packages from a profile
    #[command(
        long_about = "Import config, trusted keys, and packages from a profile export.\n\n\
        Profile imports apply config first, merge trust keys second, and install release \
        packages last. By default, package imports use the version tags recorded in the \
        profile. Use --latest to install current releases instead.\n\n\
        EXAMPLES:\n  \
        upstream import profile ./profile.json\n  \
        upstream import profile ./profile.json --latest"
    )]
    Profile {
        /// Path to an upstream profile export
        path: std::path::PathBuf,

        /// Continue installing remaining packages after a package import fails
        #[arg(long, default_value_t = false)]
        skip_failed: bool,

        /// Ignore exported package version tags and install latest releases
        #[arg(long, default_value_t = false)]
        latest: bool,
    },
}

#[derive(Subcommand)]
pub enum ExportAction {
    /// Export config.toml
    #[command(long_about = "Export the active upstream config as TOML.\n\n\
        The export includes config values only. Trust keys, packages, rollback data, \
        installed files, and cache contents are not included.\n\n\
        EXAMPLE:\n  \
        upstream export config ./config.toml")]
    Config {
        /// Output path for the config export
        path: std::path::PathBuf,
    },

    /// Export trusted minisign and cosign public keys
    #[command(long_about = "Export trusted minisign and cosign public keys.\n\n\
        The export can be imported later with `upstream import keys`.\n\n\
        EXAMPLE:\n  \
        upstream export keys ./keys.json")]
    Keys {
        /// Output path for the keys export
        path: std::path::PathBuf,
    },

    /// Export installed release-package references
    #[command(long_about = "Export installed release-package references.\n\n\
        The output records enough source and version information for `upstream import \
        packages` to reinstall release packages. It does not include installed files, \
        build-only package artifacts, rollback data, or cache contents.\n\n\
        EXAMPLE:\n  \
        upstream export packages ./packages.json")]
    Packages {
        /// Output path for the packages export
        path: std::path::PathBuf,
    },

    /// Export config, trust keys, and package references
    #[command(
        long_about = "Export config, trust keys, and installed package references.\n\n\
        The output is a portable profile for restoring upstream settings, trust keys, \
        and release package references. It does not include installed artifacts, \
        rollback data, or cache contents.\n\n\
        EXAMPLE:\n  \
        upstream export profile ./profile.json"
    )]
    Profile {
        /// Output path for the profile export
        path: std::path::PathBuf,
    },
}

#[derive(Subcommand)]
pub enum HooksAction {
    /// Install shell PATH hooks
    #[command(
        long_about = "Install upstream shell PATH hooks and create required local directories.\n\n\
        Hooks add upstream's managed bin directory to your shell PATH so installed \
        command-line packages can be run by name.\n\n\
        EXAMPLE:\n  \
        upstream hooks init"
    )]
    Init,

    /// Check shell PATH hooks
    #[command(
        long_about = "Check upstream shell PATH hooks and required local directories.\n\n\
        This is a read-only check. Use `upstream doctor --fix` for broader automatic \
        repairs.\n\n\
        EXAMPLE:\n  \
        upstream hooks check"
    )]
    Check,

    /// Remove shell PATH hooks
    #[command(
        long_about = "Remove upstream shell PATH hooks without deleting installed package data.\n\n\
        Installed files and metadata under upstream's data directory are preserved.\n\n\
        EXAMPLE:\n  \
        upstream hooks clean"
    )]
    Clean,

    /// Remove hooks and delete the local upstream data directory
    #[command(
        long_about = "Remove upstream shell PATH hooks and delete the local upstream data directory.\n\n\
        This deletes installed package files, metadata, rollback data, caches, config, and \
        trust keys under ~/.upstream. Pass global --yes to skip the confirmation prompt.\n\n\
        EXAMPLE:\n  \
        upstream --yes hooks purge"
    )]
    Purge,
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Set configuration values
    #[command(long_about = "Set one or more configuration values.\n\n\
        Use dot notation for nested keys. Multiple key=value pairs can be set at once. \
        Values are parsed as TOML literals where possible, so strings, booleans, and numbers \
        can be set without editing the file manually.\n\n\
        EXAMPLES:\n  \
        upstream config set github.api_token=ghp_xxx\n  \
        upstream config set download.low_threads=2\n  \
        upstream config set gitlab.api_token=glpat_xxx")]
    Set {
        /// Configuration assignments (format: key.path=value)
        #[arg(required = true)]
        keys: Vec<String>,
    },

    /// Get configuration values
    #[command(long_about = "Retrieve one or more configuration values.\n\n\
        Use dot notation to access nested keys. Missing keys are reported individually \
        when multiple keys are requested.\n\n\
        EXAMPLES:\n  \
        upstream config get download.high_threads\n  \
        upstream config get github.api_token gitlab.api_token")]
    Get {
        /// Configuration keys to retrieve (format: key.path)
        #[arg(required = true)]
        keys: Vec<String>,
    },

    /// List current configuration values
    #[command(long_about = "List current configuration values.\n\n\
        Values are flattened to dot-notation keys and shown through the configured pager \
        when output is long.\n\n\
        EXAMPLE:\n  \
        upstream config list")]
    List,

    /// Check config.toml for missing or unused keys
    #[command(long_about = "Check config.toml for missing or unused keys.\n\n\
        Missing supported keys are warnings because upstream will use built-in defaults. \
        Unused keys are failures because this version of upstream does not read them. \
        Run `upstream doctor --fix` to remove unused keys automatically.\n\n\
        EXAMPLE:\n  \
        upstream config verify")]
    Verify,

    /// Open config.toml in your default editor
    #[command(long_about = "Open config.toml in your default editor.\n\n\
        Uses EDITOR, then VISUAL, then a platform default. After the editor exits, \
        upstream reloads the config and reports whether it can still be parsed.\n\n\
        EXAMPLE:\n  \
        upstream config edit")]
    Edit,

    /// Reset config.toml to defaults
    #[command(long_about = "Reset config.toml to upstream defaults.\n\n\
        This replaces configured values after confirmation. Installed packages, trust keys, \
        rollback data, and caches are not removed.\n\n\
        EXAMPLE:\n  \
        upstream config reset")]
    Reset,
}

#[derive(Subcommand)]
pub enum PackageAction {
    /// Mark an installed package as pinned
    #[command(long_about = "Mark an installed package as pinned.\n\n\
        This updates package metadata so the package is skipped during \
        'upstream upgrade' operations.\n\n\
        EXAMPLE:\n  \
        upstream package pin nvim")]
    Pin {
        /// Name of package to pin
        name: String,
    },

    /// Clear the pinned flag on an installed package
    #[command(long_about = "Clear the pinned flag on an installed package.\n\n\
        This updates package metadata so the package can be included in future \
        upgrade operations.\n\n\
        EXAMPLE:\n  \
        upstream package unpin nvim")]
    Unpin {
        /// Name of package to unpin
        name: String,
    },

    /// Rename an installed package record and aliases
    #[command(long_about = "Rename an installed package record and aliases.\n\n\
        This changes the local package name stored by upstream and updates integration \
        aliases such as symlinks when possible. It does not reinstall the package.\n\n\
        EXAMPLE:\n  \
        upstream package rename nvim neovim")]
    Rename {
        /// Existing package alias
        old_name: String,

        /// New package alias
        new_name: String,
    },

    /// Add a desktop launcher entry for an installed package
    #[command(
        long_about = "Add a desktop launcher entry for an installed package.\n\n\
        This re-runs upstream's desktop entry creation flow, including AppImage \
        extraction, embedded .desktop metadata, icon lookup, launcher file writing, \
        and stored package metadata updates.\n\n\
        EXAMPLE:\n  \
        upstream package add-entry nvim"
    )]
    AddEntry {
        /// Installed package name
        name: String,
    },

    /// Remove an upstream-managed desktop launcher entry
    #[command(long_about = "Remove an upstream-managed desktop launcher entry.\n\n\
        This removes the launcher entry and any stored icon metadata owned by upstream. \
        It does not uninstall package files.\n\n\
        EXAMPLE:\n  \
        upstream package rm-entry nvim")]
    RmEntry {
        /// Installed package name
        name: String,
    },
}
