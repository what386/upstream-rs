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

complete -c upstream -n "__fish_upstream_needs_command" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_needs_command" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_needs_command" -s V -l version -d 'Print version'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "install" -d 'Install a package from an upstream release source'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "build" -d 'Build and install from source for release tags without artifacts'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "remove" -d 'Remove one or more installed packages'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "rollback" -d 'Restore or prune stored rollback artifacts'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "reinstall" -d 'Reinstall one or more packages (remove then install)'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "upgrade" -d 'Upgrade installed packages to their latest versions'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "list" -d 'List installed packages and their metadata'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "changelog" -d 'Show upstream release notes for an installed package'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "probe" -d 'Inspect releases visible from a provider without installing'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "search" -d 'Search provider repositories by keyword(s)'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "config" -d 'Manage upstream configuration'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "package" -d 'Manage package-specific behavior'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "hooks" -d 'Manage shell integration hooks and local upstream data'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "import" -d 'Import trusted keys, package metadata manifests, or full snapshots'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "export" -d 'Export packages to a manifest or full snapshot'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "doctor" -d 'Run diagnostics to detect installation and integration issues'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand install" -s t -l tag -d 'Version tag to install (defaults to latest)' -r
complete -c upstream -n "__fish_upstream_using_subcommand install" -s k -l kind -d 'File type to install' -r -f -a "app-image\t''
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
complete -c upstream -n "__fish_upstream_using_subcommand install" -s c -l channel -d 'Update channel to track' -r -f -a "stable\t''
preview\t''
nightly\t''"
complete -c upstream -n "__fish_upstream_using_subcommand install" -s m -l match-pattern -d 'Match pattern to use as a hint for which asset to prefer' -r
complete -c upstream -n "__fish_upstream_using_subcommand install" -s e -l exclude-pattern -d 'Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")' -r
complete -c upstream -n "__fish_upstream_using_subcommand install" -l trust -d 'Trust verification mode for downloaded assets' -r -f -a "none\t''
best-effort\t''
checksum\t''
signature\t''
all\t''"
complete -c upstream -n "__fish_upstream_using_subcommand install" -s d -l desktop -d 'Whether or not to create a .desktop entry for GUI applications'
complete -c upstream -n "__fish_upstream_using_subcommand install" -l dry-run -d 'Preview install resolution without downloading or writing files'
complete -c upstream -n "__fish_upstream_using_subcommand install" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand install" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand build" -s t -l tag -d 'Version tag to build (defaults to latest)' -r
complete -c upstream -n "__fish_upstream_using_subcommand build" -l branch -d 'Branch name to build from (uses latest commit from that branch)' -r
complete -c upstream -n "__fish_upstream_using_subcommand build" -s p -l provider -d 'Source provider hosting the repository. Defaults to auto-detection' -r
complete -c upstream -n "__fish_upstream_using_subcommand build" -l base-url -d 'Custom base URL. Defaults to provider\'s root' -r
complete -c upstream -n "__fish_upstream_using_subcommand build" -s c -l channel -d 'Update channel to track' -r -f -a "stable\t''
preview\t''
nightly\t''"
complete -c upstream -n "__fish_upstream_using_subcommand build" -l build-profile -d 'Build profile used to compile/install from source (auto-detected when omitted)' -r -f -a "rust\t''
dotnet\t''
go\t''
zig\t''
cmake\t''"
complete -c upstream -n "__fish_upstream_using_subcommand build" -s d -l desktop -d 'Whether or not to create a .desktop entry for GUI applications'
complete -c upstream -n "__fish_upstream_using_subcommand build" -l dry-run -d 'Preview build resolution without compiling or writing files'
complete -c upstream -n "__fish_upstream_using_subcommand build" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand build" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand remove" -l purge -d 'Remove all associated cached data'
complete -c upstream -n "__fish_upstream_using_subcommand remove" -l force -d 'Ignore uninstall errors and remove metadata anyway'
complete -c upstream -n "__fish_upstream_using_subcommand remove" -l dry-run -d 'Preview removal actions without deleting files or metadata'
complete -c upstream -n "__fish_upstream_using_subcommand remove" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand remove" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand rollback" -l prune -d 'Prune rollback artifacts instead of restoring'
complete -c upstream -n "__fish_upstream_using_subcommand rollback" -l dry-run -d 'Preview rollback/prune actions without modifying files or metadata'
complete -c upstream -n "__fish_upstream_using_subcommand rollback" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand rollback" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand reinstall" -l trust -d 'Trust verification mode for release-asset reinstalls' -r -f -a "none\t''
best-effort\t''
checksum\t''
signature\t''
all\t''"
complete -c upstream -n "__fish_upstream_using_subcommand reinstall" -l force -d 'Ignore uninstall errors and remove metadata anyway before reinstalling'
complete -c upstream -n "__fish_upstream_using_subcommand reinstall" -l dry-run -d 'Preview reinstall resolution without removing, building, or writing files'
complete -c upstream -n "__fish_upstream_using_subcommand reinstall" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand reinstall" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand upgrade" -l trust -d 'Trust verification mode for downloaded assets' -r -f -a "none\t''
best-effort\t''
checksum\t''
signature\t''
all\t''"
complete -c upstream -n "__fish_upstream_using_subcommand upgrade" -l force -d 'Force upgrade even if already up to date'
complete -c upstream -n "__fish_upstream_using_subcommand upgrade" -l check -d 'Check for available upgrades without applying them'
complete -c upstream -n "__fish_upstream_using_subcommand upgrade" -l machine-readable -d 'Use script-friendly check output: one line per update, "name oldver newver"'
complete -c upstream -n "__fish_upstream_using_subcommand upgrade" -l dry-run -d 'Preview upgrade resolution without downloading or writing files'
complete -c upstream -n "__fish_upstream_using_subcommand upgrade" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand upgrade" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand list" -l json -d 'Print raw package metadata as JSON'
complete -c upstream -n "__fish_upstream_using_subcommand list" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand list" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand changelog" -l from -d 'Override the starting release tag' -r
complete -c upstream -n "__fish_upstream_using_subcommand changelog" -l to -d 'Override the ending release tag' -r
complete -c upstream -n "__fish_upstream_using_subcommand changelog" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand changelog" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand probe" -s p -l provider -d 'Source provider (defaults to github, or scraper for URLs)' -r
complete -c upstream -n "__fish_upstream_using_subcommand probe" -l base-url -d 'Custom base URL for self-hosted providers' -r
complete -c upstream -n "__fish_upstream_using_subcommand probe" -s c -l channel -d 'Channel view to display' -r -f -a "stable\t''
preview\t''
nightly\t''"
complete -c upstream -n "__fish_upstream_using_subcommand probe" -l limit -d 'Maximum number of releases to display' -r
complete -c upstream -n "__fish_upstream_using_subcommand probe" -l verbose -d 'Include scored candidate assets for each release'
complete -c upstream -n "__fish_upstream_using_subcommand probe" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand probe" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand search" -s p -l provider -d 'Source provider to search (defaults to github)' -r
complete -c upstream -n "__fish_upstream_using_subcommand search" -l base-url -d 'Custom base URL for self-hosted providers' -r
complete -c upstream -n "__fish_upstream_using_subcommand search" -l limit -d 'Maximum number of results to display' -r
complete -c upstream -n "__fish_upstream_using_subcommand search" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand search" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list edit reset help" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list edit reset help" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list edit reset help" -f -a "set" -d 'Set configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list edit reset help" -f -a "get" -d 'Get configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list edit reset help" -f -a "list" -d 'List all configuration keys'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list edit reset help" -f -a "edit" -d 'Open configuration file in your default editor'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list edit reset help" -f -a "reset" -d 'Reset configuration to defaults'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list edit reset help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from set" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from set" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from get" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from get" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from list" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from edit" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from edit" -s h -l help -d 'Print help'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from reset" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from reset" -s h -l help -d 'Print help'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "set" -d 'Set configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "get" -d 'Get configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "list" -d 'List all configuration keys'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "edit" -d 'Open configuration file in your default editor'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "reset" -d 'Reset configuration to defaults'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin rename help" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin rename help" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin rename help" -f -a "pin" -d 'Pin a package to its current version'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin rename help" -f -a "unpin" -d 'Unpin a package to allow updates'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin rename help" -f -a "rename" -d 'Rename package alias without reinstalling'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin rename help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from pin" -l reason -d 'Optional reason for pinning this package' -r
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from pin" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from pin" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from unpin" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from unpin" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from rename" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from rename" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from help" -f -a "pin" -d 'Pin a package to its current version'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from help" -f -a "unpin" -d 'Unpin a package to allow updates'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from help" -f -a "rename" -d 'Rename package alias without reinstalling'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and not __fish_seen_subcommand_from init check clean purge help" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and not __fish_seen_subcommand_from init check clean purge help" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and not __fish_seen_subcommand_from init check clean purge help" -f -a "init" -d 'Add upstream shell integration hooks'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and not __fish_seen_subcommand_from init check clean purge help" -f -a "check" -d 'Check upstream shell integration hooks'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and not __fish_seen_subcommand_from init check clean purge help" -f -a "clean" -d 'Remove upstream shell integration hooks'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and not __fish_seen_subcommand_from init check clean purge help" -f -a "purge" -d 'Remove hooks and delete the local upstream data directory'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and not __fish_seen_subcommand_from init check clean purge help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from init" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from init" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from check" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from check" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from clean" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from clean" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from purge" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from purge" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from help" -f -a "init" -d 'Add upstream shell integration hooks'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from help" -f -a "check" -d 'Check upstream shell integration hooks'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from help" -f -a "clean" -d 'Remove upstream shell integration hooks'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from help" -f -a "purge" -d 'Remove hooks and delete the local upstream data directory'
complete -c upstream -n "__fish_upstream_using_subcommand hooks; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand import" -l as -d 'Force the input type instead of autodetection' -r -f -a "keys\t''
manifest\t''
snapshot\t''"
complete -c upstream -n "__fish_upstream_using_subcommand import" -l skip-failed -d 'Continue importing remaining entries when metadata manifest processing fails'
complete -c upstream -n "__fish_upstream_using_subcommand import" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand import" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand export" -l full -d 'Export a full snapshot of the upstream directory instead of a manifest'
complete -c upstream -n "__fish_upstream_using_subcommand export" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand export" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand doctor" -l verbose -d 'Print each check result line in addition to summary output'
complete -c upstream -n "__fish_upstream_using_subcommand doctor" -l fix -d 'Attempt automatic repairs for detected issues'
complete -c upstream -n "__fish_upstream_using_subcommand doctor" -s y -l yes -d 'Accept confirmation prompts'
complete -c upstream -n "__fish_upstream_using_subcommand doctor" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list changelog probe search config package hooks import export doctor help" -f -a "install" -d 'Install a package from an upstream release source'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list changelog probe search config package hooks import export doctor help" -f -a "build" -d 'Build and install from source for release tags without artifacts'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list changelog probe search config package hooks import export doctor help" -f -a "remove" -d 'Remove one or more installed packages'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list changelog probe search config package hooks import export doctor help" -f -a "rollback" -d 'Restore or prune stored rollback artifacts'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list changelog probe search config package hooks import export doctor help" -f -a "reinstall" -d 'Reinstall one or more packages (remove then install)'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list changelog probe search config package hooks import export doctor help" -f -a "upgrade" -d 'Upgrade installed packages to their latest versions'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list changelog probe search config package hooks import export doctor help" -f -a "list" -d 'List installed packages and their metadata'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list changelog probe search config package hooks import export doctor help" -f -a "changelog" -d 'Show upstream release notes for an installed package'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list changelog probe search config package hooks import export doctor help" -f -a "probe" -d 'Inspect releases visible from a provider without installing'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list changelog probe search config package hooks import export doctor help" -f -a "search" -d 'Search provider repositories by keyword(s)'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list changelog probe search config package hooks import export doctor help" -f -a "config" -d 'Manage upstream configuration'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list changelog probe search config package hooks import export doctor help" -f -a "package" -d 'Manage package-specific behavior'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list changelog probe search config package hooks import export doctor help" -f -a "hooks" -d 'Manage shell integration hooks and local upstream data'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list changelog probe search config package hooks import export doctor help" -f -a "import" -d 'Import trusted keys, package metadata manifests, or full snapshots'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list changelog probe search config package hooks import export doctor help" -f -a "export" -d 'Export packages to a manifest or full snapshot'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list changelog probe search config package hooks import export doctor help" -f -a "doctor" -d 'Run diagnostics to detect installation and integration issues'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install build remove rollback reinstall upgrade list changelog probe search config package hooks import export doctor help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "set" -d 'Set configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "get" -d 'Get configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "list" -d 'List all configuration keys'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "edit" -d 'Open configuration file in your default editor'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "reset" -d 'Reset configuration to defaults'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from package" -f -a "pin" -d 'Pin a package to its current version'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from package" -f -a "unpin" -d 'Unpin a package to allow updates'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from package" -f -a "rename" -d 'Rename package alias without reinstalling'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from hooks" -f -a "init" -d 'Add upstream shell integration hooks'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from hooks" -f -a "check" -d 'Check upstream shell integration hooks'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from hooks" -f -a "clean" -d 'Remove upstream shell integration hooks'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from hooks" -f -a "purge" -d 'Remove hooks and delete the local upstream data directory'
