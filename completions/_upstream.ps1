
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
            [CompletionResult]::new('install', 'install', [CompletionResultType]::ParameterValue, 'Install a package from a GitHub release')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove one or more installed packages')
            [CompletionResult]::new('upgrade', 'upgrade', [CompletionResultType]::ParameterValue, 'Upgrade installed packages to their latest versions')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List installed packages and their metadata')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'Manage upstream configuration')
            [CompletionResult]::new('package', 'package', [CompletionResultType]::ParameterValue, 'Manage package-specific settings and metadata')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Initialize upstream by adding it to your shell PATH')
            [CompletionResult]::new('import', 'import', [CompletionResultType]::ParameterValue, 'Import packages from a manifest or full snapshot')
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
            [CompletionResult]::new('-p', '-p', [CompletionResultType]::ParameterName, 'Source provider hosting the repository')
            [CompletionResult]::new('--provider', '--provider', [CompletionResultType]::ParameterName, 'Source provider hosting the repository')
            [CompletionResult]::new('--base-url', '--base-url', [CompletionResultType]::ParameterName, 'Custom base URL. Defaults to provider''s root')
            [CompletionResult]::new('-c', '-c', [CompletionResultType]::ParameterName, 'Update channel to track')
            [CompletionResult]::new('--channel', '--channel', [CompletionResultType]::ParameterName, 'Update channel to track')
            [CompletionResult]::new('-m', '-m', [CompletionResultType]::ParameterName, 'Match pattern to use as a hint for which asset to prefer')
            [CompletionResult]::new('--match-pattern', '--match-pattern', [CompletionResultType]::ParameterName, 'Match pattern to use as a hint for which asset to prefer')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")')
            [CompletionResult]::new('--exclude-pattern', '--exclude-pattern', [CompletionResultType]::ParameterName, 'Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")')
            [CompletionResult]::new('-d', '-d', [CompletionResultType]::ParameterName, 'Whether or not to create a .desktop entry for GUI applications')
            [CompletionResult]::new('--desktop', '--desktop', [CompletionResultType]::ParameterName, 'Whether or not to create a .desktop entry for GUI applications')
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
        'upstream;upgrade' {
            [CompletionResult]::new('--force', '--force', [CompletionResultType]::ParameterName, 'Force upgrade even if already up to date')
            [CompletionResult]::new('--check', '--check', [CompletionResultType]::ParameterName, 'Check for available upgrades without applying them')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;list' {
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
            [CompletionResult]::new('get-key', 'get-key', [CompletionResultType]::ParameterValue, 'Get specific package metadata fields')
            [CompletionResult]::new('set-key', 'set-key', [CompletionResultType]::ParameterValue, 'Manually set package metadata fields')
            [CompletionResult]::new('metadata', 'metadata', [CompletionResultType]::ParameterValue, 'Display all metadata for a package')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'upstream;package;pin' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;package;unpin' {
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
        'upstream;package;metadata' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;package;help' {
            [CompletionResult]::new('pin', 'pin', [CompletionResultType]::ParameterValue, 'Pin a package to its current version')
            [CompletionResult]::new('unpin', 'unpin', [CompletionResultType]::ParameterValue, 'Unpin a package to allow updates')
            [CompletionResult]::new('get-key', 'get-key', [CompletionResultType]::ParameterValue, 'Get specific package metadata fields')
            [CompletionResult]::new('set-key', 'set-key', [CompletionResultType]::ParameterValue, 'Manually set package metadata fields')
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
        'upstream;package;help;get-key' {
            break
        }
        'upstream;package;help;set-key' {
            break
        }
        'upstream;package;help;metadata' {
            break
        }
        'upstream;package;help;help' {
            break
        }
        'upstream;init' {
            [CompletionResult]::new('--clean', '--clean', [CompletionResultType]::ParameterName, 'Clean initialization (remove existing hooks first)')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;import' {
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
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;help' {
            [CompletionResult]::new('install', 'install', [CompletionResultType]::ParameterValue, 'Install a package from a GitHub release')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove one or more installed packages')
            [CompletionResult]::new('upgrade', 'upgrade', [CompletionResultType]::ParameterValue, 'Upgrade installed packages to their latest versions')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List installed packages and their metadata')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'Manage upstream configuration')
            [CompletionResult]::new('package', 'package', [CompletionResultType]::ParameterValue, 'Manage package-specific settings and metadata')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Initialize upstream by adding it to your shell PATH')
            [CompletionResult]::new('import', 'import', [CompletionResultType]::ParameterValue, 'Import packages from a manifest or full snapshot')
            [CompletionResult]::new('export', 'export', [CompletionResultType]::ParameterValue, 'Export packages to a manifest or full snapshot')
            [CompletionResult]::new('doctor', 'doctor', [CompletionResultType]::ParameterValue, 'Run diagnostics to detect installation and integration issues')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'upstream;help;install' {
            break
        }
        'upstream;help;remove' {
            break
        }
        'upstream;help;upgrade' {
            break
        }
        'upstream;help;list' {
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
            [CompletionResult]::new('get-key', 'get-key', [CompletionResultType]::ParameterValue, 'Get specific package metadata fields')
            [CompletionResult]::new('set-key', 'set-key', [CompletionResultType]::ParameterValue, 'Manually set package metadata fields')
            [CompletionResult]::new('metadata', 'metadata', [CompletionResultType]::ParameterValue, 'Display all metadata for a package')
            break
        }
        'upstream;help;package;pin' {
            break
        }
        'upstream;help;package;unpin' {
            break
        }
        'upstream;help;package;get-key' {
            break
        }
        'upstream;help;package;set-key' {
            break
        }
        'upstream;help;package;metadata' {
            break
        }
        'upstream;help;init' {
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
