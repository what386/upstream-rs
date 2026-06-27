# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_upstream_global_optspecs
	string join \n y/yes h/help V/version
end

function __fish_upstream_needs_command
	# Figure out if the current invocation already has a command.
	set -l cmd (commandline -opc)
	set -e cmd[1]
	argparse -s (__fish_upstream_global_optspecs) -- $cmd 2>/dev/null
	or return
	if set -q argv[1]
		# Also print the command, so this can be used to figure out what it is.
		echo $argv[1]
		return 1
	end
	return 0
end

function __fish_upstream_using_subcommand
	set -l cmd (__fish_upstream_needs_command)
	test -z "$cmd"
	and return 1
	contains -- $cmd[1] $argv
end

complete -c upstream -n "__fish_upstream_needs_command" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_needs_command" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_needs_command" -s V -l version -d 'Print version'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "install" -d 'Install a release asset or direct download'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "build" -d 'Build and install a package from source'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "remove" -d 'Remove installed package files and metadata'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "uninstall" -d 'Remove installed package files and metadata'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "rollback" -d 'Restore or prune stored rollback artifacts'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "reinstall" -d 'Reinstall packages from their stored source metadata'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "upgrade" -d 'Check for or install package updates'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "list" -d 'List installed packages'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "info" -d 'Show details for one installed package'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "changelog" -d 'Show release notes for an installed package'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "docs" -d 'Search cached or fetched package README docs'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "probe" -d 'Inspect releases, choose an asset, and install it'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "search" -d 'Search provider repositories without installing'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "find" -d 'Search repositories interactively and install one'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "config" -d 'View, edit, and validate config.toml'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "package" -d 'Manage installed package records and launcher entries'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "hooks" -d 'Manage shell PATH hooks and local upstream data'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "import" -d 'Import config, trust keys, packages, or a profile'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "export" -d 'Export config, trust keys, packages, or a profile'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "doctor" -d 'Run diagnostics to detect installation and integration issues'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand install" -s t -l tag -d 'Release tag to install (defaults to latest matching the channel)' -r
complete -c upstream -n "__fish_upstream_using_subcommand install" -s k -l kind -d 'Asset kind to install' -r -f -a "app-image\t''
mac-app\t''
mac-dmg\t''
archive\t''
compressed\t''
binary\t''
win-exe\t''
checksum\t''
auto\t''"
complete -c upstream -n "__fish_upstream_using_subcommand install" -s p -l provider -d 'Source provider hosting the repository. Defaults to auto-detection' -r
complete -c upstream -n "__fish_upstream_using_subcommand install" -l base-url -d 'Custom base URL. Defaults to provider\'s root' -r
complete -c upstream -n "__fish_upstream_using_subcommand install" -s c -l channel -d 'Release channel to track for upgrades' -r -f -a "stable\t''
preview\t''
nightly\t''"
complete -c upstream -n "__fish_upstream_using_subcommand install" -s m -l match-pattern -d 'Match pattern to use as a hint for which asset to prefer' -r
complete -c upstream -n "__fish_upstream_using_subcommand install" -s e -l exclude-pattern -d 'Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")' -r
complete -c upstream -n "__fish_upstream_using_subcommand install" -l trust -d 'Trust verification mode for downloaded assets' -r -f -a "none\t''
best-effort\t''
checksum\t''
signature\t''
all\t''"
complete -c upstream -n "__fish_upstream_using_subcommand install" -s d -l desktop -d 'Create a desktop launcher entry for GUI applications'
complete -c upstream -n "__fish_upstream_using_subcommand install" -l dry-run -d 'Preview install resolution without downloading, installing, or writing metadata'
complete -c upstream -n "__fish_upstream_using_subcommand install" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand install" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand build" -s t -l tag -d 'Release tag to build (defaults to latest matching the channel)' -r
complete -c upstream -n "__fish_upstream_using_subcommand build" -l branch -d 'Branch to build from instead of a release tag' -r
complete -c upstream -n "__fish_upstream_using_subcommand build" -s p -l provider -d 'Source provider hosting the repository. Defaults to auto-detection' -r
complete -c upstream -n "__fish_upstream_using_subcommand build" -l base-url -d 'Custom base URL. Defaults to provider\'s root' -r
complete -c upstream -n "__fish_upstream_using_subcommand build" -s c -l channel -d 'Release channel to track for future builds' -r -f -a "stable\t''
preview\t''
nightly\t''"
complete -c upstream -n "__fish_upstream_using_subcommand build" -l build-profile -d 'Build profile used to compile/install from source (auto-detected when omitted)' -r -f -a "rust\t''
dotnet\t''
go\t''
zig\t''
cmake\t''"
complete -c upstream -n "__fish_upstream_using_subcommand build" -s d -l desktop -d 'Create a desktop launcher entry for GUI applications'
complete -c upstream -n "__fish_upstream_using_subcommand build" -l dry-run -d 'Preview build resolution without compiling, installing, or writing metadata'
complete -c upstream -n "__fish_upstream_using_subcommand build" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand build" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand remove" -l purge -d 'Remove package-owned cached data as well as active files'
complete -c upstream -n "__fish_upstream_using_subcommand remove" -l force -d 'Remove metadata even when uninstall cleanup fails'
complete -c upstream -n "__fish_upstream_using_subcommand remove" -l dry-run -d 'Preview removal actions without deleting files or metadata'
complete -c upstream -n "__fish_upstream_using_subcommand remove" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand remove" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand uninstall" -l purge -d 'Remove package-owned cached data as well as active files'
complete -c upstream -n "__fish_upstream_using_subcommand uninstall" -l force -d 'Remove metadata even when uninstall cleanup fails'
complete -c upstream -n "__fish_upstream_using_subcommand uninstall" -l dry-run -d 'Preview removal actions without deleting files or metadata'
complete -c upstream -n "__fish_upstream_using_subcommand uninstall" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand uninstall" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand rollback" -l prune -d 'Delete rollback artifacts for all packages or selected package names' -r
complete -c upstream -n "__fish_upstream_using_subcommand rollback" -l list -d 'List available rollback artifacts'
complete -c upstream -n "__fish_upstream_using_subcommand rollback" -l dry-run -d 'Preview rollback restore or prune actions without modifying files or metadata'
complete -c upstream -n "__fish_upstream_using_subcommand rollback" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand rollback" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand reinstall" -l trust -d 'Trust verification mode for release-asset reinstalls' -r -f -a "none\t''
best-effort\t''
checksum\t''
signature\t''
all\t''"
complete -c upstream -n "__fish_upstream_using_subcommand reinstall" -l force -d 'Continue reinstalling after uninstall cleanup errors'
complete -c upstream -n "__fish_upstream_using_subcommand reinstall" -l dry-run -d 'Preview reinstall resolution without removing, installing, or writing metadata'
complete -c upstream -n "__fish_upstream_using_subcommand reinstall" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand reinstall" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand upgrade" -l trust -d 'Trust verification mode for downloaded assets' -r -f -a "none\t''
best-effort\t''
checksum\t''
signature\t''
all\t''"
complete -c upstream -n "__fish_upstream_using_subcommand upgrade" -l force -d 'Reinstall even when the selected version is already installed'
complete -c upstream -n "__fish_upstream_using_subcommand upgrade" -l check -d 'Check for available upgrades without applying them'
complete -c upstream -n "__fish_upstream_using_subcommand upgrade" -l machine-readable -d 'Print one line per available update: "name oldver newver"'
complete -c upstream -n "__fish_upstream_using_subcommand upgrade" -l json -d 'Print check results as JSON'
complete -c upstream -n "__fish_upstream_using_subcommand upgrade" -l dry-run -d 'Preview upgrade resolution without downloading, installing, or writing metadata'
complete -c upstream -n "__fish_upstream_using_subcommand upgrade" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand upgrade" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand list" -l json -d 'Print package list as JSON'
complete -c upstream -n "__fish_upstream_using_subcommand list" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand list" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand info" -l json -d 'Print raw package metadata as JSON'
complete -c upstream -n "__fish_upstream_using_subcommand info" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand info" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand changelog" -l from -d 'Starting release tag, or "current"' -r
complete -c upstream -n "__fish_upstream_using_subcommand changelog" -l to -d 'Ending release tag, "current", or "latest"' -r
complete -c upstream -n "__fish_upstream_using_subcommand changelog" -l for -d 'Show release notes for exactly one release tag' -r
complete -c upstream -n "__fish_upstream_using_subcommand changelog" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand changelog" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand docs" -l fetch -d 'Refresh cached README docs for named packages, or all installed packages when empty' -r
complete -c upstream -n "__fish_upstream_using_subcommand docs" -l offline -d 'Use only the cached README and skip network fetching'
complete -c upstream -n "__fish_upstream_using_subcommand docs" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand docs" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand probe" -s p -l provider -d 'Source provider (defaults to GitHub, or scraper for plain URLs)' -r
complete -c upstream -n "__fish_upstream_using_subcommand probe" -l base-url -d 'Custom base URL for self-hosted providers' -r
complete -c upstream -n "__fish_upstream_using_subcommand probe" -s c -l channel -d 'Release channel to display and track' -r -f -a "stable\t''
preview\t''
nightly\t''"
complete -c upstream -n "__fish_upstream_using_subcommand probe" -l limit -d 'Number of releases to inspect instead of only one tag/latest release' -r
complete -c upstream -n "__fish_upstream_using_subcommand probe" -l tag -d 'Release tag to inspect exactly' -r
complete -c upstream -n "__fish_upstream_using_subcommand probe" -s k -l kind -d 'Asset kind to show and install' -r -f -a "app-image\t''
mac-app\t''
mac-dmg\t''
archive\t''
compressed\t''
binary\t''
win-exe\t''
checksum\t''
auto\t''"
complete -c upstream -n "__fish_upstream_using_subcommand probe" -l trust -d 'Trust verification mode for downloaded assets' -r -f -a "none\t''
best-effort\t''
checksum\t''
signature\t''
all\t''"
complete -c upstream -n "__fish_upstream_using_subcommand probe" -l verbose -d 'Show scored candidate assets and selection details'
complete -c upstream -n "__fish_upstream_using_subcommand probe" -l include-incompatible -d 'Include assets that do not match the current OS/architecture or selected file type'
complete -c upstream -n "__fish_upstream_using_subcommand probe" -l json -d 'Print probe results as JSON and exit'
complete -c upstream -n "__fish_upstream_using_subcommand probe" -s d -l desktop -d 'Create a desktop launcher entry for GUI applications'
complete -c upstream -n "__fish_upstream_using_subcommand probe" -l dry-run -d 'Show parsed releases without selecting, downloading, or installing'
complete -c upstream -n "__fish_upstream_using_subcommand probe" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand probe" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand search" -s p -l provider -d 'Source provider to search (defaults to GitHub)' -r
complete -c upstream -n "__fish_upstream_using_subcommand search" -l base-url -d 'Custom base URL for self-hosted providers' -r
complete -c upstream -n "__fish_upstream_using_subcommand search" -l limit -d 'Maximum number of results to display' -r
complete -c upstream -n "__fish_upstream_using_subcommand search" -l language -d 'Restrict results to repositories with this primary language' -r
complete -c upstream -n "__fish_upstream_using_subcommand search" -l topic -d 'Restrict results to repositories tagged with this topic' -r
complete -c upstream -n "__fish_upstream_using_subcommand search" -l min-stars -d 'Restrict results to repositories with at least this many stars' -r
complete -c upstream -n "__fish_upstream_using_subcommand search" -l max-stars -d 'Restrict results to repositories with at most this many stars' -r
complete -c upstream -n "__fish_upstream_using_subcommand search" -l pushed-after -d 'Restrict results to repositories pushed on or after YYYY-MM-DD' -r
complete -c upstream -n "__fish_upstream_using_subcommand search" -l include-forks -d 'Include forked repositories in provider search results'
complete -c upstream -n "__fish_upstream_using_subcommand search" -l include-archived -d 'Include archived repositories in provider search results'
complete -c upstream -n "__fish_upstream_using_subcommand search" -l json -d 'Print repository search results as JSON'
complete -c upstream -n "__fish_upstream_using_subcommand search" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand search" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand find" -s p -l provider -d 'Source provider to search (defaults to GitHub)' -r
complete -c upstream -n "__fish_upstream_using_subcommand find" -l base-url -d 'Custom base URL for self-hosted providers' -r
complete -c upstream -n "__fish_upstream_using_subcommand find" -l limit -d 'Maximum number of results to display' -r
complete -c upstream -n "__fish_upstream_using_subcommand find" -l language -d 'Restrict results to repositories with this primary language' -r
complete -c upstream -n "__fish_upstream_using_subcommand find" -l topic -d 'Restrict results to repositories tagged with this topic' -r
complete -c upstream -n "__fish_upstream_using_subcommand find" -l min-stars -d 'Restrict results to repositories with at least this many stars' -r
complete -c upstream -n "__fish_upstream_using_subcommand find" -l max-stars -d 'Restrict results to repositories with at most this many stars' -r
complete -c upstream -n "__fish_upstream_using_subcommand find" -l pushed-after -d 'Restrict results to repositories pushed on or after YYYY-MM-DD' -r
complete -c upstream -n "__fish_upstream_using_subcommand find" -l name -d 'Package name to register without prompting' -r
complete -c upstream -n "__fish_upstream_using_subcommand find" -s k -l kind -d 'Asset kind to install' -r -f -a "app-image\t''
mac-app\t''
mac-dmg\t''
archive\t''
compressed\t''
binary\t''
win-exe\t''
checksum\t''
auto\t''"
complete -c upstream -n "__fish_upstream_using_subcommand find" -s c -l channel -d 'Release channel to track for upgrades' -r -f -a "stable\t''
preview\t''
nightly\t''"
complete -c upstream -n "__fish_upstream_using_subcommand find" -s m -l match-pattern -d 'Match pattern to use as a hint for which asset to prefer' -r
complete -c upstream -n "__fish_upstream_using_subcommand find" -s e -l exclude-pattern -d 'Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")' -r
complete -c upstream -n "__fish_upstream_using_subcommand find" -l trust -d 'Trust verification mode for downloaded assets' -r -f -a "none\t''
best-effort\t''
checksum\t''
signature\t''
all\t''"
complete -c upstream -n "__fish_upstream_using_subcommand find" -l include-forks -d 'Include forked repositories in provider search results'
complete -c upstream -n "__fish_upstream_using_subcommand find" -l include-archived -d 'Include archived repositories in provider search results'
complete -c upstream -n "__fish_upstream_using_subcommand find" -s d -l desktop -d 'Create a desktop launcher entry for GUI applications'
complete -c upstream -n "__fish_upstream_using_subcommand find" -l dry-run -d 'Preview install resolution without downloading, installing, or writing metadata'
complete -c upstream -n "__fish_upstream_using_subcommand find" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand find" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list verify edit reset help" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list verify edit reset help" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list verify edit reset help" -f -a "set" -d 'Set configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list verify edit reset help" -f -a "get" -d 'Get configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list verify edit reset help" -f -a "list" -d 'List current configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list verify edit reset help" -f -a "verify" -d 'Check config.toml for missing or unused keys'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list verify edit reset help" -f -a "edit" -d 'Open config.toml in your default editor'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list verify edit reset help" -f -a "reset" -d 'Reset config.toml to defaults'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list verify edit reset help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from set" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from set" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from get" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from get" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from list" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from verify" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from verify" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from edit" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from edit" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from reset" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from reset" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "set" -d 'Set configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "get" -d 'Get configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "list" -d 'List current configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "verify" -d 'Check config.toml for missing or unused keys'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "edit" -d 'Open config.toml in your default editor'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "reset" -d 'Reset config.toml to defaults'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin rename add-entry rm-entry help" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin rename add-entry rm-entry help" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin rename add-entry rm-entry help" -f -a "pin" -d 'Mark an installed package as pinned'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin rename add-entry rm-entry help" -f -a "unpin" -d 'Clear the pinned flag on an installed package'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin rename add-entry rm-entry help" -f -a "rename" -d 'Rename an installed package record and aliases'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin rename add-entry rm-entry help" -f -a "add-entry" -d 'Add a desktop launcher entry for an installed package'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin rename add-entry rm-entry help" -f -a "rm-entry" -d 'Remove an upstream-managed desktop launcher entry'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin rename add-entry rm-entry help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from pin" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from pin" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from unpin" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from unpin" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from rename" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from rename" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from add-entry" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from add-entry" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from rm-entry" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from rm-entry" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from help" -f -a "pin" -d 'Mark an installed package as pinned'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from help" -f -a "unpin" -d 'Clear the pinned flag on an installed package'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from help" -f -a "rename" -d 'Rename an installed package record and aliases'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from help" -f -a "add-entry" -d 'Add a desktop launcher entry for an installed package'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from help" -f -a "rm-entry" -d 'Remove an upstream-managed desktop launcher entry'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and not __fish_seen_subcommand_from init check clean purge help" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and not __fish_seen_subcommand_from init check clean purge help" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and not __fish_seen_subcommand_from init check clean purge help" -f -a "init" -d 'Install shell PATH hooks'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and not __fish_seen_subcommand_from init check clean purge help" -f -a "check" -d 'Check shell PATH hooks'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and not __fish_seen_subcommand_from init check clean purge help" -f -a "clean" -d 'Remove shell PATH hooks'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and not __fish_seen_subcommand_from init check clean purge help" -f -a "purge" -d 'Remove hooks and delete the local upstream data directory'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and not __fish_seen_subcommand_from init check clean purge help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from init" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from init" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from check" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from check" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from clean" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from clean" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from purge" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from purge" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from help" -f -a "init" -d 'Install shell PATH hooks'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from help" -f -a "check" -d 'Check shell PATH hooks'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from help" -f -a "clean" -d 'Remove shell PATH hooks'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from help" -f -a "purge" -d 'Remove hooks and delete the local upstream data directory'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand import; and not __fish_seen_subcommand_from config keys packages profile help" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand import; and not __fish_seen_subcommand_from config keys packages profile help" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand import; and not __fish_seen_subcommand_from config keys packages profile help" -f -a "config" -d 'Replace config.toml from an export'
complete -c upstream -n "__fish_upstream_using_subcommand import; and not __fish_seen_subcommand_from config keys packages profile help" -f -a "keys" -d 'Import trusted minisign or cosign public keys'
complete -c upstream -n "__fish_upstream_using_subcommand import; and not __fish_seen_subcommand_from config keys packages profile help" -f -a "packages" -d 'Install packages from an exported package list'
complete -c upstream -n "__fish_upstream_using_subcommand import; and not __fish_seen_subcommand_from config keys packages profile help" -f -a "profile" -d 'Import config, keys, and packages from a profile'
complete -c upstream -n "__fish_upstream_using_subcommand import; and not __fish_seen_subcommand_from config keys packages profile help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand import; and __fish_seen_subcommand_from config" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand import; and __fish_seen_subcommand_from config" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand import; and __fish_seen_subcommand_from keys" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand import; and __fish_seen_subcommand_from keys" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand import; and __fish_seen_subcommand_from packages" -l skip-failed -d 'Continue installing remaining packages after a package import fails'
complete -c upstream -n "__fish_upstream_using_subcommand import; and __fish_seen_subcommand_from packages" -l latest -d 'Ignore exported version tags and install latest releases'
complete -c upstream -n "__fish_upstream_using_subcommand import; and __fish_seen_subcommand_from packages" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand import; and __fish_seen_subcommand_from packages" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand import; and __fish_seen_subcommand_from profile" -l skip-failed -d 'Continue installing remaining packages after a package import fails'
complete -c upstream -n "__fish_upstream_using_subcommand import; and __fish_seen_subcommand_from profile" -l latest -d 'Ignore exported package version tags and install latest releases'
complete -c upstream -n "__fish_upstream_using_subcommand import; and __fish_seen_subcommand_from profile" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand import; and __fish_seen_subcommand_from profile" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand import; and __fish_seen_subcommand_from help" -f -a "config" -d 'Replace config.toml from an export'
complete -c upstream -n "__fish_upstream_using_subcommand import; and __fish_seen_subcommand_from help" -f -a "keys" -d 'Import trusted minisign or cosign public keys'
complete -c upstream -n "__fish_upstream_using_subcommand import; and __fish_seen_subcommand_from help" -f -a "packages" -d 'Install packages from an exported package list'
complete -c upstream -n "__fish_upstream_using_subcommand import; and __fish_seen_subcommand_from help" -f -a "profile" -d 'Import config, keys, and packages from a profile'
complete -c upstream -n "__fish_upstream_using_subcommand import; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand export; and not __fish_seen_subcommand_from config keys packages profile help" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand export; and not __fish_seen_subcommand_from config keys packages profile help" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand export; and not __fish_seen_subcommand_from config keys packages profile help" -f -a "config" -d 'Export config.toml'
complete -c upstream -n "__fish_upstream_using_subcommand export; and not __fish_seen_subcommand_from config keys packages profile help" -f -a "keys" -d 'Export trusted minisign and cosign public keys'
complete -c upstream -n "__fish_upstream_using_subcommand export; and not __fish_seen_subcommand_from config keys packages profile help" -f -a "packages" -d 'Export installed release-package references'
complete -c upstream -n "__fish_upstream_using_subcommand export; and not __fish_seen_subcommand_from config keys packages profile help" -f -a "profile" -d 'Export config, trust keys, and package references'
complete -c upstream -n "__fish_upstream_using_subcommand export; and not __fish_seen_subcommand_from config keys packages profile help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand export; and __fish_seen_subcommand_from config" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand export; and __fish_seen_subcommand_from config" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand export; and __fish_seen_subcommand_from keys" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand export; and __fish_seen_subcommand_from keys" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand export; and __fish_seen_subcommand_from packages" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand export; and __fish_seen_subcommand_from packages" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand export; and __fish_seen_subcommand_from profile" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand export; and __fish_seen_subcommand_from profile" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand export; and __fish_seen_subcommand_from help" -f -a "config" -d 'Export config.toml'
complete -c upstream -n "__fish_upstream_using_subcommand export; and __fish_seen_subcommand_from help" -f -a "keys" -d 'Export trusted minisign and cosign public keys'
complete -c upstream -n "__fish_upstream_using_subcommand export; and __fish_seen_subcommand_from help" -f -a "packages" -d 'Export installed release-package references'
complete -c upstream -n "__fish_upstream_using_subcommand export; and __fish_seen_subcommand_from help" -f -a "profile" -d 'Export config, trust keys, and package references'
complete -c upstream -n "__fish_upstream_using_subcommand export; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand doctor" -l verbose -d 'Print each check result line in addition to summary output'
complete -c upstream -n "__fish_upstream_using_subcommand doctor" -l fix -d 'Attempt automatic repairs for detected issues'
complete -c upstream -n "__fish_upstream_using_subcommand doctor" -l migrate -d 'Migrate local upstream data after breaking layout or metadata changes'
complete -c upstream -n "__fish_upstream_using_subcommand doctor" -l json -d 'Print diagnostic report as JSON'
complete -c upstream -n "__fish_upstream_using_subcommand doctor" -s y -l yes -d 'Accept confirmation prompts automatically'
complete -c upstream -n "__fish_upstream_using_subcommand doctor" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "install" -d 'Install a release asset or direct download'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "build" -d 'Build and install a package from source'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "remove" -d 'Remove installed package files and metadata'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "rollback" -d 'Restore or prune stored rollback artifacts'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "reinstall" -d 'Reinstall packages from their stored source metadata'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "upgrade" -d 'Check for or install package updates'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "list" -d 'List installed packages'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "info" -d 'Show details for one installed package'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "changelog" -d 'Show release notes for an installed package'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "docs" -d 'Search cached or fetched package README docs'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "probe" -d 'Inspect releases, choose an asset, and install it'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "search" -d 'Search provider repositories without installing'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "find" -d 'Search repositories interactively and install one'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "config" -d 'View, edit, and validate config.toml'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "package" -d 'Manage installed package records and launcher entries'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "hooks" -d 'Manage shell PATH hooks and local upstream data'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "import" -d 'Import config, trust keys, packages, or a profile'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "export" -d 'Export config, trust keys, packages, or a profile'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "doctor" -d 'Run diagnostics to detect installation and integration issues'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list info changelog docs probe search find config package hooks import export doctor help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "set" -d 'Set configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "get" -d 'Get configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "list" -d 'List current configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "verify" -d 'Check config.toml for missing or unused keys'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "edit" -d 'Open config.toml in your default editor'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "reset" -d 'Reset config.toml to defaults'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from package" -f -a "pin" -d 'Mark an installed package as pinned'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from package" -f -a "unpin" -d 'Clear the pinned flag on an installed package'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from package" -f -a "rename" -d 'Rename an installed package record and aliases'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from package" -f -a "add-entry" -d 'Add a desktop launcher entry for an installed package'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from package" -f -a "rm-entry" -d 'Remove an upstream-managed desktop launcher entry'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from hooks" -f -a "init" -d 'Install shell PATH hooks'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from hooks" -f -a "check" -d 'Check shell PATH hooks'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from hooks" -f -a "clean" -d 'Remove shell PATH hooks'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from hooks" -f -a "purge" -d 'Remove hooks and delete the local upstream data directory'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from import" -f -a "config" -d 'Replace config.toml from an export'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from import" -f -a "keys" -d 'Import trusted minisign or cosign public keys'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from import" -f -a "packages" -d 'Install packages from an exported package list'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from import" -f -a "profile" -d 'Import config, keys, and packages from a profile'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from export" -f -a "config" -d 'Export config.toml'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from export" -f -a "keys" -d 'Export trusted minisign and cosign public keys'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from export" -f -a "packages" -d 'Export installed release-package references'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from export" -f -a "profile" -d 'Export config, trust keys, and package references'
