#compdef upstream

autoload -U is-at-least

_upstream() {
    typeset -A opt_args
    typeset -a _arguments_options
    local ret=1

    if is-at-least 5.2; then
        _arguments_options=(-s -S -C)
    else
        _arguments_options=(-s -C)
    fi

    local context curcontext="$curcontext" state line
    _arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'-V[Print version]' \
'--version[Print version]' \
":: :_upstream_commands" \
"*::: :->upstream" \
&& ret=0
    case $state in
    (upstream)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-command-$line[1]:"
        case $line[1] in
            (install)
_arguments "${_arguments_options[@]}" : \
'-t+[Version tag to install (defaults to latest)]:TAG:_default' \
'--tag=[Version tag to install (defaults to latest)]:TAG:_default' \
'-k+[File type to install]:KIND:(app-image mac-app mac-dmg archive compressed binary win-exe checksum auto)' \
'--kind=[File type to install]:KIND:(app-image mac-app mac-dmg archive compressed binary win-exe checksum auto)' \
'-p+[Source provider hosting the repository. Defaults to auto-detection]:PROVIDER:_default' \
'--provider=[Source provider hosting the repository. Defaults to auto-detection]:PROVIDER:_default' \
'--base-url=[Custom base URL. Defaults to provider'\''s root]:BASE_URL:_default' \
'-c+[Update channel to track]:CHANNEL:(stable preview nightly)' \
'--channel=[Update channel to track]:CHANNEL:(stable preview nightly)' \
'-m+[Match pattern to use as a hint for which asset to prefer]:match:_default' \
'--match-pattern=[Match pattern to use as a hint for which asset to prefer]:match:_default' \
'-e+[Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")]:exclude:_default' \
'--exclude-pattern=[Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")]:exclude:_default' \
'--trust=[Trust verification mode for downloaded assets]:TRUST_MODE:(none best-effort checksum signature all)' \
'-d[Whether or not to create a .desktop entry for GUI applications]' \
'--desktop[Whether or not to create a .desktop entry for GUI applications]' \
'--dry-run[Preview install resolution without downloading or writing files]' \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':name -- Name to register the application under:_default' \
':repo_slug -- Repository identifier or URL:_default' \
&& ret=0
;;
(build)
_arguments "${_arguments_options[@]}" : \
'(--branch)-t+[Version tag to build (defaults to latest)]:TAG:_default' \
'(--branch)--tag=[Version tag to build (defaults to latest)]:TAG:_default' \
'(-t --tag)--branch=[Branch name to build from (uses latest commit from that branch)]:BRANCH:_default' \
'-p+[Source provider hosting the repository. Defaults to auto-detection]:PROVIDER:_default' \
'--provider=[Source provider hosting the repository. Defaults to auto-detection]:PROVIDER:_default' \
'--base-url=[Custom base URL. Defaults to provider'\''s root]:BASE_URL:_default' \
'-c+[Update channel to track]:CHANNEL:(stable preview nightly)' \
'--channel=[Update channel to track]:CHANNEL:(stable preview nightly)' \
'--build-profile=[Build profile used to compile/install from source (auto-detected when omitted)]:BUILD_PROFILE:(rust dotnet go zig cmake)' \
'-d[Whether or not to create a .desktop entry for GUI applications]' \
'--desktop[Whether or not to create a .desktop entry for GUI applications]' \
'--dry-run[Preview build resolution without compiling or writing files]' \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':name -- Name to register the application under:_default' \
':repo_slug -- Repository identifier or URL:_default' \
&& ret=0
;;
(remove)
_arguments "${_arguments_options[@]}" : \
'--purge[Remove all associated cached data]' \
'--force[Ignore uninstall errors and remove metadata anyway]' \
'--dry-run[Preview removal actions without deleting files or metadata]' \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'*::names -- Names of packages to remove:_default' \
&& ret=0
;;
(rollback)
_arguments "${_arguments_options[@]}" : \
'--prune[Prune rollback artifacts instead of restoring]' \
'--dry-run[Preview rollback/prune actions without modifying files or metadata]' \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'*::names -- Package names to restore or prune:_default' \
&& ret=0
;;
(reinstall)
_arguments "${_arguments_options[@]}" : \
'--trust=[Trust verification mode for release-asset reinstalls]:TRUST_MODE:(none best-effort checksum signature all)' \
'--force[Ignore uninstall errors and remove metadata anyway before reinstalling]' \
'--dry-run[Preview reinstall resolution without removing, building, or writing files]' \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'*::names -- Names of packages to reinstall:_default' \
&& ret=0
;;
(upgrade)
_arguments "${_arguments_options[@]}" : \
'--trust=[Trust verification mode for downloaded assets]:TRUST_MODE:(none best-effort checksum signature all)' \
'--force[Force upgrade even if already up to date]' \
'--check[Check for available upgrades without applying them]' \
'--machine-readable[Use script-friendly check output\: one line per update, "name oldver newver"]' \
'--dry-run[Preview upgrade resolution without downloading or writing files]' \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'*::names -- Packages to upgrade (upgrades all if omitted):_default' \
&& ret=0
;;
(list)
_arguments "${_arguments_options[@]}" : \
'--json[Print raw package metadata as JSON]' \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'::name -- Package name for detailed information:_default' \
&& ret=0
;;
(changelog)
_arguments "${_arguments_options[@]}" : \
'--from=[Override the starting release tag]:FROM_TAG:_default' \
'--to=[Override the ending release tag]:TO_TAG:_default' \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':name -- Installed package name:_default' \
&& ret=0
;;
(probe)
_arguments "${_arguments_options[@]}" : \
'-p+[Source provider (defaults to github, or scraper for URLs)]:PROVIDER:_default' \
'--provider=[Source provider (defaults to github, or scraper for URLs)]:PROVIDER:_default' \
'--base-url=[Custom base URL for self-hosted providers]:BASE_URL:_default' \
'-c+[Channel view to display]:CHANNEL:(stable preview nightly)' \
'--channel=[Channel view to display]:CHANNEL:(stable preview nightly)' \
'--limit=[Maximum number of releases to display]:LIMIT:_default' \
'--verbose[Include scored candidate assets for each release]' \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':repo_slug -- Repository identifier or URL to probe:_default' \
&& ret=0
;;
(search)
_arguments "${_arguments_options[@]}" : \
'-p+[Source provider to search (defaults to github)]:PROVIDER:_default' \
'--provider=[Source provider to search (defaults to github)]:PROVIDER:_default' \
'--base-url=[Custom base URL for self-hosted providers]:BASE_URL:_default' \
'--limit=[Maximum number of results to display]:LIMIT:_default' \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'*::query_words -- Query words (joined with spaces):_default' \
&& ret=0
;;
(config)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
":: :_upstream__subcmd__config_commands" \
"*::: :->config" \
&& ret=0

    case $state in
    (config)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-config-command-$line[1]:"
        case $line[1] in
            (set)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'*::keys -- Configuration assignments (format\: key.path=value):_default' \
&& ret=0
;;
(get)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'*::keys -- Configuration keys to retrieve (format\: key.path):_default' \
&& ret=0
;;
(list)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(edit)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(reset)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_upstream__subcmd__config__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-config-help-command-$line[1]:"
        case $line[1] in
            (set)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(get)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(list)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(edit)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(reset)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(package)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
":: :_upstream__subcmd__package_commands" \
"*::: :->package" \
&& ret=0

    case $state in
    (package)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-package-command-$line[1]:"
        case $line[1] in
            (pin)
_arguments "${_arguments_options[@]}" : \
'--reason=[Optional reason for pinning this package]:REASON:_default' \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':name -- Name of package to pin:_default' \
&& ret=0
;;
(unpin)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':name -- Name of package to unpin:_default' \
&& ret=0
;;
(rename)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':old_name -- Existing package alias:_default' \
':new_name -- New package alias:_default' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_upstream__subcmd__package__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-package-help-command-$line[1]:"
        case $line[1] in
            (pin)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(unpin)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(rename)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(hooks)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
":: :_upstream__subcmd__hooks_commands" \
"*::: :->hooks" \
&& ret=0

    case $state in
    (hooks)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-hooks-command-$line[1]:"
        case $line[1] in
            (init)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(check)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(clean)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(purge)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_upstream__subcmd__hooks__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-hooks-help-command-$line[1]:"
        case $line[1] in
            (init)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(check)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(clean)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(purge)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(import)
_arguments "${_arguments_options[@]}" : \
'--as=[Force the input type instead of autodetection]:IMPORT_AS:(keys manifest snapshot)' \
'--skip-failed[Continue importing remaining entries when metadata manifest processing fails]' \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':path -- Path to a keys file, metadata manifest, or snapshot archive:_files' \
&& ret=0
;;
(export)
_arguments "${_arguments_options[@]}" : \
'--full[Export a full snapshot of the upstream directory instead of a manifest]' \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':path -- Output path for the manifest or snapshot archive:_files' \
&& ret=0
;;
(doctor)
_arguments "${_arguments_options[@]}" : \
'--verbose[Print each check result line in addition to summary output]' \
'--fix[Attempt automatic repairs for detected issues]' \
'-y[Accept confirmation prompts]' \
'--yes[Accept confirmation prompts]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'*::names -- Package names to check (all installed packages if omitted):_default' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_upstream__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-help-command-$line[1]:"
        case $line[1] in
            (install)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(build)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(remove)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(rollback)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(reinstall)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(upgrade)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(list)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(changelog)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(probe)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(search)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(config)
_arguments "${_arguments_options[@]}" : \
":: :_upstream__subcmd__help__subcmd__config_commands" \
"*::: :->config" \
&& ret=0

    case $state in
    (config)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-help-config-command-$line[1]:"
        case $line[1] in
            (set)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(get)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(list)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(edit)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(reset)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(package)
_arguments "${_arguments_options[@]}" : \
":: :_upstream__subcmd__help__subcmd__package_commands" \
"*::: :->package" \
&& ret=0

    case $state in
    (package)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-help-package-command-$line[1]:"
        case $line[1] in
            (pin)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(unpin)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(rename)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(hooks)
_arguments "${_arguments_options[@]}" : \
":: :_upstream__subcmd__help__subcmd__hooks_commands" \
"*::: :->hooks" \
&& ret=0

    case $state in
    (hooks)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-help-hooks-command-$line[1]:"
        case $line[1] in
            (init)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(check)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(clean)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(purge)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(import)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(export)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(doctor)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
}

(( $+functions[_upstream_commands] )) ||
_upstream_commands() {
    local commands; commands=(
'install:Install a package from an upstream release source' \
'build:Build and install from source for release tags without artifacts' \
'remove:Remove one or more installed packages' \
'rollback:Restore or prune stored rollback artifacts' \
'reinstall:Reinstall one or more packages (remove then install)' \
'upgrade:Upgrade installed packages to their latest versions' \
'list:List installed packages and their metadata' \
'changelog:Show upstream release notes for an installed package' \
'probe:Inspect releases visible from a provider without installing' \
'search:Search provider repositories by keyword(s)' \
'config:Manage upstream configuration' \
'package:Manage package-specific behavior' \
'hooks:Manage shell integration hooks and local upstream data' \
'import:Import trusted keys, package metadata manifests, or full snapshots' \
'export:Export packages to a manifest or full snapshot' \
'doctor:Run diagnostics to detect installation and integration issues' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'upstream commands' commands "$@"
}
(( $+functions[_upstream__subcmd__build_commands] )) ||
_upstream__subcmd__build_commands() {
    local commands; commands=()
    _describe -t commands 'upstream build commands' commands "$@"
}
(( $+functions[_upstream__subcmd__changelog_commands] )) ||
_upstream__subcmd__changelog_commands() {
    local commands; commands=()
    _describe -t commands 'upstream changelog commands' commands "$@"
}
(( $+functions[_upstream__subcmd__config_commands] )) ||
_upstream__subcmd__config_commands() {
    local commands; commands=(
'set:Set configuration values' \
'get:Get configuration values' \
'list:List all configuration keys' \
'edit:Open configuration file in your default editor' \
'reset:Reset configuration to defaults' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'upstream config commands' commands "$@"
}
(( $+functions[_upstream__subcmd__config__subcmd__edit_commands] )) ||
_upstream__subcmd__config__subcmd__edit_commands() {
    local commands; commands=()
    _describe -t commands 'upstream config edit commands' commands "$@"
}
(( $+functions[_upstream__subcmd__config__subcmd__get_commands] )) ||
_upstream__subcmd__config__subcmd__get_commands() {
    local commands; commands=()
    _describe -t commands 'upstream config get commands' commands "$@"
}
(( $+functions[_upstream__subcmd__config__subcmd__help_commands] )) ||
_upstream__subcmd__config__subcmd__help_commands() {
    local commands; commands=(
'set:Set configuration values' \
'get:Get configuration values' \
'list:List all configuration keys' \
'edit:Open configuration file in your default editor' \
'reset:Reset configuration to defaults' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'upstream config help commands' commands "$@"
}
(( $+functions[_upstream__subcmd__config__subcmd__help__subcmd__edit_commands] )) ||
_upstream__subcmd__config__subcmd__help__subcmd__edit_commands() {
    local commands; commands=()
    _describe -t commands 'upstream config help edit commands' commands "$@"
}
(( $+functions[_upstream__subcmd__config__subcmd__help__subcmd__get_commands] )) ||
_upstream__subcmd__config__subcmd__help__subcmd__get_commands() {
    local commands; commands=()
    _describe -t commands 'upstream config help get commands' commands "$@"
}
(( $+functions[_upstream__subcmd__config__subcmd__help__subcmd__help_commands] )) ||
_upstream__subcmd__config__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'upstream config help help commands' commands "$@"
}
(( $+functions[_upstream__subcmd__config__subcmd__help__subcmd__list_commands] )) ||
_upstream__subcmd__config__subcmd__help__subcmd__list_commands() {
    local commands; commands=()
    _describe -t commands 'upstream config help list commands' commands "$@"
}
(( $+functions[_upstream__subcmd__config__subcmd__help__subcmd__reset_commands] )) ||
_upstream__subcmd__config__subcmd__help__subcmd__reset_commands() {
    local commands; commands=()
    _describe -t commands 'upstream config help reset commands' commands "$@"
}
(( $+functions[_upstream__subcmd__config__subcmd__help__subcmd__set_commands] )) ||
_upstream__subcmd__config__subcmd__help__subcmd__set_commands() {
    local commands; commands=()
    _describe -t commands 'upstream config help set commands' commands "$@"
}
(( $+functions[_upstream__subcmd__config__subcmd__list_commands] )) ||
_upstream__subcmd__config__subcmd__list_commands() {
    local commands; commands=()
    _describe -t commands 'upstream config list commands' commands "$@"
}
(( $+functions[_upstream__subcmd__config__subcmd__reset_commands] )) ||
_upstream__subcmd__config__subcmd__reset_commands() {
    local commands; commands=()
    _describe -t commands 'upstream config reset commands' commands "$@"
}
(( $+functions[_upstream__subcmd__config__subcmd__set_commands] )) ||
_upstream__subcmd__config__subcmd__set_commands() {
    local commands; commands=()
    _describe -t commands 'upstream config set commands' commands "$@"
}
(( $+functions[_upstream__subcmd__doctor_commands] )) ||
_upstream__subcmd__doctor_commands() {
    local commands; commands=()
    _describe -t commands 'upstream doctor commands' commands "$@"
}
(( $+functions[_upstream__subcmd__export_commands] )) ||
_upstream__subcmd__export_commands() {
    local commands; commands=()
    _describe -t commands 'upstream export commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help_commands] )) ||
_upstream__subcmd__help_commands() {
    local commands; commands=(
'install:Install a package from an upstream release source' \
'build:Build and install from source for release tags without artifacts' \
'remove:Remove one or more installed packages' \
'rollback:Restore or prune stored rollback artifacts' \
'reinstall:Reinstall one or more packages (remove then install)' \
'upgrade:Upgrade installed packages to their latest versions' \
'list:List installed packages and their metadata' \
'changelog:Show upstream release notes for an installed package' \
'probe:Inspect releases visible from a provider without installing' \
'search:Search provider repositories by keyword(s)' \
'config:Manage upstream configuration' \
'package:Manage package-specific behavior' \
'hooks:Manage shell integration hooks and local upstream data' \
'import:Import trusted keys, package metadata manifests, or full snapshots' \
'export:Export packages to a manifest or full snapshot' \
'doctor:Run diagnostics to detect installation and integration issues' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'upstream help commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__build_commands] )) ||
_upstream__subcmd__help__subcmd__build_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help build commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__changelog_commands] )) ||
_upstream__subcmd__help__subcmd__changelog_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help changelog commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__config_commands] )) ||
_upstream__subcmd__help__subcmd__config_commands() {
    local commands; commands=(
'set:Set configuration values' \
'get:Get configuration values' \
'list:List all configuration keys' \
'edit:Open configuration file in your default editor' \
'reset:Reset configuration to defaults' \
    )
    _describe -t commands 'upstream help config commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__config__subcmd__edit_commands] )) ||
_upstream__subcmd__help__subcmd__config__subcmd__edit_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help config edit commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__config__subcmd__get_commands] )) ||
_upstream__subcmd__help__subcmd__config__subcmd__get_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help config get commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__config__subcmd__list_commands] )) ||
_upstream__subcmd__help__subcmd__config__subcmd__list_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help config list commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__config__subcmd__reset_commands] )) ||
_upstream__subcmd__help__subcmd__config__subcmd__reset_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help config reset commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__config__subcmd__set_commands] )) ||
_upstream__subcmd__help__subcmd__config__subcmd__set_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help config set commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__doctor_commands] )) ||
_upstream__subcmd__help__subcmd__doctor_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help doctor commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__export_commands] )) ||
_upstream__subcmd__help__subcmd__export_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help export commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__help_commands] )) ||
_upstream__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help help commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__hooks_commands] )) ||
_upstream__subcmd__help__subcmd__hooks_commands() {
    local commands; commands=(
'init:Add upstream shell integration hooks' \
'check:Check upstream shell integration hooks' \
'clean:Remove upstream shell integration hooks' \
'purge:Remove hooks and delete the local upstream data directory' \
    )
    _describe -t commands 'upstream help hooks commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__hooks__subcmd__check_commands] )) ||
_upstream__subcmd__help__subcmd__hooks__subcmd__check_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help hooks check commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__hooks__subcmd__clean_commands] )) ||
_upstream__subcmd__help__subcmd__hooks__subcmd__clean_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help hooks clean commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__hooks__subcmd__init_commands] )) ||
_upstream__subcmd__help__subcmd__hooks__subcmd__init_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help hooks init commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__hooks__subcmd__purge_commands] )) ||
_upstream__subcmd__help__subcmd__hooks__subcmd__purge_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help hooks purge commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__import_commands] )) ||
_upstream__subcmd__help__subcmd__import_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help import commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__install_commands] )) ||
_upstream__subcmd__help__subcmd__install_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help install commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__list_commands] )) ||
_upstream__subcmd__help__subcmd__list_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help list commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__package_commands] )) ||
_upstream__subcmd__help__subcmd__package_commands() {
    local commands; commands=(
'pin:Pin a package to its current version' \
'unpin:Unpin a package to allow updates' \
'rename:Rename package alias without reinstalling' \
    )
    _describe -t commands 'upstream help package commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__package__subcmd__pin_commands] )) ||
_upstream__subcmd__help__subcmd__package__subcmd__pin_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help package pin commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__package__subcmd__rename_commands] )) ||
_upstream__subcmd__help__subcmd__package__subcmd__rename_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help package rename commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__package__subcmd__unpin_commands] )) ||
_upstream__subcmd__help__subcmd__package__subcmd__unpin_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help package unpin commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__probe_commands] )) ||
_upstream__subcmd__help__subcmd__probe_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help probe commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__reinstall_commands] )) ||
_upstream__subcmd__help__subcmd__reinstall_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help reinstall commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__remove_commands] )) ||
_upstream__subcmd__help__subcmd__remove_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help remove commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__rollback_commands] )) ||
_upstream__subcmd__help__subcmd__rollback_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help rollback commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__search_commands] )) ||
_upstream__subcmd__help__subcmd__search_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help search commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__upgrade_commands] )) ||
_upstream__subcmd__help__subcmd__upgrade_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help upgrade commands' commands "$@"
}
(( $+functions[_upstream__subcmd__hooks_commands] )) ||
_upstream__subcmd__hooks_commands() {
    local commands; commands=(
'init:Add upstream shell integration hooks' \
'check:Check upstream shell integration hooks' \
'clean:Remove upstream shell integration hooks' \
'purge:Remove hooks and delete the local upstream data directory' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'upstream hooks commands' commands "$@"
}
(( $+functions[_upstream__subcmd__hooks__subcmd__check_commands] )) ||
_upstream__subcmd__hooks__subcmd__check_commands() {
    local commands; commands=()
    _describe -t commands 'upstream hooks check commands' commands "$@"
}
(( $+functions[_upstream__subcmd__hooks__subcmd__clean_commands] )) ||
_upstream__subcmd__hooks__subcmd__clean_commands() {
    local commands; commands=()
    _describe -t commands 'upstream hooks clean commands' commands "$@"
}
(( $+functions[_upstream__subcmd__hooks__subcmd__help_commands] )) ||
_upstream__subcmd__hooks__subcmd__help_commands() {
    local commands; commands=(
'init:Add upstream shell integration hooks' \
'check:Check upstream shell integration hooks' \
'clean:Remove upstream shell integration hooks' \
'purge:Remove hooks and delete the local upstream data directory' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'upstream hooks help commands' commands "$@"
}
(( $+functions[_upstream__subcmd__hooks__subcmd__help__subcmd__check_commands] )) ||
_upstream__subcmd__hooks__subcmd__help__subcmd__check_commands() {
    local commands; commands=()
    _describe -t commands 'upstream hooks help check commands' commands "$@"
}
(( $+functions[_upstream__subcmd__hooks__subcmd__help__subcmd__clean_commands] )) ||
_upstream__subcmd__hooks__subcmd__help__subcmd__clean_commands() {
    local commands; commands=()
    _describe -t commands 'upstream hooks help clean commands' commands "$@"
}
(( $+functions[_upstream__subcmd__hooks__subcmd__help__subcmd__help_commands] )) ||
_upstream__subcmd__hooks__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'upstream hooks help help commands' commands "$@"
}
(( $+functions[_upstream__subcmd__hooks__subcmd__help__subcmd__init_commands] )) ||
_upstream__subcmd__hooks__subcmd__help__subcmd__init_commands() {
    local commands; commands=()
    _describe -t commands 'upstream hooks help init commands' commands "$@"
}
(( $+functions[_upstream__subcmd__hooks__subcmd__help__subcmd__purge_commands] )) ||
_upstream__subcmd__hooks__subcmd__help__subcmd__purge_commands() {
    local commands; commands=()
    _describe -t commands 'upstream hooks help purge commands' commands "$@"
}
(( $+functions[_upstream__subcmd__hooks__subcmd__init_commands] )) ||
_upstream__subcmd__hooks__subcmd__init_commands() {
    local commands; commands=()
    _describe -t commands 'upstream hooks init commands' commands "$@"
}
(( $+functions[_upstream__subcmd__hooks__subcmd__purge_commands] )) ||
_upstream__subcmd__hooks__subcmd__purge_commands() {
    local commands; commands=()
    _describe -t commands 'upstream hooks purge commands' commands "$@"
}
(( $+functions[_upstream__subcmd__import_commands] )) ||
_upstream__subcmd__import_commands() {
    local commands; commands=()
    _describe -t commands 'upstream import commands' commands "$@"
}
(( $+functions[_upstream__subcmd__install_commands] )) ||
_upstream__subcmd__install_commands() {
    local commands; commands=()
    _describe -t commands 'upstream install commands' commands "$@"
}
(( $+functions[_upstream__subcmd__list_commands] )) ||
_upstream__subcmd__list_commands() {
    local commands; commands=()
    _describe -t commands 'upstream list commands' commands "$@"
}
(( $+functions[_upstream__subcmd__package_commands] )) ||
_upstream__subcmd__package_commands() {
    local commands; commands=(
'pin:Pin a package to its current version' \
'unpin:Unpin a package to allow updates' \
'rename:Rename package alias without reinstalling' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'upstream package commands' commands "$@"
}
(( $+functions[_upstream__subcmd__package__subcmd__help_commands] )) ||
_upstream__subcmd__package__subcmd__help_commands() {
    local commands; commands=(
'pin:Pin a package to its current version' \
'unpin:Unpin a package to allow updates' \
'rename:Rename package alias without reinstalling' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'upstream package help commands' commands "$@"
}
(( $+functions[_upstream__subcmd__package__subcmd__help__subcmd__help_commands] )) ||
_upstream__subcmd__package__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'upstream package help help commands' commands "$@"
}
(( $+functions[_upstream__subcmd__package__subcmd__help__subcmd__pin_commands] )) ||
_upstream__subcmd__package__subcmd__help__subcmd__pin_commands() {
    local commands; commands=()
    _describe -t commands 'upstream package help pin commands' commands "$@"
}
(( $+functions[_upstream__subcmd__package__subcmd__help__subcmd__rename_commands] )) ||
_upstream__subcmd__package__subcmd__help__subcmd__rename_commands() {
    local commands; commands=()
    _describe -t commands 'upstream package help rename commands' commands "$@"
}
(( $+functions[_upstream__subcmd__package__subcmd__help__subcmd__unpin_commands] )) ||
_upstream__subcmd__package__subcmd__help__subcmd__unpin_commands() {
    local commands; commands=()
    _describe -t commands 'upstream package help unpin commands' commands "$@"
}
(( $+functions[_upstream__subcmd__package__subcmd__pin_commands] )) ||
_upstream__subcmd__package__subcmd__pin_commands() {
    local commands; commands=()
    _describe -t commands 'upstream package pin commands' commands "$@"
}
(( $+functions[_upstream__subcmd__package__subcmd__rename_commands] )) ||
_upstream__subcmd__package__subcmd__rename_commands() {
    local commands; commands=()
    _describe -t commands 'upstream package rename commands' commands "$@"
}
(( $+functions[_upstream__subcmd__package__subcmd__unpin_commands] )) ||
_upstream__subcmd__package__subcmd__unpin_commands() {
    local commands; commands=()
    _describe -t commands 'upstream package unpin commands' commands "$@"
}
(( $+functions[_upstream__subcmd__probe_commands] )) ||
_upstream__subcmd__probe_commands() {
    local commands; commands=()
    _describe -t commands 'upstream probe commands' commands "$@"
}
(( $+functions[_upstream__subcmd__reinstall_commands] )) ||
_upstream__subcmd__reinstall_commands() {
    local commands; commands=()
    _describe -t commands 'upstream reinstall commands' commands "$@"
}
(( $+functions[_upstream__subcmd__remove_commands] )) ||
_upstream__subcmd__remove_commands() {
    local commands; commands=()
    _describe -t commands 'upstream remove commands' commands "$@"
}
(( $+functions[_upstream__subcmd__rollback_commands] )) ||
_upstream__subcmd__rollback_commands() {
    local commands; commands=()
    _describe -t commands 'upstream rollback commands' commands "$@"
}
(( $+functions[_upstream__subcmd__search_commands] )) ||
_upstream__subcmd__search_commands() {
    local commands; commands=()
    _describe -t commands 'upstream search commands' commands "$@"
}
(( $+functions[_upstream__subcmd__upgrade_commands] )) ||
_upstream__subcmd__upgrade_commands() {
    local commands; commands=()
    _describe -t commands 'upstream upgrade commands' commands "$@"
}

if [ "$funcstack[1]" = "_upstream" ]; then
    _upstream "$@"
else
    compdef _upstream upstream
fi
