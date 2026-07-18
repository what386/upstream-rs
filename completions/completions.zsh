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
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
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
'-t+[Release tag to install (defaults to latest matching the channel)]:TAG:_default' \
'--tag=[Release tag to install (defaults to latest matching the channel)]:TAG:_default' \
'-k+[Asset kind to install]:KIND:(app-image mac-app mac-dmg archive compressed binary win-exe checksum auto)' \
'--kind=[Asset kind to install]:KIND:(app-image mac-app mac-dmg archive compressed binary win-exe checksum auto)' \
'-p+[Source provider hosting the repository. Defaults to auto-detection]:PROVIDER:_default' \
'--provider=[Source provider hosting the repository. Defaults to auto-detection]:PROVIDER:_default' \
'--base-url=[Custom base URL. Defaults to provider'\''s root]:BASE_URL:_default' \
'-c+[Release channel to track for upgrades]:CHANNEL:(stable preview nightly)' \
'--channel=[Release channel to track for upgrades]:CHANNEL:(stable preview nightly)' \
'-m+[Match pattern to use as a hint for which asset to prefer]:match:_default' \
'--match-pattern=[Match pattern to use as a hint for which asset to prefer]:match:_default' \
'-e+[Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")]:exclude:_default' \
'--exclude-pattern=[Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")]:exclude:_default' \
'--trust=[Trust verification mode for downloaded assets]:TRUST_MODE:(none best-effort checksum signature all)' \
'-d[Create a desktop launcher entry for GUI applications]' \
'--desktop[Create a desktop launcher entry for GUI applications]' \
'--dry-run[Preview install resolution without downloading, installing, or writing metadata]' \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':repo_slug -- Repository identifier or direct download URL:_default' \
'::name -- Name to register the application under (falls back to git repository name when omitted):_default' \
&& ret=0
;;
(build)
_arguments "${_arguments_options[@]}" : \
'(--branch)-t+[Release tag to build (defaults to latest matching the channel)]:TAG:_default' \
'(--branch)--tag=[Release tag to build (defaults to latest matching the channel)]:TAG:_default' \
'(-t --tag)--branch=[Branch to build from instead of a release tag]:BRANCH:_default' \
'-p+[Source provider hosting the repository. Defaults to auto-detection]:PROVIDER:_default' \
'--provider=[Source provider hosting the repository. Defaults to auto-detection]:PROVIDER:_default' \
'--base-url=[Custom base URL. Defaults to provider'\''s root]:BASE_URL:_default' \
'-c+[Release channel to track for future builds]:CHANNEL:(stable preview nightly)' \
'--channel=[Release channel to track for future builds]:CHANNEL:(stable preview nightly)' \
'--build-profile=[Build profile used to compile/install from source (auto-detected when omitted)]:BUILD_PROFILE:(rust dotnet go zig cmake)' \
'-d[Create a desktop launcher entry for GUI applications]' \
'--desktop[Create a desktop launcher entry for GUI applications]' \
'--dry-run[Preview build resolution without compiling, installing, or writing metadata]' \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':repo_slug -- Repository identifier or git URL:_default' \
'::name -- Name to register the application under (falls back to git repository name when omitted):_default' \
&& ret=0
;;
(remove)
_arguments "${_arguments_options[@]}" : \
'--purge[Remove package-owned cached data as well as active files]' \
'--force[Remove metadata even when uninstall cleanup fails]' \
'--dry-run[Preview removal actions without deleting files or metadata]' \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'*::names -- Names of packages to remove:_default' \
&& ret=0
;;
(uninstall)
_arguments "${_arguments_options[@]}" : \
'--purge[Remove package-owned cached data as well as active files]' \
'--force[Remove metadata even when uninstall cleanup fails]' \
'--dry-run[Preview removal actions without deleting files or metadata]' \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'*::names -- Names of packages to remove:_default' \
&& ret=0
;;
(rollback)
_arguments "${_arguments_options[@]}" : \
'*--prune=[Delete rollback artifacts for all packages or selected package names]::NAMES:_default' \
'--list[List available rollback artifacts]' \
'--dry-run[Preview rollback restore or prune actions without modifying files or metadata]' \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'*::names -- Package names to restore:_default' \
&& ret=0
;;
(reinstall)
_arguments "${_arguments_options[@]}" : \
'--trust=[Trust verification mode for release-asset reinstalls]:TRUST_MODE:(none best-effort checksum signature all)' \
'--force[Continue reinstalling after uninstall cleanup errors]' \
'--dry-run[Preview reinstall resolution without removing, installing, or writing metadata]' \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'*::names -- Installed package names to reinstall:_default' \
&& ret=0
;;
(upgrade)
_arguments "${_arguments_options[@]}" : \
'--trust=[Trust verification mode for downloaded assets]:TRUST_MODE:(none best-effort checksum signature all)' \
'--force[Reinstall even when the selected version is already installed]' \
'--check[Check for available upgrades without applying them]' \
'--machine-readable[Print one line per available update\: "name oldver newver"]' \
'(--machine-readable)--json[Print check results as JSON]' \
'--dry-run[Preview upgrade resolution without downloading, installing, or writing metadata]' \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'*::names -- Installed package names to upgrade (all packages if omitted):_default' \
&& ret=0
;;
(list)
_arguments "${_arguments_options[@]}" : \
'--json[Print package list as JSON]' \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'::filter -- Package name substring to filter the list:_default' \
&& ret=0
;;
(info)
_arguments "${_arguments_options[@]}" : \
'--json[Print raw package metadata as JSON]' \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':query -- Package name or unique substring for detailed information:_default' \
&& ret=0
;;
(changelog)
_arguments "${_arguments_options[@]}" : \
'(--for)--from=[Starting release tag, or "current"]:FROM_TAG:_default' \
'(--for)--to=[Ending release tag, "current", or "latest"]:TO_TAG:_default' \
'(--from --to)--for=[Show release notes for exactly one release tag]:FOR_TAG:_default' \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':name -- Installed package name:_default' \
&& ret=0
;;
(docs)
_arguments "${_arguments_options[@]}" : \
'*--fetch=[Refresh cached README docs for named packages, or all installed packages when empty]::NAME:_default' \
'(--fetch)--offline[Use only the cached README and skip network fetching]' \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'::name -- Installed package name to search, unless --fetch is refreshing all docs:_default' \
'*::keywords -- Optional search keywords (joined with spaces):_default' \
&& ret=0
;;
(probe)
_arguments "${_arguments_options[@]}" : \
'-p+[Source provider (defaults to GitHub, or scraper for plain URLs)]:PROVIDER:_default' \
'--provider=[Source provider (defaults to GitHub, or scraper for plain URLs)]:PROVIDER:_default' \
'--base-url=[Custom base URL for self-hosted providers]:BASE_URL:_default' \
'-c+[Release channel to display and track]:CHANNEL:(stable preview nightly)' \
'--channel=[Release channel to display and track]:CHANNEL:(stable preview nightly)' \
'--limit=[Number of releases to inspect instead of only one tag/latest release]:LIMIT:_default' \
'--tag=[Release tag to inspect exactly]:TAG:_default' \
'-k+[Asset kind to show and install]:KIND:(app-image mac-app mac-dmg archive compressed binary win-exe checksum auto)' \
'--kind=[Asset kind to show and install]:KIND:(app-image mac-app mac-dmg archive compressed binary win-exe checksum auto)' \
'--trust=[Trust verification mode for downloaded assets]:TRUST_MODE:(none best-effort checksum signature all)' \
'--include-incompatible[Include assets that do not match the current OS/architecture or selected file type]' \
'--json[Print probe results as JSON and exit]' \
'-d[Create a desktop launcher entry for GUI applications]' \
'--desktop[Create a desktop launcher entry for GUI applications]' \
'--dry-run[Run the normal interactive selection and preview flow, then stop before installation]' \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':repo_slug -- Repository identifier or download page URL to inspect:_default' \
'::name -- Name to register the application under (prompts with inferred default when omitted):_default' \
&& ret=0
;;
(search)
_arguments "${_arguments_options[@]}" : \
'-p+[Source provider to search (defaults to GitHub)]:PROVIDER:_default' \
'--provider=[Source provider to search (defaults to GitHub)]:PROVIDER:_default' \
'--base-url=[Custom base URL for self-hosted providers]:BASE_URL:_default' \
'--limit=[Maximum number of results to display]:LIMIT:_default' \
'--language=[Restrict results to repositories with this primary language]:LANGUAGE:_default' \
'--topic=[Restrict results to repositories tagged with this topic]:TOPIC:_default' \
'--min-stars=[Restrict results to repositories with at least this many stars]:N:_default' \
'--max-stars=[Restrict results to repositories with at most this many stars]:N:_default' \
'--pushed-after=[Restrict results to repositories pushed on or after YYYY-MM-DD]:YYYY-MM-DD:_default' \
'--include-forks[Include forked repositories in provider search results]' \
'--include-archived[Include archived repositories in provider search results]' \
'--json[Print repository search results as JSON]' \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'*::query_words -- Optional query words:_default' \
&& ret=0
;;
(find)
_arguments "${_arguments_options[@]}" : \
'-p+[Source provider to search (defaults to GitHub)]:PROVIDER:_default' \
'--provider=[Source provider to search (defaults to GitHub)]:PROVIDER:_default' \
'--base-url=[Custom base URL for self-hosted providers]:BASE_URL:_default' \
'--limit=[Maximum number of results to display]:LIMIT:_default' \
'--language=[Restrict results to repositories with this primary language]:LANGUAGE:_default' \
'--topic=[Restrict results to repositories tagged with this topic]:TOPIC:_default' \
'--min-stars=[Restrict results to repositories with at least this many stars]:N:_default' \
'--max-stars=[Restrict results to repositories with at most this many stars]:N:_default' \
'--pushed-after=[Restrict results to repositories pushed on or after YYYY-MM-DD]:YYYY-MM-DD:_default' \
'--name=[Package name to register without prompting]:NAME:_default' \
'-k+[Asset kind to install]:KIND:(app-image mac-app mac-dmg archive compressed binary win-exe checksum auto)' \
'--kind=[Asset kind to install]:KIND:(app-image mac-app mac-dmg archive compressed binary win-exe checksum auto)' \
'-c+[Release channel to track for upgrades]:CHANNEL:(stable preview nightly)' \
'--channel=[Release channel to track for upgrades]:CHANNEL:(stable preview nightly)' \
'-m+[Match pattern to use as a hint for which asset to prefer]:match:_default' \
'--match-pattern=[Match pattern to use as a hint for which asset to prefer]:match:_default' \
'-e+[Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")]:exclude:_default' \
'--exclude-pattern=[Exclude pattern to filter out unwanted assets (e.g., "rocm", "debug")]:exclude:_default' \
'--trust=[Trust verification mode for downloaded assets]:TRUST_MODE:(none best-effort checksum signature all)' \
'--include-forks[Include forked repositories in provider search results]' \
'--include-archived[Include archived repositories in provider search results]' \
'-d[Create a desktop launcher entry for GUI applications]' \
'--desktop[Create a desktop launcher entry for GUI applications]' \
'--dry-run[Preview install resolution without downloading, installing, or writing metadata]' \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'*::query_words -- Query words:_default' \
&& ret=0
;;
(config)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
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
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'*::keys -- Configuration assignments (format\: key.path=value):_default' \
&& ret=0
;;
(get)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'*::keys -- Configuration keys to retrieve (format\: key.path):_default' \
&& ret=0
;;
(list)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(edit)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(reset)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
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
(auth)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
":: :_upstream__subcmd__auth_commands" \
"*::: :->auth" \
&& ret=0

    case $state in
    (auth)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-auth-command-$line[1]:"
        case $line[1] in
            (set)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'*::keys -- Auth assignments (format\: key.path=value):_default' \
&& ret=0
;;
(get)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'*::keys -- Auth keys to retrieve (format\: key.path):_default' \
&& ret=0
;;
(list)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(edit)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(reset)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_upstream__subcmd__auth__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-auth-help-command-$line[1]:"
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
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
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
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':name -- Name of package to pin:_default' \
&& ret=0
;;
(unpin)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':name -- Name of package to unpin:_default' \
&& ret=0
;;
(rename)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':old_name -- Existing package alias:_default' \
':new_name -- New package alias:_default' \
&& ret=0
;;
(add-entry)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':name -- Installed package name:_default' \
&& ret=0
;;
(rm-entry)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':name -- Installed package name:_default' \
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
(add-entry)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(rm-entry)
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
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
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
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(check)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(clean)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(purge)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
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
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
":: :_upstream__subcmd__import_commands" \
"*::: :->import" \
&& ret=0

    case $state in
    (import)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-import-command-$line[1]:"
        case $line[1] in
            (config)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':path -- Path to an upstream config TOML file:_files' \
&& ret=0
;;
(keys)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':path -- Path to a minisign or cosign public key file:_files' \
&& ret=0
;;
(packages)
_arguments "${_arguments_options[@]}" : \
'--skip-failed[Continue installing remaining packages after a package import fails]' \
'--latest[Ignore exported version tags and install latest releases]' \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':path -- Path to an upstream packages export:_files' \
&& ret=0
;;
(profile)
_arguments "${_arguments_options[@]}" : \
'--skip-failed[Continue installing remaining packages after a package import fails]' \
'--latest[Ignore exported package version tags and install latest releases]' \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':path -- Path to an upstream profile export:_files' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_upstream__subcmd__import__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-import-help-command-$line[1]:"
        case $line[1] in
            (config)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(keys)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(packages)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(profile)
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
(export)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
":: :_upstream__subcmd__export_commands" \
"*::: :->export" \
&& ret=0

    case $state in
    (export)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-export-command-$line[1]:"
        case $line[1] in
            (config)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':path -- Output path for the config export:_files' \
&& ret=0
;;
(keys)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':path -- Output path for the keys export:_files' \
&& ret=0
;;
(packages)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':path -- Output path for the packages export:_files' \
&& ret=0
;;
(profile)
_arguments "${_arguments_options[@]}" : \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
':path -- Output path for the profile export:_files' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_upstream__subcmd__export__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-export-help-command-$line[1]:"
        case $line[1] in
            (config)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(keys)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(packages)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(profile)
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
(history)
_arguments "${_arguments_options[@]}" : \
'--package=[Only show records for this package]:PACKAGE:_default' \
'--action=[Only show records whose command begins with this action]:ACTION:_default' \
'--status=[Only show this status (for example\: ok, warn, fail, success, failed)]:STATUS:_default' \
'--limit=[Maximum number of matching records to show]:LIMIT:_default' \
'--json[Print matching records as JSON]' \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(doctor)
_arguments "${_arguments_options[@]}" : \
'--verbose[Print each check result line in addition to summary output]' \
'--fix[Attempt automatic repairs for detected issues]' \
'--json[Print diagnostic report as JSON]' \
'-y[Accept confirmation prompts automatically]' \
'--yes[Accept confirmation prompts automatically]' \
'--no-pager[Prevent paging long command outputs]' \
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
(info)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(changelog)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(docs)
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
(find)
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
(auth)
_arguments "${_arguments_options[@]}" : \
":: :_upstream__subcmd__help__subcmd__auth_commands" \
"*::: :->auth" \
&& ret=0

    case $state in
    (auth)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-help-auth-command-$line[1]:"
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
(add-entry)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(rm-entry)
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
":: :_upstream__subcmd__help__subcmd__import_commands" \
"*::: :->import" \
&& ret=0

    case $state in
    (import)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-help-import-command-$line[1]:"
        case $line[1] in
            (config)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(keys)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(packages)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(profile)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(export)
_arguments "${_arguments_options[@]}" : \
":: :_upstream__subcmd__help__subcmd__export_commands" \
"*::: :->export" \
&& ret=0

    case $state in
    (export)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:upstream-help-export-command-$line[1]:"
        case $line[1] in
            (config)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(keys)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(packages)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(profile)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(history)
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
'install:Install a release asset or direct download' \
'build:Build and install a package from source' \
'remove:Remove installed package files and metadata' \
'uninstall:Remove installed package files and metadata' \
'rollback:Restore or prune stored rollback artifacts' \
'reinstall:Reinstall packages from their stored source metadata' \
'upgrade:Check for or install package updates' \
'list:List installed packages' \
'info:Show details for one installed package' \
'changelog:Show release notes for an installed package' \
'docs:Search cached or fetched package README docs' \
'probe:Inspect releases, choose an asset, and install it' \
'search:Search provider repositories without installing' \
'find:Search repositories interactively and install one' \
'config:View and edit config.toml' \
'auth:View and edit auth.toml' \
'package:Manage installed package records and launcher entries' \
'hooks:Manage shell PATH hooks and local upstream data' \
'import:Import config, trust keys, packages, or a profile' \
'export:Export config, trust keys, packages, or a profile' \
'history:Show recent command and package audit history' \
'doctor:Run diagnostics to detect installation and integration issues' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'upstream commands' commands "$@"
}
(( $+functions[_upstream__subcmd__auth_commands] )) ||
_upstream__subcmd__auth_commands() {
    local commands; commands=(
'set:Set provider API tokens' \
'get:Get provider API tokens' \
'list:List current provider API tokens' \
'edit:Open auth.toml in your default editor' \
'reset:Reset auth.toml to defaults' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'upstream auth commands' commands "$@"
}
(( $+functions[_upstream__subcmd__auth__subcmd__edit_commands] )) ||
_upstream__subcmd__auth__subcmd__edit_commands() {
    local commands; commands=()
    _describe -t commands 'upstream auth edit commands' commands "$@"
}
(( $+functions[_upstream__subcmd__auth__subcmd__get_commands] )) ||
_upstream__subcmd__auth__subcmd__get_commands() {
    local commands; commands=()
    _describe -t commands 'upstream auth get commands' commands "$@"
}
(( $+functions[_upstream__subcmd__auth__subcmd__help_commands] )) ||
_upstream__subcmd__auth__subcmd__help_commands() {
    local commands; commands=(
'set:Set provider API tokens' \
'get:Get provider API tokens' \
'list:List current provider API tokens' \
'edit:Open auth.toml in your default editor' \
'reset:Reset auth.toml to defaults' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'upstream auth help commands' commands "$@"
}
(( $+functions[_upstream__subcmd__auth__subcmd__help__subcmd__edit_commands] )) ||
_upstream__subcmd__auth__subcmd__help__subcmd__edit_commands() {
    local commands; commands=()
    _describe -t commands 'upstream auth help edit commands' commands "$@"
}
(( $+functions[_upstream__subcmd__auth__subcmd__help__subcmd__get_commands] )) ||
_upstream__subcmd__auth__subcmd__help__subcmd__get_commands() {
    local commands; commands=()
    _describe -t commands 'upstream auth help get commands' commands "$@"
}
(( $+functions[_upstream__subcmd__auth__subcmd__help__subcmd__help_commands] )) ||
_upstream__subcmd__auth__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'upstream auth help help commands' commands "$@"
}
(( $+functions[_upstream__subcmd__auth__subcmd__help__subcmd__list_commands] )) ||
_upstream__subcmd__auth__subcmd__help__subcmd__list_commands() {
    local commands; commands=()
    _describe -t commands 'upstream auth help list commands' commands "$@"
}
(( $+functions[_upstream__subcmd__auth__subcmd__help__subcmd__reset_commands] )) ||
_upstream__subcmd__auth__subcmd__help__subcmd__reset_commands() {
    local commands; commands=()
    _describe -t commands 'upstream auth help reset commands' commands "$@"
}
(( $+functions[_upstream__subcmd__auth__subcmd__help__subcmd__set_commands] )) ||
_upstream__subcmd__auth__subcmd__help__subcmd__set_commands() {
    local commands; commands=()
    _describe -t commands 'upstream auth help set commands' commands "$@"
}
(( $+functions[_upstream__subcmd__auth__subcmd__list_commands] )) ||
_upstream__subcmd__auth__subcmd__list_commands() {
    local commands; commands=()
    _describe -t commands 'upstream auth list commands' commands "$@"
}
(( $+functions[_upstream__subcmd__auth__subcmd__reset_commands] )) ||
_upstream__subcmd__auth__subcmd__reset_commands() {
    local commands; commands=()
    _describe -t commands 'upstream auth reset commands' commands "$@"
}
(( $+functions[_upstream__subcmd__auth__subcmd__set_commands] )) ||
_upstream__subcmd__auth__subcmd__set_commands() {
    local commands; commands=()
    _describe -t commands 'upstream auth set commands' commands "$@"
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
'list:List current configuration values' \
'edit:Open config.toml in your default editor' \
'reset:Reset config.toml to defaults' \
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
'list:List current configuration values' \
'edit:Open config.toml in your default editor' \
'reset:Reset config.toml to defaults' \
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
(( $+functions[_upstream__subcmd__docs_commands] )) ||
_upstream__subcmd__docs_commands() {
    local commands; commands=()
    _describe -t commands 'upstream docs commands' commands "$@"
}
(( $+functions[_upstream__subcmd__doctor_commands] )) ||
_upstream__subcmd__doctor_commands() {
    local commands; commands=()
    _describe -t commands 'upstream doctor commands' commands "$@"
}
(( $+functions[_upstream__subcmd__export_commands] )) ||
_upstream__subcmd__export_commands() {
    local commands; commands=(
'config:Export config.toml' \
'keys:Export trusted minisign and cosign public keys' \
'packages:Export installed package references' \
'profile:Export config, trust keys, and package references' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'upstream export commands' commands "$@"
}
(( $+functions[_upstream__subcmd__export__subcmd__config_commands] )) ||
_upstream__subcmd__export__subcmd__config_commands() {
    local commands; commands=()
    _describe -t commands 'upstream export config commands' commands "$@"
}
(( $+functions[_upstream__subcmd__export__subcmd__help_commands] )) ||
_upstream__subcmd__export__subcmd__help_commands() {
    local commands; commands=(
'config:Export config.toml' \
'keys:Export trusted minisign and cosign public keys' \
'packages:Export installed package references' \
'profile:Export config, trust keys, and package references' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'upstream export help commands' commands "$@"
}
(( $+functions[_upstream__subcmd__export__subcmd__help__subcmd__config_commands] )) ||
_upstream__subcmd__export__subcmd__help__subcmd__config_commands() {
    local commands; commands=()
    _describe -t commands 'upstream export help config commands' commands "$@"
}
(( $+functions[_upstream__subcmd__export__subcmd__help__subcmd__help_commands] )) ||
_upstream__subcmd__export__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'upstream export help help commands' commands "$@"
}
(( $+functions[_upstream__subcmd__export__subcmd__help__subcmd__keys_commands] )) ||
_upstream__subcmd__export__subcmd__help__subcmd__keys_commands() {
    local commands; commands=()
    _describe -t commands 'upstream export help keys commands' commands "$@"
}
(( $+functions[_upstream__subcmd__export__subcmd__help__subcmd__packages_commands] )) ||
_upstream__subcmd__export__subcmd__help__subcmd__packages_commands() {
    local commands; commands=()
    _describe -t commands 'upstream export help packages commands' commands "$@"
}
(( $+functions[_upstream__subcmd__export__subcmd__help__subcmd__profile_commands] )) ||
_upstream__subcmd__export__subcmd__help__subcmd__profile_commands() {
    local commands; commands=()
    _describe -t commands 'upstream export help profile commands' commands "$@"
}
(( $+functions[_upstream__subcmd__export__subcmd__keys_commands] )) ||
_upstream__subcmd__export__subcmd__keys_commands() {
    local commands; commands=()
    _describe -t commands 'upstream export keys commands' commands "$@"
}
(( $+functions[_upstream__subcmd__export__subcmd__packages_commands] )) ||
_upstream__subcmd__export__subcmd__packages_commands() {
    local commands; commands=()
    _describe -t commands 'upstream export packages commands' commands "$@"
}
(( $+functions[_upstream__subcmd__export__subcmd__profile_commands] )) ||
_upstream__subcmd__export__subcmd__profile_commands() {
    local commands; commands=()
    _describe -t commands 'upstream export profile commands' commands "$@"
}
(( $+functions[_upstream__subcmd__find_commands] )) ||
_upstream__subcmd__find_commands() {
    local commands; commands=()
    _describe -t commands 'upstream find commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help_commands] )) ||
_upstream__subcmd__help_commands() {
    local commands; commands=(
'install:Install a release asset or direct download' \
'build:Build and install a package from source' \
'remove:Remove installed package files and metadata' \
'rollback:Restore or prune stored rollback artifacts' \
'reinstall:Reinstall packages from their stored source metadata' \
'upgrade:Check for or install package updates' \
'list:List installed packages' \
'info:Show details for one installed package' \
'changelog:Show release notes for an installed package' \
'docs:Search cached or fetched package README docs' \
'probe:Inspect releases, choose an asset, and install it' \
'search:Search provider repositories without installing' \
'find:Search repositories interactively and install one' \
'config:View and edit config.toml' \
'auth:View and edit auth.toml' \
'package:Manage installed package records and launcher entries' \
'hooks:Manage shell PATH hooks and local upstream data' \
'import:Import config, trust keys, packages, or a profile' \
'export:Export config, trust keys, packages, or a profile' \
'history:Show recent command and package audit history' \
'doctor:Run diagnostics to detect installation and integration issues' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'upstream help commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__auth_commands] )) ||
_upstream__subcmd__help__subcmd__auth_commands() {
    local commands; commands=(
'set:Set provider API tokens' \
'get:Get provider API tokens' \
'list:List current provider API tokens' \
'edit:Open auth.toml in your default editor' \
'reset:Reset auth.toml to defaults' \
    )
    _describe -t commands 'upstream help auth commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__auth__subcmd__edit_commands] )) ||
_upstream__subcmd__help__subcmd__auth__subcmd__edit_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help auth edit commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__auth__subcmd__get_commands] )) ||
_upstream__subcmd__help__subcmd__auth__subcmd__get_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help auth get commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__auth__subcmd__list_commands] )) ||
_upstream__subcmd__help__subcmd__auth__subcmd__list_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help auth list commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__auth__subcmd__reset_commands] )) ||
_upstream__subcmd__help__subcmd__auth__subcmd__reset_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help auth reset commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__auth__subcmd__set_commands] )) ||
_upstream__subcmd__help__subcmd__auth__subcmd__set_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help auth set commands' commands "$@"
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
'list:List current configuration values' \
'edit:Open config.toml in your default editor' \
'reset:Reset config.toml to defaults' \
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
(( $+functions[_upstream__subcmd__help__subcmd__docs_commands] )) ||
_upstream__subcmd__help__subcmd__docs_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help docs commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__doctor_commands] )) ||
_upstream__subcmd__help__subcmd__doctor_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help doctor commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__export_commands] )) ||
_upstream__subcmd__help__subcmd__export_commands() {
    local commands; commands=(
'config:Export config.toml' \
'keys:Export trusted minisign and cosign public keys' \
'packages:Export installed package references' \
'profile:Export config, trust keys, and package references' \
    )
    _describe -t commands 'upstream help export commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__export__subcmd__config_commands] )) ||
_upstream__subcmd__help__subcmd__export__subcmd__config_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help export config commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__export__subcmd__keys_commands] )) ||
_upstream__subcmd__help__subcmd__export__subcmd__keys_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help export keys commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__export__subcmd__packages_commands] )) ||
_upstream__subcmd__help__subcmd__export__subcmd__packages_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help export packages commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__export__subcmd__profile_commands] )) ||
_upstream__subcmd__help__subcmd__export__subcmd__profile_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help export profile commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__find_commands] )) ||
_upstream__subcmd__help__subcmd__find_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help find commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__help_commands] )) ||
_upstream__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help help commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__history_commands] )) ||
_upstream__subcmd__help__subcmd__history_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help history commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__hooks_commands] )) ||
_upstream__subcmd__help__subcmd__hooks_commands() {
    local commands; commands=(
'init:Install shell PATH hooks' \
'check:Check shell PATH hooks' \
'clean:Remove shell PATH hooks' \
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
    local commands; commands=(
'config:Replace config.toml from an export' \
'keys:Import trusted minisign or cosign public keys' \
'packages:Install packages from an exported package list' \
'profile:Import config, keys, and packages from a profile' \
    )
    _describe -t commands 'upstream help import commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__import__subcmd__config_commands] )) ||
_upstream__subcmd__help__subcmd__import__subcmd__config_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help import config commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__import__subcmd__keys_commands] )) ||
_upstream__subcmd__help__subcmd__import__subcmd__keys_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help import keys commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__import__subcmd__packages_commands] )) ||
_upstream__subcmd__help__subcmd__import__subcmd__packages_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help import packages commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__import__subcmd__profile_commands] )) ||
_upstream__subcmd__help__subcmd__import__subcmd__profile_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help import profile commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__info_commands] )) ||
_upstream__subcmd__help__subcmd__info_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help info commands' commands "$@"
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
'pin:Mark an installed package as pinned' \
'unpin:Clear the pinned flag on an installed package' \
'rename:Rename an installed package record and aliases' \
'add-entry:Add a desktop launcher entry for an installed package' \
'rm-entry:Remove an upstream-managed desktop launcher entry' \
    )
    _describe -t commands 'upstream help package commands' commands "$@"
}
(( $+functions[_upstream__subcmd__help__subcmd__package__subcmd__add-entry_commands] )) ||
_upstream__subcmd__help__subcmd__package__subcmd__add-entry_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help package add-entry commands' commands "$@"
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
(( $+functions[_upstream__subcmd__help__subcmd__package__subcmd__rm-entry_commands] )) ||
_upstream__subcmd__help__subcmd__package__subcmd__rm-entry_commands() {
    local commands; commands=()
    _describe -t commands 'upstream help package rm-entry commands' commands "$@"
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
(( $+functions[_upstream__subcmd__history_commands] )) ||
_upstream__subcmd__history_commands() {
    local commands; commands=()
    _describe -t commands 'upstream history commands' commands "$@"
}
(( $+functions[_upstream__subcmd__hooks_commands] )) ||
_upstream__subcmd__hooks_commands() {
    local commands; commands=(
'init:Install shell PATH hooks' \
'check:Check shell PATH hooks' \
'clean:Remove shell PATH hooks' \
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
'init:Install shell PATH hooks' \
'check:Check shell PATH hooks' \
'clean:Remove shell PATH hooks' \
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
    local commands; commands=(
'config:Replace config.toml from an export' \
'keys:Import trusted minisign or cosign public keys' \
'packages:Install packages from an exported package list' \
'profile:Import config, keys, and packages from a profile' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'upstream import commands' commands "$@"
}
(( $+functions[_upstream__subcmd__import__subcmd__config_commands] )) ||
_upstream__subcmd__import__subcmd__config_commands() {
    local commands; commands=()
    _describe -t commands 'upstream import config commands' commands "$@"
}
(( $+functions[_upstream__subcmd__import__subcmd__help_commands] )) ||
_upstream__subcmd__import__subcmd__help_commands() {
    local commands; commands=(
'config:Replace config.toml from an export' \
'keys:Import trusted minisign or cosign public keys' \
'packages:Install packages from an exported package list' \
'profile:Import config, keys, and packages from a profile' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'upstream import help commands' commands "$@"
}
(( $+functions[_upstream__subcmd__import__subcmd__help__subcmd__config_commands] )) ||
_upstream__subcmd__import__subcmd__help__subcmd__config_commands() {
    local commands; commands=()
    _describe -t commands 'upstream import help config commands' commands "$@"
}
(( $+functions[_upstream__subcmd__import__subcmd__help__subcmd__help_commands] )) ||
_upstream__subcmd__import__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'upstream import help help commands' commands "$@"
}
(( $+functions[_upstream__subcmd__import__subcmd__help__subcmd__keys_commands] )) ||
_upstream__subcmd__import__subcmd__help__subcmd__keys_commands() {
    local commands; commands=()
    _describe -t commands 'upstream import help keys commands' commands "$@"
}
(( $+functions[_upstream__subcmd__import__subcmd__help__subcmd__packages_commands] )) ||
_upstream__subcmd__import__subcmd__help__subcmd__packages_commands() {
    local commands; commands=()
    _describe -t commands 'upstream import help packages commands' commands "$@"
}
(( $+functions[_upstream__subcmd__import__subcmd__help__subcmd__profile_commands] )) ||
_upstream__subcmd__import__subcmd__help__subcmd__profile_commands() {
    local commands; commands=()
    _describe -t commands 'upstream import help profile commands' commands "$@"
}
(( $+functions[_upstream__subcmd__import__subcmd__keys_commands] )) ||
_upstream__subcmd__import__subcmd__keys_commands() {
    local commands; commands=()
    _describe -t commands 'upstream import keys commands' commands "$@"
}
(( $+functions[_upstream__subcmd__import__subcmd__packages_commands] )) ||
_upstream__subcmd__import__subcmd__packages_commands() {
    local commands; commands=()
    _describe -t commands 'upstream import packages commands' commands "$@"
}
(( $+functions[_upstream__subcmd__import__subcmd__profile_commands] )) ||
_upstream__subcmd__import__subcmd__profile_commands() {
    local commands; commands=()
    _describe -t commands 'upstream import profile commands' commands "$@"
}
(( $+functions[_upstream__subcmd__info_commands] )) ||
_upstream__subcmd__info_commands() {
    local commands; commands=()
    _describe -t commands 'upstream info commands' commands "$@"
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
'pin:Mark an installed package as pinned' \
'unpin:Clear the pinned flag on an installed package' \
'rename:Rename an installed package record and aliases' \
'add-entry:Add a desktop launcher entry for an installed package' \
'rm-entry:Remove an upstream-managed desktop launcher entry' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'upstream package commands' commands "$@"
}
(( $+functions[_upstream__subcmd__package__subcmd__add-entry_commands] )) ||
_upstream__subcmd__package__subcmd__add-entry_commands() {
    local commands; commands=()
    _describe -t commands 'upstream package add-entry commands' commands "$@"
}
(( $+functions[_upstream__subcmd__package__subcmd__help_commands] )) ||
_upstream__subcmd__package__subcmd__help_commands() {
    local commands; commands=(
'pin:Mark an installed package as pinned' \
'unpin:Clear the pinned flag on an installed package' \
'rename:Rename an installed package record and aliases' \
'add-entry:Add a desktop launcher entry for an installed package' \
'rm-entry:Remove an upstream-managed desktop launcher entry' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'upstream package help commands' commands "$@"
}
(( $+functions[_upstream__subcmd__package__subcmd__help__subcmd__add-entry_commands] )) ||
_upstream__subcmd__package__subcmd__help__subcmd__add-entry_commands() {
    local commands; commands=()
    _describe -t commands 'upstream package help add-entry commands' commands "$@"
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
(( $+functions[_upstream__subcmd__package__subcmd__help__subcmd__rm-entry_commands] )) ||
_upstream__subcmd__package__subcmd__help__subcmd__rm-entry_commands() {
    local commands; commands=()
    _describe -t commands 'upstream package help rm-entry commands' commands "$@"
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
(( $+functions[_upstream__subcmd__package__subcmd__rm-entry_commands] )) ||
_upstream__subcmd__package__subcmd__rm-entry_commands() {
    local commands; commands=()
    _describe -t commands 'upstream package rm-entry commands' commands "$@"
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
