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

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ImportAs {
    Keys,
    Manifest,
    Snapshot,
}

#[derive(Parser)]
#[command(name = "upstream")]
#[command(about = "A package manager for everything else.")]
#[command(
    long_about = "Upstream is a lightweight package manager that installs and manages \
    applications from most software sources that do not have their own package manager.\n\n\
    Install binaries, AppImages, and other artifacts with automatic updates, \
    version pinning, rollback support, and minimal configuration.\n\n\
    EXAMPLES:\n  \
    upstream install neovim/neovim nvim --desktop\n  \
    upstream install BurntSushi/ripgrep        # name inferred as ripgrep\n  \
    upstream upgrade                # Upgrade all packages\n  \
    upstream list                   # Show installed packages\n  \
    upstream config set github.api_token=ghp_xxx"
)]
#[command(
    version,
    long_version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("GIT_HASH"), ")")
)]
pub struct Cli {
    /// Accept confirmation prompts
    #[arg(short = 'y', long, global = true, default_value_t = false)]
    pub yes: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Install a package from an upstream release source
    #[command(long_about = "Install a new package from a download source.\n\n\
        Downloads the specified file type from the latest release (or specified channel) \
        and registers it under the given name for future updates. If the name is omitted \
        for a git repository, upstream falls back to the repository name. \
        Direct HTTP sources may still require an explicit name.\n\n\
        EXAMPLES:\n  \
        upstream install BurntSushi/ripgrep rg -k binary\n  \
        upstream install bootandy/dust       # name inferred as dust\n  \
        upstream install sharkdp/bat bat")]
    Install {
        /// Repository identifier or URL
        repo_slug: String,

        /// Name to register the application under (falls back to git repository name when omitted)
        name: Option<String>,

        /// Version tag to install (defaults to latest)
        #[arg(short, long)]
        tag: Option<String>,

        /// File type to install
        #[arg(short, long, value_enum, default_value_t = Filetype::Auto)]
        kind: Filetype,

        /// Source provider hosting the repository. Defaults to auto-detection.
        #[arg(short = 'p', long)]
        provider: Option<Provider>,

        /// Custom base URL. Defaults to provider's root
        #[arg(long, requires = "provider")]
        base_url: Option<String>,

        /// Update channel to track
        #[arg(short, long, value_enum, default_value_t = Channel::Stable)]
        channel: Channel,

        /// Match pattern to use as a hint for which asset to prefer
        #[arg(short = 'm', long, name = "match")]
        match_pattern: Option<String>,

        /// Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")
        #[arg(short = 'e', long, name = "exclude")]
        exclude_pattern: Option<String>,

        /// Whether or not to create a .desktop entry for GUI applications
        #[arg(short, long, default_value_t = false)]
        desktop: bool,

        /// Trust verification mode for downloaded assets
        #[arg(long = "trust", value_enum, default_value_t = TrustMode::BestEffort)]
        trust_mode: TrustMode,

        /// Preview install resolution without downloading or writing files
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Build and install from source for release tags without artifacts
    #[command(long_about = "Build and install a package from source.\n\n\
        Use this command when release tags exist but prebuilt artifacts are missing \
        or unsuitable for your system.\n\n\
        Mirrors install-style source resolution, with optional automatic profile detection. \
        Git workspaces are cached under upstream's cache directory so upgrades can reuse \
        prior build outputs when the project build system supports incremental rebuilds. \
        If the name is omitted for a git repository, upstream falls back to the repository name.\n\n\
        EXAMPLES:\n  \
        upstream build BurntSushi/ripgrep rg\n  \
        upstream build BurntSushi/ripgrep       # name inferred as ripgrep\n  \
        upstream build BurntSushi/ripgrep rg --branch main\n  \
        upstream build BurntSushi/ripgrep rg --build-profile rust\n  \
        upstream build owner/repo app --build-profile dotnet --tag v1.2.3")]
    Build {
        /// Repository identifier or URL
        repo_slug: String,

        /// Name to register the application under (falls back to git repository name when omitted)
        name: Option<String>,

        /// Version tag to build (defaults to latest)
        #[arg(short, long, conflicts_with = "branch")]
        tag: Option<String>,

        /// Branch name to build from (uses latest commit from that branch)
        #[arg(long, conflicts_with = "tag")]
        branch: Option<String>,

        /// Source provider hosting the repository. Defaults to auto-detection.
        #[arg(short = 'p', long)]
        provider: Option<Provider>,

        /// Custom base URL. Defaults to provider's root
        #[arg(long, requires = "provider")]
        base_url: Option<String>,

        /// Update channel to track
        #[arg(short, long, value_enum, default_value_t = Channel::Stable)]
        channel: Channel,

        /// Whether or not to create a .desktop entry for GUI applications
        #[arg(short, long, default_value_t = false)]
        desktop: bool,

        /// Build profile used to compile/install from source (auto-detected when omitted)
        #[arg(long, value_enum)]
        build_profile: Option<BuildProfile>,

        /// Preview build resolution without compiling or writing files
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Remove one or more installed packages
    #[command(
        long_about = "Uninstall packages and optionally remove cached data.\n\n\
        By default, removes the package binary/files but preserves cached release data. \
        Use --purge to remove everything. Use --force to ignore uninstall errors \
        (for example, missing files) and still remove package metadata.\n\n\
        EXAMPLES:\n  \
        upstream remove nvim\n  \
        upstream remove rg fd bat --purge\n  \
        upstream remove rg --force\n  \
        upstream remove rg --dry-run"
    )]
    Remove {
        /// Names of packages to remove
        names: Vec<String>,

        /// Remove all associated cached data
        #[arg(long, default_value_t = false)]
        purge: bool,

        /// Ignore uninstall errors and remove metadata anyway
        #[arg(long, default_value_t = false)]
        force: bool,

        /// Preview removal actions without deleting files or metadata
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Manage stored rollback artifacts
    #[command(long_about = "Manage package rollback points.\n\n\
        Use restore without package names to restore the latest reversible transaction \
        recorded in upstream's transaction history. Use explicit package names to restore \
        selected packages, list rollback artifacts, or prune stored rollback data.\n\n\
        EXAMPLES:\n  \
        upstream rollback restore\n  \
        upstream rollback restore rg\n  \
        upstream rollback restore rg fd --dry-run\n  \
        upstream rollback list\n  \
        upstream rollback prune\n  \
        upstream rollback prune rg")]
    Rollback {
        #[command(subcommand)]
        action: RollbackAction,
    },

    /// Reinstall one or more packages (remove then install)
    #[command(
        long_about = "Reinstall packages by uninstalling and then installing them again.\n\n\
        Reinstall uses each package's stored source metadata. Release installs attempt \
        the currently recorded version tag; build installs rebuild from source.\n\n\
        EXAMPLES:\n  \
        upstream reinstall nvim\n  \
        upstream reinstall rg fd\n  \
        upstream reinstall rg --force\n  \
        upstream reinstall rg --trust none"
    )]
    Reinstall {
        /// Names of packages to reinstall
        names: Vec<String>,

        /// Trust verification mode for release-asset reinstalls
        #[arg(long = "trust", value_enum, default_value_t = TrustMode::BestEffort)]
        trust_mode: TrustMode,

        /// Ignore uninstall errors and remove metadata anyway before reinstalling
        #[arg(long, default_value_t = false)]
        force: bool,

        /// Preview reinstall resolution without removing, building, or writing files
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Upgrade installed packages to their latest versions
    #[command(long_about = "Check for and install updates to packages.\n\n\
        Without arguments, upgrades all packages. Specify package names to upgrade \
        only those packages. Use --check to preview available updates. At the \
        confirmation prompt, enter c to view release notes before deciding.\n\n\
        EXAMPLES:\n  \
        upstream upgrade              # Upgrade all\n  \
        upstream upgrade nvim rg      # Upgrade specific packages\n  \
        upstream upgrade --check      # Check for updates\n  \
        upstream upgrade --check --json # Check for updates as JSON\n  \
        upstream upgrade --check --machine-readable # Script-friendly output\n  \
        upstream upgrade nvim --force # Force reinstall\n  \
        upstream upgrade --trust none")]
    Upgrade {
        /// Packages to upgrade (upgrades all if omitted)
        names: Option<Vec<String>>,

        /// Force upgrade even if already up to date
        #[arg(long, default_value_t = false)]
        force: bool,

        /// Check for available upgrades without applying them
        #[arg(long, default_value_t = false)]
        check: bool,

        /// Use script-friendly check output: one line per update, "name oldver newver"
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

        /// Preview upgrade resolution without downloading or writing files
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// List installed packages and their metadata
    #[command(long_about = "Display information about installed packages.\n\n\
        Without arguments, shows a summary of all installed packages. \
        Provide a package name to see detailed information.\n\n\
        EXAMPLES:\n  \
        upstream list       # List all packages\n  \
        upstream list nvim  # Show details for nvim")]
    List {
        /// Package name for detailed information
        name: Option<String>,

        /// Print raw package metadata as JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Show upstream release notes for an installed package
    #[command(long_about = "Show release notes for an installed package.\n\n\
        By default, prints release bodies newer than the installed version up to \
        the latest release for the package's tracked channel. Use --from and --to \
        to override the range endpoints by release tag, or use current/latest for \
        the installed or tracked latest release.\n\n\
        EXAMPLES:\n  \
        upstream changelog nvim\n  \
        upstream changelog nvim --from current --to latest\n  \
        upstream changelog nvim --from v0.10.0\n  \
        upstream changelog nvim --from v0.10.0 --to v0.11.0")]
    Changelog {
        /// Installed package name
        name: String,

        /// Override the starting release tag
        #[arg(long = "from")]
        from_tag: Option<String>,

        /// Override the ending release tag
        #[arg(long = "to")]
        to_tag: Option<String>,
    },

    /// Search package README documentation
    #[command(long_about = "Search package README documentation.\n\n\
        Fetches the installed package's upstream README, caches it for offline use, \
        parses Markdown sections, and opens ranked keyword matches in an interactive \
        picker with a live preview. If fetching fails and a cached README exists, \
        upstream falls back to the cached copy. Use --offline to skip fetching and \
        search only cached documentation. If glow is installed, Markdown previews \
        and selected sections are rendered with glow's terminal styling.\n\n\
        EXAMPLES:\n  \
        upstream docs rg usage\n  \
        upstream docs rg --offline usage\n  \
        upstream docs ripgrep configuration file\n  \
        upstream docs bat themes syntax")]
    Docs {
        /// Installed package name
        name: String,

        /// Use only the cached README and skip network fetching
        #[arg(long, default_value_t = false)]
        offline: bool,

        /// Search keywords (joined with spaces)
        #[arg(required = true, num_args(1..), value_delimiter = ' ')]
        keywords: Vec<String>,
    },

    /// Probe a repository/source, choose an asset, and install it
    #[command(
        long_about = "Probe a repository/source, choose a release asset interactively, \
        and install it. Use --dry-run to show parsed releases without installing.\n\n\
        EXAMPLES:\n  \
        upstream probe neovim/neovim\n  \
        upstream probe https://ziglang.org/download/ -p scraper --limit 20\n  \
        upstream probe owner/repo tool --desktop\n  \
        upstream probe owner/repo --include-incompatible\n  \
        upstream probe owner/repo --limit 20\n  \
        upstream probe owner/repo --tag v1.2.3\n  \
        upstream probe owner/repo -k archive\n  \
        upstream probe owner/repo --channel nightly --verbose\n  \
        upstream probe owner/repo --dry-run\n  \
        upstream probe owner/repo --json"
    )]
    Probe {
        /// Repository identifier or URL to probe
        repo_slug: String,

        /// Name to register the application under (prompts with inferred default when omitted)
        name: Option<String>,

        /// Source provider (defaults to github, or scraper for URLs)
        #[arg(short = 'p', long)]
        provider: Option<Provider>,

        /// Custom base URL for self-hosted providers
        #[arg(long)]
        base_url: Option<String>,

        /// Channel view to display
        #[arg(short, long, value_enum, default_value_t = Channel::Stable)]
        channel: Channel,

        /// Number of releases to inspect instead of only the latest release
        #[arg(long)]
        limit: Option<u32>,

        /// Release tag to inspect (defaults to latest when --limit is omitted)
        #[arg(long, value_name = "TAG")]
        tag: Option<String>,

        /// File type to show and install
        #[arg(short, long, value_enum, default_value_t = Filetype::Auto)]
        kind: Filetype,

        /// Include scored candidate assets for each release
        #[arg(long, default_value_t = false)]
        verbose: bool,

        /// Include assets that do not match the current OS/architecture or selected file type
        #[arg(long, default_value_t = false)]
        include_incompatible: bool,

        /// Print dry-run probe results as JSON
        #[arg(long, default_value_t = false)]
        json: bool,

        /// Whether or not to create a .desktop entry for GUI applications
        #[arg(short, long, default_value_t = false)]
        desktop: bool,

        /// Trust verification mode for downloaded assets
        #[arg(long = "trust", value_enum, default_value_t = TrustMode::BestEffort)]
        trust_mode: TrustMode,

        /// Show parsed releases without selecting, downloading, or installing
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Search provider repositories by keyword(s)
    #[command(long_about = "Search for repositories on a provider.\n\n\
        Defaults to GitHub when provider is omitted.\n\n\
        EXAMPLES:\n  \
        upstream search\n  \
        upstream search ripgrep\n  \
        upstream search editor --language Rust --min-stars 100 --max-stars 50000\n  \
        upstream search rip grep --limit 5\n  \
        upstream search my tool -p github\n  \
        upstream search cli --topic terminal\n  \
        upstream search ripgrep --json")]
    Search {
        /// Optional query words (joined with spaces)
        #[arg(num_args(0..), value_delimiter = ' ')]
        query_words: Vec<String>,

        /// Source provider to search (defaults to github)
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

        /// Print search results as JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Search repositories interactively and install a selected result
    #[command(
        long_about = "Search for repositories on a provider, choose a result interactively, \
        and install the selected repository.\n\n\
        Defaults to GitHub when provider is omitted. After selection, prompts for the package \
        name with the selected repository name as the default; submitting an empty name uses \
        that inferred default. Use --name to skip the prompt.\n\n\
        EXAMPLES:\n  \
        upstream find ripgrep\n  \
        upstream find terminal emulator --limit 20\n  \
        upstream find cli --language Rust --topic cli\n  \
        upstream find ripgrep --name rg -k binary\n  \
        upstream find app -p github --desktop --trust none"
    )]
    Find {
        /// Query words (joined with spaces)
        #[arg(required = true, num_args(1..), value_delimiter = ' ')]
        query_words: Vec<String>,

        /// Source provider to search (defaults to github)
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

        /// File type to install
        #[arg(short, long, value_enum, default_value_t = Filetype::Auto)]
        kind: Filetype,

        /// Update channel to track
        #[arg(short, long, value_enum, default_value_t = Channel::Stable)]
        channel: Channel,

        /// Match pattern to use as a hint for which asset to prefer
        #[arg(short = 'm', long, name = "match")]
        match_pattern: Option<String>,

        /// Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")
        #[arg(short = 'e', long, name = "exclude")]
        exclude_pattern: Option<String>,

        /// Whether or not to create a .desktop entry for GUI applications
        #[arg(short, long, default_value_t = false)]
        desktop: bool,

        /// Trust verification mode for downloaded assets
        #[arg(long = "trust", value_enum, default_value_t = TrustMode::BestEffort)]
        trust_mode: TrustMode,

        /// Preview install resolution without downloading or writing files
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Manage upstream configuration
    #[command(long_about = "View and modify upstream's configuration.\n\n\
        Configuration is stored in TOML format and includes settings like \
        API tokens, default providers, and installation preferences.\n\n\
        EXAMPLES:\n  \
        upstream config set github.api_token=ghp_xxx\n  \
        upstream config get download.high_threads\n  \
        upstream config list\n  \
        upstream config edit")]
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Manage package-specific behavior
    #[command(long_about = "Control package behavior.\n\n\
        Pin packages to prevent upgrades or rename installed package aliases.\n\n\
        EXAMPLES:\n  \
        upstream package pin nvim\n  \
        upstream package unpin nvim\n  \
        upstream package rename nvim neovim")]
    Package {
        #[command(subcommand)]
        action: PackageAction,
    },

    /// Manage shell integration hooks and local upstream data
    #[command(long_about = "Manage upstream shell integration hooks.\n\n\
        Use these commands to add, verify, or remove shell PATH hooks. \
        Purge removes shell hooks and deletes the local upstream data directory.\n\n\
        EXAMPLES:\n  \
        upstream hooks init\n  \
        upstream hooks check\n  \
        upstream hooks clean\n  \
        upstream --yes hooks purge")]
    Hooks {
        #[command(subcommand)]
        action: HooksAction,
    },

    /// Import trusted keys, package metadata manifests, or full snapshots
    #[command(
        long_about = "Import trusted keys, package metadata manifests, or full snapshots.\n\n\
        Autodetects the input type by content/extension, prompts for confirmation by default, \
        and then performs the selected import operation.\n\n\
        EXAMPLES:\n  \
        upstream import ./minisign.pub            # Import trusted minisign keys\n  \
        upstream import ./cosign.pub              # Import trusted cosign PEM keys\n  \
        upstream import ./packages.json           # Import package metadata manifest\n  \
        upstream import ./backup.tar.gz           # Restore full snapshot\n  \
        upstream --yes import ./input.bin --as keys"
    )]
    Import {
        /// Path to a keys file, metadata manifest, or snapshot archive
        path: std::path::PathBuf,

        /// Continue importing remaining entries when metadata manifest processing fails
        #[arg(long, default_value_t = false)]
        skip_failed: bool,

        /// Force the input type instead of autodetection
        #[arg(long = "as", value_enum)]
        import_as: Option<ImportAs>,
    },

    /// Export packages to a manifest or full snapshot
    #[command(long_about = "Export installed packages for backup or transfer.\n\n\
        By default, writes a lightweight manifest containing just enough info to \
        reinstall each package. Use --full to instead create a tarball of the entire \
        upstream directory (a full snapshot).\n\n\
        EXAMPLES:\n  \
        upstream export ./packages.json           # Export manifest\n  \
        upstream export ./backup.tar.gz --full    # Full snapshot")]
    Export {
        /// Output path for the manifest or snapshot archive
        path: std::path::PathBuf,
        /// Export a full snapshot of the upstream directory instead of a manifest
        #[arg(long, default_value_t = false)]
        full: bool,
    },

    /// Migrate local upstream data after breaking changes
    #[command(
        long_about = "Migrate existing upstream data to the current application format.\n\n\
        Use this after upgrading across breaking changes that affect local data, \
        metadata, package paths, or integration files. The migration is designed to \
        be run manually when release notes or diagnostics ask for it. The current \
        migration moves legacy package directories into the packages layout and rewrites \
        affected metadata paths.\n\n\
        EXAMPLE:\n  \
        upstream migrate"
    )]
    Migrate,

    /// Run diagnostics to detect installation and integration issues
    #[command(
        long_about = "Inspect upstream installation health and package state.\n\n\
        Checks package paths, symlinks, shell PATH integration, cached completions, \
        desktop/icon files, and metadata. \
        Reports a compact summary by default and includes actionable hints. \
        Use --verbose to print each individual check result. Use --fix to repair \
        supported issues such as PATH hooks, missing symlinks, executable bits, \
        executable metadata, and cached completion drift.\n\n\
        EXAMPLES:\n  \
        upstream doctor\n  \
        upstream doctor --verbose\n  \
        upstream doctor --fix\n  \
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

        /// Print diagnostic report as JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

fn parse_search_date(raw: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .map_err(|_| format!("expected date in YYYY-MM-DD format, got '{raw}'"))
}

#[derive(Subcommand)]
pub enum RollbackAction {
    /// Restore rollback artifacts
    #[command(long_about = "Restore stored rollback artifacts.\n\n\
        Without package names, restores the latest reversible transaction recorded \
        in upstream's transaction history.\n\n\
        EXAMPLES:\n  \
        upstream rollback restore\n  \
        upstream rollback restore rg\n  \
        upstream rollback restore rg fd\n  \
        upstream rollback restore rg --dry-run")]
    Restore {
        /// Package names to restore (latest reversible transaction if omitted)
        #[arg(num_args(0..))]
        names: Vec<String>,

        /// Preview rollback restore actions without modifying files or metadata
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Prune stored rollback artifacts
    #[command(long_about = "Delete stored rollback artifacts.\n\n\
        Without package names, prunes all stored rollback artifacts.\n\n\
        EXAMPLES:\n  \
        upstream rollback prune\n  \
        upstream rollback prune rg\n  \
        upstream rollback prune rg fd --dry-run")]
    Prune {
        /// Package names to prune (all rollback artifacts if omitted)
        names: Vec<String>,

        /// Preview rollback prune actions without deleting artifacts or metadata
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// List stored rollback artifacts
    #[command(long_about = "List packages with stored rollback artifacts.\n\n\
        EXAMPLE:\n  \
        upstream rollback list")]
    List,
}

impl Commands {
    pub fn requires_lock(&self) -> bool {
        match self {
            Commands::List { .. } => false,
            Commands::Changelog { .. } => false,
            Commands::Docs { .. } => false,
            Commands::Doctor { fix, .. } => *fix,
            Commands::Search { .. } => false,
            Commands::Find { .. } => true,
            Commands::Rollback {
                action: RollbackAction::List,
            } => false,
            Commands::Hooks { action } => !matches!(action, HooksAction::Check),
            Commands::Package { .. } => true,
            Commands::Config { action } => {
                !matches!(action, ConfigAction::Get { .. } | ConfigAction::List)
            }
            Commands::Install { .. }
            | Commands::Build { .. }
            | Commands::Remove { .. }
            | Commands::Rollback { .. }
            | Commands::Reinstall { .. }
            | Commands::Upgrade { .. }
            | Commands::Probe { .. }
            | Commands::Import { .. }
            | Commands::Export { .. }
            | Commands::Migrate => true,
        }
    }
}

#[derive(Subcommand)]
pub enum HooksAction {
    /// Add upstream shell integration hooks
    #[command(
        long_about = "Add upstream shell integration hooks and create required local directories.\n\n\
        EXAMPLE:\n  \
        upstream hooks init"
    )]
    Init,

    /// Check upstream shell integration hooks
    #[command(
        long_about = "Check upstream shell integration hooks and required local directories.\n\n\
        EXAMPLE:\n  \
        upstream hooks check"
    )]
    Check,

    /// Remove upstream shell integration hooks
    #[command(
        long_about = "Remove upstream shell integration hooks without deleting installed package data.\n\n\
        EXAMPLE:\n  \
        upstream hooks clean"
    )]
    Clean,

    /// Remove hooks and delete the local upstream data directory
    #[command(
        long_about = "Remove upstream shell integration hooks and delete the local upstream data directory.\n\n\
        This deletes installed package files and metadata under ~/.upstream. \
        Pass global --yes to skip the confirmation prompt.\n\n\
        EXAMPLE:\n  \
        upstream --yes hooks purge"
    )]
    Purge,
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Set configuration values
    #[command(long_about = "Set one or more configuration values.\n\n\
        Use dot notation for nested keys. Multiple key=value pairs can be set at once.\n\n\
        EXAMPLES:\n  \
        upstream config set github.api_token=ghp_xxx\n  \
        upstream config set gitlab.api_token=glpat_xxx")]
    Set {
        /// Configuration assignments (format: key.path=value)
        #[arg(required = true)]
        keys: Vec<String>,
    },

    /// Get configuration values
    #[command(long_about = "Retrieve one or more configuration values.\n\n\
        Use dot notation to access nested keys.\n\n\
        EXAMPLES:\n  \
        upstream config get download.high_threads\n  \
        upstream config get github.api_token gitlab.api_token")]
    Get {
        /// Configuration keys to retrieve (format: key.path)
        #[arg(required = true)]
        keys: Vec<String>,
    },

    /// List all configuration keys
    List,

    /// Open configuration file in your default editor
    Edit,

    /// Reset configuration to defaults
    Reset,
}

#[derive(Subcommand)]
pub enum PackageAction {
    /// Pin a package to its current version
    #[command(long_about = "Prevent a package from being upgraded.\n\n\
        Pinned packages are skipped during 'upstream upgrade' operations.\n\n\
        EXAMPLE:\n  \
        upstream package pin nvim")]
    Pin {
        /// Name of package to pin
        name: String,

        /// Optional reason for pinning this package
        #[arg(long)]
        reason: Option<String>,
    },

    /// Unpin a package to allow updates
    #[command(long_about = "Remove version pin from a package.\n\n\
        Unpinned packages will be included in future upgrade operations.\n\n\
        EXAMPLE:\n  \
        upstream package unpin nvim")]
    Unpin {
        /// Name of package to unpin
        name: String,
    },

    /// Rename package alias without reinstalling
    #[command(long_about = "Rename the local alias of an installed package.\n\n\
        This changes how upstream tracks the package and updates integration aliases \
        (symlink/desktop entry) when possible.\n\n\
        EXAMPLE:\n  \
        upstream package rename nvim neovim")]
    Rename {
        /// Existing package alias
        old_name: String,

        /// New package alias
        new_name: String,
    },
}
