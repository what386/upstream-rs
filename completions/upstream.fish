# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_upstream_global_optspecs
	string join \n h/help V/version
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

complete -c upstream -n "__fish_upstream_needs_command" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_needs_command" -s V -l version -d 'Print version'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "install" -d 'Install a package from a GitHub release'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "remove" -d 'Remove one or more installed packages'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "upgrade" -d 'Upgrade installed packages to their latest versions'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "list" -d 'List installed packages and their metadata'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "config" -d 'Manage upstream configuration'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "package" -d 'Manage package-specific settings and metadata'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "init" -d 'Initialize upstream by adding it to your shell PATH'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "import" -d 'Import packages from a manifest or full snapshot'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "export" -d 'Export packages to a manifest or full snapshot'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "doctor" -d 'Run diagnostics to detect installation and integration issues'
complete -c upstream -n "__fish_upstream_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand install" -s t -l tag -d 'Version tag to install (defaults to latest)' -r
complete -c upstream -n "__fish_upstream_using_subcommand install" -s k -l kind -d 'File type to install' -r -f -a "app-image\t''
archive\t''
compressed\t''
binary\t''
win-exe\t''
checksum\t''
auto\t''"
complete -c upstream -n "__fish_upstream_using_subcommand install" -s p -l provider -d 'Source provider hosting the repository' -r
complete -c upstream -n "__fish_upstream_using_subcommand install" -l base-url -d 'Custom base URL. Defaults to provider\'s root' -r
complete -c upstream -n "__fish_upstream_using_subcommand install" -s c -l channel -d 'Update channel to track' -r -f -a "stable\t''
nightly\t''"
complete -c upstream -n "__fish_upstream_using_subcommand install" -s m -l match-pattern -d 'Match pattern to use as a hint for which asset to prefer' -r
complete -c upstream -n "__fish_upstream_using_subcommand install" -s e -l exclude-pattern -d 'Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")' -r
complete -c upstream -n "__fish_upstream_using_subcommand install" -s d -l desktop -d 'Whether or not to create a .desktop entry for GUI applications'
complete -c upstream -n "__fish_upstream_using_subcommand install" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand remove" -l purge -d 'Remove all associated cached data'
complete -c upstream -n "__fish_upstream_using_subcommand remove" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand upgrade" -l force -d 'Force upgrade even if already up to date'
complete -c upstream -n "__fish_upstream_using_subcommand upgrade" -l check -d 'Check for available upgrades without applying them'
complete -c upstream -n "__fish_upstream_using_subcommand upgrade" -l machine-readable -d 'Use script-friendly check output: one line per update, "name oldver newver"'
complete -c upstream -n "__fish_upstream_using_subcommand upgrade" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand list" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list edit reset help" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list edit reset help" -f -a "set" -d 'Set configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list edit reset help" -f -a "get" -d 'Get configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list edit reset help" -f -a "list" -d 'List all configuration keys'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list edit reset help" -f -a "edit" -d 'Open configuration file in your default editor'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list edit reset help" -f -a "reset" -d 'Reset configuration to defaults'
complete -c upstream -n "__fish_upstream_using_subcommand config; and not __fish_seen_subcommand_from set get list edit reset help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from set" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from get" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from edit" -s h -l help -d 'Print help'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from reset" -s h -l help -d 'Print help'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "set" -d 'Set configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "get" -d 'Get configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "list" -d 'List all configuration keys'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "edit" -d 'Open configuration file in your default editor'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "reset" -d 'Reset configuration to defaults'
complete -c upstream -n "__fish_upstream_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin get-key set-key metadata help" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin get-key set-key metadata help" -f -a "pin" -d 'Pin a package to its current version'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin get-key set-key metadata help" -f -a "unpin" -d 'Unpin a package to allow updates'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin get-key set-key metadata help" -f -a "get-key" -d 'Get specific package metadata fields'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin get-key set-key metadata help" -f -a "set-key" -d 'Manually set package metadata fields'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin get-key set-key metadata help" -f -a "metadata" -d 'Display all metadata for a package'
complete -c upstream -n "__fish_upstream_using_subcommand package; and not __fish_seen_subcommand_from pin unpin get-key set-key metadata help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from pin" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from unpin" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from get-key" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from set-key" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from metadata" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from help" -f -a "pin" -d 'Pin a package to its current version'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from help" -f -a "unpin" -d 'Unpin a package to allow updates'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from help" -f -a "get-key" -d 'Get specific package metadata fields'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from help" -f -a "set-key" -d 'Manually set package metadata fields'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from help" -f -a "metadata" -d 'Display all metadata for a package'
complete -c upstream -n "__fish_upstream_using_subcommand package; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand init" -l clean -d 'Clean initialization (remove existing hooks first)'
complete -c upstream -n "__fish_upstream_using_subcommand init" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand import" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand export" -l full -d 'Export a full snapshot of the upstream directory instead of a manifest'
complete -c upstream -n "__fish_upstream_using_subcommand export" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand doctor" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install remove upgrade list config package init import export doctor help" -f -a "install" -d 'Install a package from a GitHub release'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install remove upgrade list config package init import export doctor help" -f -a "remove" -d 'Remove one or more installed packages'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install remove upgrade list config package init import export doctor help" -f -a "upgrade" -d 'Upgrade installed packages to their latest versions'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install remove upgrade list config package init import export doctor help" -f -a "list" -d 'List installed packages and their metadata'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install remove upgrade list config package init import export doctor help" -f -a "config" -d 'Manage upstream configuration'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install remove upgrade list config package init import export doctor help" -f -a "package" -d 'Manage package-specific settings and metadata'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install remove upgrade list config package init import export doctor help" -f -a "init" -d 'Initialize upstream by adding it to your shell PATH'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install remove upgrade list config package init import export doctor help" -f -a "import" -d 'Import packages from a manifest or full snapshot'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install remove upgrade list config package init import export doctor help" -f -a "export" -d 'Export packages to a manifest or full snapshot'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install remove upgrade list config package init import export doctor help" -f -a "doctor" -d 'Run diagnostics to detect installation and integration issues'
complete -c upstream -n "__fish_upstream_using_subcommand help; and not __fish_seen_subcommand_from install remove upgrade list config package init import export doctor help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "set" -d 'Set configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "get" -d 'Get configuration values'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "list" -d 'List all configuration keys'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "edit" -d 'Open configuration file in your default editor'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "reset" -d 'Reset configuration to defaults'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from package" -f -a "pin" -d 'Pin a package to its current version'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from package" -f -a "unpin" -d 'Unpin a package to allow updates'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from package" -f -a "get-key" -d 'Get specific package metadata fields'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from package" -f -a "set-key" -d 'Manually set package metadata fields'
complete -c upstream -n "__fish_upstream_using_subcommand help; and __fish_seen_subcommand_from package" -f -a "metadata" -d 'Display all metadata for a package'
