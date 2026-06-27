
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
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('install', 'install', [CompletionResultType]::ParameterValue, 'Install a release asset or direct download')
            [CompletionResult]::new('build', 'build', [CompletionResultType]::ParameterValue, 'Build and install a package from source')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove installed package files and metadata')
            [CompletionResult]::new('uninstall', 'uninstall', [CompletionResultType]::ParameterValue, 'Remove installed package files and metadata')
            [CompletionResult]::new('rollback', 'rollback', [CompletionResultType]::ParameterValue, 'Restore or prune stored rollback artifacts')
            [CompletionResult]::new('reinstall', 'reinstall', [CompletionResultType]::ParameterValue, 'Reinstall packages from their stored source metadata')
            [CompletionResult]::new('upgrade', 'upgrade', [CompletionResultType]::ParameterValue, 'Check for or install package updates')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List installed packages')
            [CompletionResult]::new('info', 'info', [CompletionResultType]::ParameterValue, 'Show details for one installed package')
            [CompletionResult]::new('changelog', 'changelog', [CompletionResultType]::ParameterValue, 'Show release notes for an installed package')
            [CompletionResult]::new('docs', 'docs', [CompletionResultType]::ParameterValue, 'Search cached or fetched package README docs')
            [CompletionResult]::new('probe', 'probe', [CompletionResultType]::ParameterValue, 'Inspect releases, choose an asset, and install it')
            [CompletionResult]::new('search', 'search', [CompletionResultType]::ParameterValue, 'Search provider repositories without installing')
            [CompletionResult]::new('find', 'find', [CompletionResultType]::ParameterValue, 'Search repositories interactively and install one')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'View, edit, and validate config.toml')
            [CompletionResult]::new('package', 'package', [CompletionResultType]::ParameterValue, 'Manage installed package records and launcher entries')
            [CompletionResult]::new('hooks', 'hooks', [CompletionResultType]::ParameterValue, 'Manage shell PATH hooks and local upstream data')
            [CompletionResult]::new('import', 'import', [CompletionResultType]::ParameterValue, 'Import config, trust keys, packages, or a profile')
            [CompletionResult]::new('export', 'export', [CompletionResultType]::ParameterValue, 'Export config, trust keys, packages, or a profile')
            [CompletionResult]::new('doctor', 'doctor', [CompletionResultType]::ParameterValue, 'Run diagnostics to detect installation and integration issues')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'upstream;install' {
            [CompletionResult]::new('-t', '-t', [CompletionResultType]::ParameterName, 'Release tag to install (defaults to latest matching the channel)')
            [CompletionResult]::new('--tag', '--tag', [CompletionResultType]::ParameterName, 'Release tag to install (defaults to latest matching the channel)')
            [CompletionResult]::new('-k', '-k', [CompletionResultType]::ParameterName, 'Asset kind to install')
            [CompletionResult]::new('--kind', '--kind', [CompletionResultType]::ParameterName, 'Asset kind to install')
            [CompletionResult]::new('-p', '-p', [CompletionResultType]::ParameterName, 'Source provider hosting the repository. Defaults to auto-detection')
            [CompletionResult]::new('--provider', '--provider', [CompletionResultType]::ParameterName, 'Source provider hosting the repository. Defaults to auto-detection')
            [CompletionResult]::new('--base-url', '--base-url', [CompletionResultType]::ParameterName, 'Custom base URL. Defaults to provider''s root')
            [CompletionResult]::new('-c', '-c', [CompletionResultType]::ParameterName, 'Release channel to track for upgrades')
            [CompletionResult]::new('--channel', '--channel', [CompletionResultType]::ParameterName, 'Release channel to track for upgrades')
            [CompletionResult]::new('-m', '-m', [CompletionResultType]::ParameterName, 'Match pattern to use as a hint for which asset to prefer')
            [CompletionResult]::new('--match-pattern', '--match-pattern', [CompletionResultType]::ParameterName, 'Match pattern to use as a hint for which asset to prefer')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")')
            [CompletionResult]::new('--exclude-pattern', '--exclude-pattern', [CompletionResultType]::ParameterName, 'Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")')
            [CompletionResult]::new('--trust', '--trust', [CompletionResultType]::ParameterName, 'Trust verification mode for downloaded assets')
            [CompletionResult]::new('-d', '-d', [CompletionResultType]::ParameterName, 'Create a desktop launcher entry for GUI applications')
            [CompletionResult]::new('--desktop', '--desktop', [CompletionResultType]::ParameterName, 'Create a desktop launcher entry for GUI applications')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Preview install resolution without downloading, installing, or writing metadata')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;build' {
            [CompletionResult]::new('-t', '-t', [CompletionResultType]::ParameterName, 'Release tag to build (defaults to latest matching the channel)')
            [CompletionResult]::new('--tag', '--tag', [CompletionResultType]::ParameterName, 'Release tag to build (defaults to latest matching the channel)')
            [CompletionResult]::new('--branch', '--branch', [CompletionResultType]::ParameterName, 'Branch to build from instead of a release tag')
            [CompletionResult]::new('-p', '-p', [CompletionResultType]::ParameterName, 'Source provider hosting the repository. Defaults to auto-detection')
            [CompletionResult]::new('--provider', '--provider', [CompletionResultType]::ParameterName, 'Source provider hosting the repository. Defaults to auto-detection')
            [CompletionResult]::new('--base-url', '--base-url', [CompletionResultType]::ParameterName, 'Custom base URL. Defaults to provider''s root')
            [CompletionResult]::new('-c', '-c', [CompletionResultType]::ParameterName, 'Release channel to track for future builds')
            [CompletionResult]::new('--channel', '--channel', [CompletionResultType]::ParameterName, 'Release channel to track for future builds')
            [CompletionResult]::new('--build-profile', '--build-profile', [CompletionResultType]::ParameterName, 'Build profile used to compile/install from source (auto-detected when omitted)')
            [CompletionResult]::new('-d', '-d', [CompletionResultType]::ParameterName, 'Create a desktop launcher entry for GUI applications')
            [CompletionResult]::new('--desktop', '--desktop', [CompletionResultType]::ParameterName, 'Create a desktop launcher entry for GUI applications')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Preview build resolution without compiling, installing, or writing metadata')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;remove' {
            [CompletionResult]::new('--purge', '--purge', [CompletionResultType]::ParameterName, 'Remove package-owned cached data as well as active files')
            [CompletionResult]::new('--force', '--force', [CompletionResultType]::ParameterName, 'Remove metadata even when uninstall cleanup fails')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Preview removal actions without deleting files or metadata')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;uninstall' {
            [CompletionResult]::new('--purge', '--purge', [CompletionResultType]::ParameterName, 'Remove package-owned cached data as well as active files')
            [CompletionResult]::new('--force', '--force', [CompletionResultType]::ParameterName, 'Remove metadata even when uninstall cleanup fails')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Preview removal actions without deleting files or metadata')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;rollback' {
            [CompletionResult]::new('--prune', '--prune', [CompletionResultType]::ParameterName, 'Delete rollback artifacts for all packages or selected package names')
            [CompletionResult]::new('--list', '--list', [CompletionResultType]::ParameterName, 'List available rollback artifacts')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Preview rollback restore or prune actions without modifying files or metadata')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;reinstall' {
            [CompletionResult]::new('--trust', '--trust', [CompletionResultType]::ParameterName, 'Trust verification mode for release-asset reinstalls')
            [CompletionResult]::new('--force', '--force', [CompletionResultType]::ParameterName, 'Continue reinstalling after uninstall cleanup errors')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Preview reinstall resolution without removing, installing, or writing metadata')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;upgrade' {
            [CompletionResult]::new('--trust', '--trust', [CompletionResultType]::ParameterName, 'Trust verification mode for downloaded assets')
            [CompletionResult]::new('--force', '--force', [CompletionResultType]::ParameterName, 'Reinstall even when the selected version is already installed')
            [CompletionResult]::new('--check', '--check', [CompletionResultType]::ParameterName, 'Check for available upgrades without applying them')
            [CompletionResult]::new('--machine-readable', '--machine-readable', [CompletionResultType]::ParameterName, 'Print one line per available update: "name oldver newver"')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Print check results as JSON')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Preview upgrade resolution without downloading, installing, or writing metadata')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;list' {
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Print package list as JSON')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;info' {
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Print raw package metadata as JSON')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;changelog' {
            [CompletionResult]::new('--from', '--from', [CompletionResultType]::ParameterName, 'Starting release tag, or "current"')
            [CompletionResult]::new('--to', '--to', [CompletionResultType]::ParameterName, 'Ending release tag, "current", or "latest"')
            [CompletionResult]::new('--for', '--for', [CompletionResultType]::ParameterName, 'Show release notes for exactly one release tag')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;docs' {
            [CompletionResult]::new('--fetch', '--fetch', [CompletionResultType]::ParameterName, 'Refresh cached README docs for named packages, or all installed packages when empty')
            [CompletionResult]::new('--offline', '--offline', [CompletionResultType]::ParameterName, 'Use only the cached README and skip network fetching')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;probe' {
            [CompletionResult]::new('-p', '-p', [CompletionResultType]::ParameterName, 'Source provider (defaults to GitHub, or scraper for plain URLs)')
            [CompletionResult]::new('--provider', '--provider', [CompletionResultType]::ParameterName, 'Source provider (defaults to GitHub, or scraper for plain URLs)')
            [CompletionResult]::new('--base-url', '--base-url', [CompletionResultType]::ParameterName, 'Custom base URL for self-hosted providers')
            [CompletionResult]::new('-c', '-c', [CompletionResultType]::ParameterName, 'Release channel to display and track')
            [CompletionResult]::new('--channel', '--channel', [CompletionResultType]::ParameterName, 'Release channel to display and track')
            [CompletionResult]::new('--limit', '--limit', [CompletionResultType]::ParameterName, 'Number of releases to inspect instead of only one tag/latest release')
            [CompletionResult]::new('--tag', '--tag', [CompletionResultType]::ParameterName, 'Release tag to inspect exactly')
            [CompletionResult]::new('-k', '-k', [CompletionResultType]::ParameterName, 'Asset kind to show and install')
            [CompletionResult]::new('--kind', '--kind', [CompletionResultType]::ParameterName, 'Asset kind to show and install')
            [CompletionResult]::new('--trust', '--trust', [CompletionResultType]::ParameterName, 'Trust verification mode for downloaded assets')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Show scored candidate assets and selection details')
            [CompletionResult]::new('--include-incompatible', '--include-incompatible', [CompletionResultType]::ParameterName, 'Include assets that do not match the current OS/architecture or selected file type')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Print probe results as JSON and exit')
            [CompletionResult]::new('-d', '-d', [CompletionResultType]::ParameterName, 'Create a desktop launcher entry for GUI applications')
            [CompletionResult]::new('--desktop', '--desktop', [CompletionResultType]::ParameterName, 'Create a desktop launcher entry for GUI applications')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Show parsed releases without selecting, downloading, or installing')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;search' {
            [CompletionResult]::new('-p', '-p', [CompletionResultType]::ParameterName, 'Source provider to search (defaults to GitHub)')
            [CompletionResult]::new('--provider', '--provider', [CompletionResultType]::ParameterName, 'Source provider to search (defaults to GitHub)')
            [CompletionResult]::new('--base-url', '--base-url', [CompletionResultType]::ParameterName, 'Custom base URL for self-hosted providers')
            [CompletionResult]::new('--limit', '--limit', [CompletionResultType]::ParameterName, 'Maximum number of results to display')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Restrict results to repositories with this primary language')
            [CompletionResult]::new('--topic', '--topic', [CompletionResultType]::ParameterName, 'Restrict results to repositories tagged with this topic')
            [CompletionResult]::new('--min-stars', '--min-stars', [CompletionResultType]::ParameterName, 'Restrict results to repositories with at least this many stars')
            [CompletionResult]::new('--max-stars', '--max-stars', [CompletionResultType]::ParameterName, 'Restrict results to repositories with at most this many stars')
            [CompletionResult]::new('--pushed-after', '--pushed-after', [CompletionResultType]::ParameterName, 'Restrict results to repositories pushed on or after YYYY-MM-DD')
            [CompletionResult]::new('--include-forks', '--include-forks', [CompletionResultType]::ParameterName, 'Include forked repositories in provider search results')
            [CompletionResult]::new('--include-archived', '--include-archived', [CompletionResultType]::ParameterName, 'Include archived repositories in provider search results')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Print repository search results as JSON')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;find' {
            [CompletionResult]::new('-p', '-p', [CompletionResultType]::ParameterName, 'Source provider to search (defaults to GitHub)')
            [CompletionResult]::new('--provider', '--provider', [CompletionResultType]::ParameterName, 'Source provider to search (defaults to GitHub)')
            [CompletionResult]::new('--base-url', '--base-url', [CompletionResultType]::ParameterName, 'Custom base URL for self-hosted providers')
            [CompletionResult]::new('--limit', '--limit', [CompletionResultType]::ParameterName, 'Maximum number of results to display')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Restrict results to repositories with this primary language')
            [CompletionResult]::new('--topic', '--topic', [CompletionResultType]::ParameterName, 'Restrict results to repositories tagged with this topic')
            [CompletionResult]::new('--min-stars', '--min-stars', [CompletionResultType]::ParameterName, 'Restrict results to repositories with at least this many stars')
            [CompletionResult]::new('--max-stars', '--max-stars', [CompletionResultType]::ParameterName, 'Restrict results to repositories with at most this many stars')
            [CompletionResult]::new('--pushed-after', '--pushed-after', [CompletionResultType]::ParameterName, 'Restrict results to repositories pushed on or after YYYY-MM-DD')
            [CompletionResult]::new('--name', '--name', [CompletionResultType]::ParameterName, 'Package name to register without prompting')
            [CompletionResult]::new('-k', '-k', [CompletionResultType]::ParameterName, 'Asset kind to install')
            [CompletionResult]::new('--kind', '--kind', [CompletionResultType]::ParameterName, 'Asset kind to install')
            [CompletionResult]::new('-c', '-c', [CompletionResultType]::ParameterName, 'Release channel to track for upgrades')
            [CompletionResult]::new('--channel', '--channel', [CompletionResultType]::ParameterName, 'Release channel to track for upgrades')
            [CompletionResult]::new('-m', '-m', [CompletionResultType]::ParameterName, 'Match pattern to use as a hint for which asset to prefer')
            [CompletionResult]::new('--match-pattern', '--match-pattern', [CompletionResultType]::ParameterName, 'Match pattern to use as a hint for which asset to prefer')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")')
            [CompletionResult]::new('--exclude-pattern', '--exclude-pattern', [CompletionResultType]::ParameterName, 'Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")')
            [CompletionResult]::new('--trust', '--trust', [CompletionResultType]::ParameterName, 'Trust verification mode for downloaded assets')
            [CompletionResult]::new('--include-forks', '--include-forks', [CompletionResultType]::ParameterName, 'Include forked repositories in provider search results')
            [CompletionResult]::new('--include-archived', '--include-archived', [CompletionResultType]::ParameterName, 'Include archived repositories in provider search results')
            [CompletionResult]::new('-d', '-d', [CompletionResultType]::ParameterName, 'Create a desktop launcher entry for GUI applications')
            [CompletionResult]::new('--desktop', '--desktop', [CompletionResultType]::ParameterName, 'Create a desktop launcher entry for GUI applications')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Preview install resolution without downloading, installing, or writing metadata')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;config' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Set configuration values')
            [CompletionResult]::new('get', 'get', [CompletionResultType]::ParameterValue, 'Get configuration values')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List current configuration values')
            [CompletionResult]::new('verify', 'verify', [CompletionResultType]::ParameterValue, 'Check config.toml for missing or unused keys')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Open config.toml in your default editor')
            [CompletionResult]::new('reset', 'reset', [CompletionResultType]::ParameterValue, 'Reset config.toml to defaults')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'upstream;config;set' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;config;get' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;config;list' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;config;verify' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;config;edit' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;config;reset' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;config;help' {
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Set configuration values')
            [CompletionResult]::new('get', 'get', [CompletionResultType]::ParameterValue, 'Get configuration values')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List current configuration values')
            [CompletionResult]::new('verify', 'verify', [CompletionResultType]::ParameterValue, 'Check config.toml for missing or unused keys')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Open config.toml in your default editor')
            [CompletionResult]::new('reset', 'reset', [CompletionResultType]::ParameterValue, 'Reset config.toml to defaults')
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
        'upstream;config;help;verify' {
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
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('pin', 'pin', [CompletionResultType]::ParameterValue, 'Mark an installed package as pinned')
            [CompletionResult]::new('unpin', 'unpin', [CompletionResultType]::ParameterValue, 'Clear the pinned flag on an installed package')
            [CompletionResult]::new('rename', 'rename', [CompletionResultType]::ParameterValue, 'Rename an installed package record and aliases')
            [CompletionResult]::new('add-entry', 'add-entry', [CompletionResultType]::ParameterValue, 'Add a desktop launcher entry for an installed package')
            [CompletionResult]::new('rm-entry', 'rm-entry', [CompletionResultType]::ParameterValue, 'Remove an upstream-managed desktop launcher entry')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'upstream;package;pin' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;package;unpin' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;package;rename' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;package;add-entry' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;package;rm-entry' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;package;help' {
            [CompletionResult]::new('pin', 'pin', [CompletionResultType]::ParameterValue, 'Mark an installed package as pinned')
            [CompletionResult]::new('unpin', 'unpin', [CompletionResultType]::ParameterValue, 'Clear the pinned flag on an installed package')
            [CompletionResult]::new('rename', 'rename', [CompletionResultType]::ParameterValue, 'Rename an installed package record and aliases')
            [CompletionResult]::new('add-entry', 'add-entry', [CompletionResultType]::ParameterValue, 'Add a desktop launcher entry for an installed package')
            [CompletionResult]::new('rm-entry', 'rm-entry', [CompletionResultType]::ParameterValue, 'Remove an upstream-managed desktop launcher entry')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'upstream;package;help;pin' {
            break
        }
        'upstream;package;help;unpin' {
            break
        }
        'upstream;package;help;rename' {
            break
        }
        'upstream;package;help;add-entry' {
            break
        }
        'upstream;package;help;rm-entry' {
            break
        }
        'upstream;package;help;help' {
            break
        }
        'upstream;hooks' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Install shell PATH hooks')
            [CompletionResult]::new('check', 'check', [CompletionResultType]::ParameterValue, 'Check shell PATH hooks')
            [CompletionResult]::new('clean', 'clean', [CompletionResultType]::ParameterValue, 'Remove shell PATH hooks')
            [CompletionResult]::new('purge', 'purge', [CompletionResultType]::ParameterValue, 'Remove hooks and delete the local upstream data directory')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'upstream;hooks;init' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;hooks;check' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;hooks;clean' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;hooks;purge' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;hooks;help' {
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Install shell PATH hooks')
            [CompletionResult]::new('check', 'check', [CompletionResultType]::ParameterValue, 'Check shell PATH hooks')
            [CompletionResult]::new('clean', 'clean', [CompletionResultType]::ParameterValue, 'Remove shell PATH hooks')
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
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'Replace config.toml from an export')
            [CompletionResult]::new('keys', 'keys', [CompletionResultType]::ParameterValue, 'Import trusted minisign or cosign public keys')
            [CompletionResult]::new('packages', 'packages', [CompletionResultType]::ParameterValue, 'Install packages from an exported package list')
            [CompletionResult]::new('profile', 'profile', [CompletionResultType]::ParameterValue, 'Import config, keys, and packages from a profile')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'upstream;import;config' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;import;keys' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;import;packages' {
            [CompletionResult]::new('--skip-failed', '--skip-failed', [CompletionResultType]::ParameterName, 'Continue installing remaining packages after a package import fails')
            [CompletionResult]::new('--latest', '--latest', [CompletionResultType]::ParameterName, 'Ignore exported version tags and install latest releases')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;import;profile' {
            [CompletionResult]::new('--skip-failed', '--skip-failed', [CompletionResultType]::ParameterName, 'Continue installing remaining packages after a package import fails')
            [CompletionResult]::new('--latest', '--latest', [CompletionResultType]::ParameterName, 'Ignore exported package version tags and install latest releases')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;import;help' {
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'Replace config.toml from an export')
            [CompletionResult]::new('keys', 'keys', [CompletionResultType]::ParameterValue, 'Import trusted minisign or cosign public keys')
            [CompletionResult]::new('packages', 'packages', [CompletionResultType]::ParameterValue, 'Install packages from an exported package list')
            [CompletionResult]::new('profile', 'profile', [CompletionResultType]::ParameterValue, 'Import config, keys, and packages from a profile')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'upstream;import;help;config' {
            break
        }
        'upstream;import;help;keys' {
            break
        }
        'upstream;import;help;packages' {
            break
        }
        'upstream;import;help;profile' {
            break
        }
        'upstream;import;help;help' {
            break
        }
        'upstream;export' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'Export config.toml')
            [CompletionResult]::new('keys', 'keys', [CompletionResultType]::ParameterValue, 'Export trusted minisign and cosign public keys')
            [CompletionResult]::new('packages', 'packages', [CompletionResultType]::ParameterValue, 'Export installed release-package references')
            [CompletionResult]::new('profile', 'profile', [CompletionResultType]::ParameterValue, 'Export config, trust keys, and package references')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'upstream;export;config' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;export;keys' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;export;packages' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;export;profile' {
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;export;help' {
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'Export config.toml')
            [CompletionResult]::new('keys', 'keys', [CompletionResultType]::ParameterValue, 'Export trusted minisign and cosign public keys')
            [CompletionResult]::new('packages', 'packages', [CompletionResultType]::ParameterValue, 'Export installed release-package references')
            [CompletionResult]::new('profile', 'profile', [CompletionResultType]::ParameterValue, 'Export config, trust keys, and package references')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'upstream;export;help;config' {
            break
        }
        'upstream;export;help;keys' {
            break
        }
        'upstream;export;help;packages' {
            break
        }
        'upstream;export;help;profile' {
            break
        }
        'upstream;export;help;help' {
            break
        }
        'upstream;doctor' {
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Print each check result line in addition to summary output')
            [CompletionResult]::new('--fix', '--fix', [CompletionResultType]::ParameterName, 'Attempt automatic repairs for detected issues')
            [CompletionResult]::new('--migrate', '--migrate', [CompletionResultType]::ParameterName, 'Migrate local upstream data after breaking layout or metadata changes')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Print diagnostic report as JSON')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept confirmation prompts automatically')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'upstream;help' {
            [CompletionResult]::new('install', 'install', [CompletionResultType]::ParameterValue, 'Install a release asset or direct download')
            [CompletionResult]::new('build', 'build', [CompletionResultType]::ParameterValue, 'Build and install a package from source')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove installed package files and metadata')
            [CompletionResult]::new('rollback', 'rollback', [CompletionResultType]::ParameterValue, 'Restore or prune stored rollback artifacts')
            [CompletionResult]::new('reinstall', 'reinstall', [CompletionResultType]::ParameterValue, 'Reinstall packages from their stored source metadata')
            [CompletionResult]::new('upgrade', 'upgrade', [CompletionResultType]::ParameterValue, 'Check for or install package updates')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List installed packages')
            [CompletionResult]::new('info', 'info', [CompletionResultType]::ParameterValue, 'Show details for one installed package')
            [CompletionResult]::new('changelog', 'changelog', [CompletionResultType]::ParameterValue, 'Show release notes for an installed package')
            [CompletionResult]::new('docs', 'docs', [CompletionResultType]::ParameterValue, 'Search cached or fetched package README docs')
            [CompletionResult]::new('probe', 'probe', [CompletionResultType]::ParameterValue, 'Inspect releases, choose an asset, and install it')
            [CompletionResult]::new('search', 'search', [CompletionResultType]::ParameterValue, 'Search provider repositories without installing')
            [CompletionResult]::new('find', 'find', [CompletionResultType]::ParameterValue, 'Search repositories interactively and install one')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'View, edit, and validate config.toml')
            [CompletionResult]::new('package', 'package', [CompletionResultType]::ParameterValue, 'Manage installed package records and launcher entries')
            [CompletionResult]::new('hooks', 'hooks', [CompletionResultType]::ParameterValue, 'Manage shell PATH hooks and local upstream data')
            [CompletionResult]::new('import', 'import', [CompletionResultType]::ParameterValue, 'Import config, trust keys, packages, or a profile')
            [CompletionResult]::new('export', 'export', [CompletionResultType]::ParameterValue, 'Export config, trust keys, packages, or a profile')
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
        'upstream;help;rollback' {
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
        'upstream;help;info' {
            break
        }
        'upstream;help;changelog' {
            break
        }
        'upstream;help;docs' {
            break
        }
        'upstream;help;probe' {
            break
        }
        'upstream;help;search' {
            break
        }
        'upstream;help;find' {
            break
        }
        'upstream;help;config' {
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Set configuration values')
            [CompletionResult]::new('get', 'get', [CompletionResultType]::ParameterValue, 'Get configuration values')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List current configuration values')
            [CompletionResult]::new('verify', 'verify', [CompletionResultType]::ParameterValue, 'Check config.toml for missing or unused keys')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Open config.toml in your default editor')
            [CompletionResult]::new('reset', 'reset', [CompletionResultType]::ParameterValue, 'Reset config.toml to defaults')
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
        'upstream;help;config;verify' {
            break
        }
        'upstream;help;config;edit' {
            break
        }
        'upstream;help;config;reset' {
            break
        }
        'upstream;help;package' {
            [CompletionResult]::new('pin', 'pin', [CompletionResultType]::ParameterValue, 'Mark an installed package as pinned')
            [CompletionResult]::new('unpin', 'unpin', [CompletionResultType]::ParameterValue, 'Clear the pinned flag on an installed package')
            [CompletionResult]::new('rename', 'rename', [CompletionResultType]::ParameterValue, 'Rename an installed package record and aliases')
            [CompletionResult]::new('add-entry', 'add-entry', [CompletionResultType]::ParameterValue, 'Add a desktop launcher entry for an installed package')
            [CompletionResult]::new('rm-entry', 'rm-entry', [CompletionResultType]::ParameterValue, 'Remove an upstream-managed desktop launcher entry')
            break
        }
        'upstream;help;package;pin' {
            break
        }
        'upstream;help;package;unpin' {
            break
        }
        'upstream;help;package;rename' {
            break
        }
        'upstream;help;package;add-entry' {
            break
        }
        'upstream;help;package;rm-entry' {
            break
        }
        'upstream;help;hooks' {
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Install shell PATH hooks')
            [CompletionResult]::new('check', 'check', [CompletionResultType]::ParameterValue, 'Check shell PATH hooks')
            [CompletionResult]::new('clean', 'clean', [CompletionResultType]::ParameterValue, 'Remove shell PATH hooks')
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
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'Replace config.toml from an export')
            [CompletionResult]::new('keys', 'keys', [CompletionResultType]::ParameterValue, 'Import trusted minisign or cosign public keys')
            [CompletionResult]::new('packages', 'packages', [CompletionResultType]::ParameterValue, 'Install packages from an exported package list')
            [CompletionResult]::new('profile', 'profile', [CompletionResultType]::ParameterValue, 'Import config, keys, and packages from a profile')
            break
        }
        'upstream;help;import;config' {
            break
        }
        'upstream;help;import;keys' {
            break
        }
        'upstream;help;import;packages' {
            break
        }
        'upstream;help;import;profile' {
            break
        }
        'upstream;help;export' {
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'Export config.toml')
            [CompletionResult]::new('keys', 'keys', [CompletionResultType]::ParameterValue, 'Export trusted minisign and cosign public keys')
            [CompletionResult]::new('packages', 'packages', [CompletionResultType]::ParameterValue, 'Export installed release-package references')
            [CompletionResult]::new('profile', 'profile', [CompletionResultType]::ParameterValue, 'Export config, trust keys, and package references')
            break
        }
        'upstream;help;export;config' {
            break
        }
        'upstream;help;export;keys' {
            break
        }
        'upstream;help;export;packages' {
            break
        }
        'upstream;help;export;profile' {
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
