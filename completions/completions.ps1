
using namespace System.Management.Automation
using namespace System.Management.Automation.Language

Register-ArgumentCompleter -Native -CommandName 'upstream' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commandElements = $commandAst.CommandElements
    $command = @(
        'upstream'
        for ($i = 1; $i -lt $commandElements.Count; $i++) {
            $element = $commandElements[$i]
            if ($element -isnot [StringConstantExpressionAst] -or
                $element.StringConstantType -ne [StringConstantType]::BareWord -or
                $element.Value.StartsWith('-') -or
                $element.Value -eq $wordToComplete) {
                break
        }
        $element.Value
    }) -join ';'

    $completions = @(switch ($command) {
        'upstream' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('install', 'install', [CompletionResultType]::ParameterValue, 'Install a package from an upstream release source')
            [CompletionResult]::new('build', 'build', [CompletionResultType]::ParameterValue, 'Build and install from source for release tags without artifacts')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove one or more installed packages')
            [CompletionResult]::new('reinstall', 'reinstall', [CompletionResultType]::ParameterValue, 'Reinstall one or more packages (remove then install)')
            [CompletionResult]::new('upgrade', 'upgrade', [CompletionResultType]::ParameterValue, 'Upgrade installed packages to their latest versions')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List installed packages and their metadata')
            [CompletionResult]::new('probe', 'probe', [CompletionResultType]::ParameterValue, 'Inspect releases visible from a provider without installing')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'Manage upstream configuration')
            [CompletionResult]::new('package', 'package', [CompletionResultType]::ParameterValue, 'Manage package-specific settings and metadata')
            [CompletionResult]::new('hooks', 'hooks', [CompletionResultType]::ParameterValue, 'Manage shell integration hooks and local upstream data')
            [CompletionResult]::new('import', 'import', [CompletionResultType]::ParameterValue, 'Import trusted keys, package metadata manifests, or full snapshots')
            [CompletionResult]::new('export', 'export', [CompletionResultType]::ParameterValue, 'Export packages to a manifest or full snapshot')
            [CompletionResult]::new('doctor', 'doctor', [CompletionResultType]::ParameterValue, 'Run diagnostics to detect installation and integration issues')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'upstream;install' {
            [CompletionResult]::new('-t', '-t', [CompletionResultType]::ParameterName, 'Version tag to install (defaults to latest)')
            [CompletionResult]::new('--tag', '--tag', [CompletionResultType]::ParameterName, 'Version tag to install (defaults to latest)')
            [CompletionResult]::new('-k', '-k', [CompletionResultType]::ParameterName, 'File type to install')
            [CompletionResult]::new('--kind', '--kind', [CompletionResultType]::ParameterName, 'File type to install')
            [CompletionResult]::new('-p', '-p', [CompletionResultType]::ParameterName, 'Source provider hosting the repository. Defaults to auto-detection')
            [CompletionResult]::new('--provider', '--provider', [CompletionResultType]::ParameterName, 'Source provider hosting the repository. Defaults to auto-detection')
            [CompletionResult]::new('--base-url', '--base-url', [CompletionResultType]::ParameterName, 'Custom base URL. Defaults to provider''s root')
            [CompletionResult]::new('-c', '-c', [CompletionResultType]::ParameterName, 'Update channel to track')
            [CompletionResult]::new('--channel', '--channel', [CompletionResultType]::ParameterName, 'Update channel to track')
            [CompletionResult]::new('-m', '-m', [CompletionResultType]::ParameterName, 'Match pattern to use as a hint for which asset to prefer')
            [CompletionResult]::new('--match-pattern', '--match-pattern', [CompletionResultType]::ParameterName, 'Match pattern to use as a hint for which asset to prefer')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")')
            [CompletionResult]::new('--exclude-pattern', '--exclude-pattern', [CompletionResultType]::ParameterName, 'Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")')
            [CompletionResult]::new('--trust', '--trust', [CompletionResultType]::ParameterName, 'Trust verification mode for downloaded assets')
            [CompletionResult]::new('-d', '-d', [CompletionResultType]::ParameterName, 'Whether or not to create a .desktop entry for GUI applications')
            [CompletionResult]::new('--desktop', '--desktop', [CompletionResultType]::ParameterName, 'Whether or not to create a .desktop entry for GUI applications')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept the recommended discovered asset without prompting')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept the recommended discovered asset without prompting')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;build' {
            [CompletionResult]::new('-t', '-t', [CompletionResultType]::ParameterName, 'Version tag to build (defaults to latest)')
            [CompletionResult]::new('--tag', '--tag', [CompletionResultType]::ParameterName, 'Version tag to build (defaults to latest)')
            [CompletionResult]::new('--branch', '--branch', [CompletionResultType]::ParameterName, 'Branch name to build from (uses latest commit from that branch)')
            [CompletionResult]::new('-p', '-p', [CompletionResultType]::ParameterName, 'Source provider hosting the repository. Defaults to auto-detection')
            [CompletionResult]::new('--provider', '--provider', [CompletionResultType]::ParameterName, 'Source provider hosting the repository. Defaults to auto-detection')
            [CompletionResult]::new('--base-url', '--base-url', [CompletionResultType]::ParameterName, 'Custom base URL. Defaults to provider''s root')
            [CompletionResult]::new('-c', '-c', [CompletionResultType]::ParameterName, 'Update channel to track')
            [CompletionResult]::new('--channel', '--channel', [CompletionResultType]::ParameterName, 'Update channel to track')
            [CompletionResult]::new('--build-profile', '--build-profile', [CompletionResultType]::ParameterName, 'Build profile used to compile/install from source (auto-detected when omitted)')
            [CompletionResult]::new('--build-output', '--build-output', [CompletionResultType]::ParameterName, 'Optional explicit output path for the compiled executable')
            [CompletionResult]::new('-d', '-d', [CompletionResultType]::ParameterName, 'Whether or not to create a .desktop entry for GUI applications')
            [CompletionResult]::new('--desktop', '--desktop', [CompletionResultType]::ParameterName, 'Whether or not to create a .desktop entry for GUI applications')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept the recommended discovered source/release without prompting')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept the recommended discovered source/release without prompting')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;remove' {
            [CompletionResult]::new('--purge', '--purge', [CompletionResultType]::ParameterName, 'Remove all associated cached data')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;reinstall' {
            [CompletionResult]::new('--trust', '--trust', [CompletionResultType]::ParameterName, 'Trust verification mode for release-asset reinstalls')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;upgrade' {
            [CompletionResult]::new('--trust', '--trust', [CompletionResultType]::ParameterName, 'Trust verification mode for downloaded assets')
            [CompletionResult]::new('--force', '--force', [CompletionResultType]::ParameterName, 'Force upgrade even if already up to date')
            [CompletionResult]::new('--check', '--check', [CompletionResultType]::ParameterName, 'Check for available upgrades without applying them')
            [CompletionResult]::new('--machine-readable', '--machine-readable', [CompletionResultType]::ParameterName, 'Use script-friendly check output: one line per update, "name oldver newver"')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;list' {
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Print raw package metadata as JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;probe' {
            [CompletionResult]::new('-p', '-p', [CompletionResultType]::ParameterName, 'Source provider (defaults to github, or scraper for URLs)')
            [CompletionResult]::new('--provider', '--provider', [CompletionResultType]::ParameterName, 'Source provider (defaults to github, or scraper for URLs)')
            [CompletionResult]::new('--base-url', '--base-url', [CompletionResultType]::ParameterName, 'Custom base URL for self-hosted providers')
            [CompletionResult]::new('-c', '-c', [CompletionResultType]::ParameterName, 'Channel view to display')
            [CompletionResult]::new('--channel', '--channel', [CompletionResultType]::ParameterName, 'Channel view to display')
            [CompletionResult]::new('--limit', '--limit', [CompletionResultType]::ParameterName, 'Maximum number of releases to display')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Include scored candidate assets for each release')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;config' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Set configuration values')
            [CompletionResult]::new('get', 'get', [CompletionResultType]::ParameterValue, 'Get configuration values')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List all configuration keys')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Open configuration file in your default editor')
            [CompletionResult]::new('reset', 'reset', [CompletionResultType]::ParameterValue, 'Reset configuration to defaults')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'upstream;config;set' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;config;get' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;config;list' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'upstream;config;edit' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'upstream;config;reset' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'upstream;config;help' {
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Set configuration values')
            [CompletionResult]::new('get', 'get', [CompletionResultType]::ParameterValue, 'Get configuration values')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List all configuration keys')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Open configuration file in your default editor')
            [CompletionResult]::new('reset', 'reset', [CompletionResultType]::ParameterValue, 'Reset configuration to defaults')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'upstream;config;help;set' {
            break
        }
        'upstream;config;help;get' {
            break
        }
        'upstream;config;help;list' {
            break
        }
        'upstream;config;help;edit' {
            break
        }
        'upstream;config;help;reset' {
            break
        }
        'upstream;config;help;help' {
            break
        }
        'upstream;package' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('pin', 'pin', [CompletionResultType]::ParameterValue, 'Pin a package to its current version')
            [CompletionResult]::new('unpin', 'unpin', [CompletionResultType]::ParameterValue, 'Unpin a package to allow updates')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a package entry from upstream metadata')
            [CompletionResult]::new('get-key', 'get-key', [CompletionResultType]::ParameterValue, 'Get specific package metadata fields')
            [CompletionResult]::new('set-key', 'set-key', [CompletionResultType]::ParameterValue, 'Manually set package metadata fields')
            [CompletionResult]::new('rename', 'rename', [CompletionResultType]::ParameterValue, 'Rename package alias without reinstalling')
            [CompletionResult]::new('metadata', 'metadata', [CompletionResultType]::ParameterValue, 'Display all metadata for a package')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'upstream;package;pin' {
            [CompletionResult]::new('--reason', '--reason', [CompletionResultType]::ParameterName, 'Optional reason for pinning this package')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;package;unpin' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;package;remove' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;package;get-key' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;package;set-key' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;package;rename' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;package;metadata' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;package;help' {
            [CompletionResult]::new('pin', 'pin', [CompletionResultType]::ParameterValue, 'Pin a package to its current version')
            [CompletionResult]::new('unpin', 'unpin', [CompletionResultType]::ParameterValue, 'Unpin a package to allow updates')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a package entry from upstream metadata')
            [CompletionResult]::new('get-key', 'get-key', [CompletionResultType]::ParameterValue, 'Get specific package metadata fields')
            [CompletionResult]::new('set-key', 'set-key', [CompletionResultType]::ParameterValue, 'Manually set package metadata fields')
            [CompletionResult]::new('rename', 'rename', [CompletionResultType]::ParameterValue, 'Rename package alias without reinstalling')
            [CompletionResult]::new('metadata', 'metadata', [CompletionResultType]::ParameterValue, 'Display all metadata for a package')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'upstream;package;help;pin' {
            break
        }
        'upstream;package;help;unpin' {
            break
        }
        'upstream;package;help;remove' {
            break
        }
        'upstream;package;help;get-key' {
            break
        }
        'upstream;package;help;set-key' {
            break
        }
        'upstream;package;help;rename' {
            break
        }
        'upstream;package;help;metadata' {
            break
        }
        'upstream;package;help;help' {
            break
        }
        'upstream;hooks' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Add upstream shell integration hooks')
            [CompletionResult]::new('check', 'check', [CompletionResultType]::ParameterValue, 'Check upstream shell integration hooks')
            [CompletionResult]::new('clean', 'clean', [CompletionResultType]::ParameterValue, 'Remove upstream shell integration hooks')
            [CompletionResult]::new('purge', 'purge', [CompletionResultType]::ParameterValue, 'Remove hooks and delete the local upstream data directory')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'upstream;hooks;init' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;hooks;check' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;hooks;clean' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;hooks;purge' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Skip the confirmation prompt')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;hooks;help' {
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Add upstream shell integration hooks')
            [CompletionResult]::new('check', 'check', [CompletionResultType]::ParameterValue, 'Check upstream shell integration hooks')
            [CompletionResult]::new('clean', 'clean', [CompletionResultType]::ParameterValue, 'Remove upstream shell integration hooks')
            [CompletionResult]::new('purge', 'purge', [CompletionResultType]::ParameterValue, 'Remove hooks and delete the local upstream data directory')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'upstream;hooks;help;init' {
            break
        }
        'upstream;hooks;help;check' {
            break
        }
        'upstream;hooks;help;clean' {
            break
        }
        'upstream;hooks;help;purge' {
            break
        }
        'upstream;hooks;help;help' {
            break
        }
        'upstream;import' {
            [CompletionResult]::new('--as', '--as', [CompletionResultType]::ParameterName, 'Force the input type instead of autodetection')
            [CompletionResult]::new('--skip-failed', '--skip-failed', [CompletionResultType]::ParameterName, 'Continue importing remaining entries when metadata manifest processing fails')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Skip import confirmation prompt')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip import confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;export' {
            [CompletionResult]::new('--full', '--full', [CompletionResultType]::ParameterName, 'Export a full snapshot of the upstream directory instead of a manifest')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;doctor' {
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Print each check result line in addition to summary output')
            [CompletionResult]::new('--fix', '--fix', [CompletionResultType]::ParameterName, 'Attempt automatic repairs for detected issues')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;help' {
            [CompletionResult]::new('install', 'install', [CompletionResultType]::ParameterValue, 'Install a package from an upstream release source')
            [CompletionResult]::new('build', 'build', [CompletionResultType]::ParameterValue, 'Build and install from source for release tags without artifacts')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove one or more installed packages')
            [CompletionResult]::new('reinstall', 'reinstall', [CompletionResultType]::ParameterValue, 'Reinstall one or more packages (remove then install)')
            [CompletionResult]::new('upgrade', 'upgrade', [CompletionResultType]::ParameterValue, 'Upgrade installed packages to their latest versions')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List installed packages and their metadata')
            [CompletionResult]::new('probe', 'probe', [CompletionResultType]::ParameterValue, 'Inspect releases visible from a provider without installing')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'Manage upstream configuration')
            [CompletionResult]::new('package', 'package', [CompletionResultType]::ParameterValue, 'Manage package-specific settings and metadata')
            [CompletionResult]::new('hooks', 'hooks', [CompletionResultType]::ParameterValue, 'Manage shell integration hooks and local upstream data')
            [CompletionResult]::new('import', 'import', [CompletionResultType]::ParameterValue, 'Import trusted keys, package metadata manifests, or full snapshots')
            [CompletionResult]::new('export', 'export', [CompletionResultType]::ParameterValue, 'Export packages to a manifest or full snapshot')
            [CompletionResult]::new('doctor', 'doctor', [CompletionResultType]::ParameterValue, 'Run diagnostics to detect installation and integration issues')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'upstream;help;install' {
            break
        }
        'upstream;help;build' {
            break
        }
        'upstream;help;remove' {
            break
        }
        'upstream;help;reinstall' {
            break
        }
        'upstream;help;upgrade' {
            break
        }
        'upstream;help;list' {
            break
        }
        'upstream;help;probe' {
            break
        }
        'upstream;help;config' {
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Set configuration values')
            [CompletionResult]::new('get', 'get', [CompletionResultType]::ParameterValue, 'Get configuration values')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List all configuration keys')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Open configuration file in your default editor')
            [CompletionResult]::new('reset', 'reset', [CompletionResultType]::ParameterValue, 'Reset configuration to defaults')
            break
        }
        'upstream;help;config;set' {
            break
        }
        'upstream;help;config;get' {
            break
        }
        'upstream;help;config;list' {
            break
        }
        'upstream;help;config;edit' {
            break
        }
        'upstream;help;config;reset' {
            break
        }
        'upstream;help;package' {
            [CompletionResult]::new('pin', 'pin', [CompletionResultType]::ParameterValue, 'Pin a package to its current version')
            [CompletionResult]::new('unpin', 'unpin', [CompletionResultType]::ParameterValue, 'Unpin a package to allow updates')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a package entry from upstream metadata')
            [CompletionResult]::new('get-key', 'get-key', [CompletionResultType]::ParameterValue, 'Get specific package metadata fields')
            [CompletionResult]::new('set-key', 'set-key', [CompletionResultType]::ParameterValue, 'Manually set package metadata fields')
            [CompletionResult]::new('rename', 'rename', [CompletionResultType]::ParameterValue, 'Rename package alias without reinstalling')
            [CompletionResult]::new('metadata', 'metadata', [CompletionResultType]::ParameterValue, 'Display all metadata for a package')
            break
        }
        'upstream;help;package;pin' {
            break
        }
        'upstream;help;package;unpin' {
            break
        }
        'upstream;help;package;remove' {
            break
        }
        'upstream;help;package;get-key' {
            break
        }
        'upstream;help;package;set-key' {
            break
        }
        'upstream;help;package;rename' {
            break
        }
        'upstream;help;package;metadata' {
            break
        }
        'upstream;help;hooks' {
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Add upstream shell integration hooks')
            [CompletionResult]::new('check', 'check', [CompletionResultType]::ParameterValue, 'Check upstream shell integration hooks')
            [CompletionResult]::new('clean', 'clean', [CompletionResultType]::ParameterValue, 'Remove upstream shell integration hooks')
            [CompletionResult]::new('purge', 'purge', [CompletionResultType]::ParameterValue, 'Remove hooks and delete the local upstream data directory')
            break
        }
        'upstream;help;hooks;init' {
            break
        }
        'upstream;help;hooks;check' {
            break
        }
        'upstream;help;hooks;clean' {
            break
        }
        'upstream;help;hooks;purge' {
            break
        }
        'upstream;help;import' {
            break
        }
        'upstream;help;export' {
            break
        }
        'upstream;help;doctor' {
            break
        }
        'upstream;help;help' {
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}
