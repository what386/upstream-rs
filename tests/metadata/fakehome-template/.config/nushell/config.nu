const upstream_paths_nu = if ("~/.upstream/generated/paths.nu" | path expand | path exists) { ("~/.upstream/generated/paths.nu" | path expand) } else { null }; source-env $upstream_paths_nu
