# fish completion for ak980
set -l cmds gui status list time agent daemon lcd effects modes light fx side custom options keys remap macro launch profile auto service setup apply link reconnect
complete -c ak980 -f
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a gui -d 'Open the app window'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a status -d 'Connection, battery, profile, agent'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a time -d 'Sync the screen clock'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a agent -d 'Background agent'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a lcd -d 'Upload an animation to the screen'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a effects -d 'List lighting effects'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a light -d 'Keyboard lighting effect'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a fx -d 'PC lighting effect'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a custom -d 'Per-key colours'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a options -d 'Keyboard options'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a keys -d 'Key names'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a remap -d 'Remap a key'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a macro -d 'Macros'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a launch -d 'Launcher keys F13-F24'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a profile -d 'Profiles'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a auto -d 'Switch profiles for programs'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a service -d 'Start the agent automatically'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a setup -d 'Allow access to the keyboard'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a apply -d 'Send the active profile'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a link -d 'The 2.4 GHz link record'
complete -c ak980 -n "not __fish_seen_subcommand_from $cmds" -a reconnect -d 'Reconnect the 2.4 GHz receiver'
complete -c ak980 -n "__fish_seen_subcommand_from lcd" -F
complete -c ak980 -n "__fish_seen_subcommand_from lcd" -l fit -xa "fit fill stretch"
complete -c ak980 -n "__fish_seen_subcommand_from fx" -xa "wave drift breath starlight reactive fire spectrum pulse"
complete -c ak980 -n "__fish_seen_subcommand_from macro" -xa "list add rm clear upload"
complete -c ak980 -n "__fish_seen_subcommand_from profile" -xa "list use apply new rm rename export import"
complete -c ak980 -n "__fish_seen_subcommand_from auto" -xa "on off list add rm default"
complete -c ak980 -n "__fish_seen_subcommand_from service" -xa "list enable disable session systemd-user dinit-user runit-user system-systemd system-openrc system-dinit system-runit system-s6"
complete -c ak980 -n "__fish_seen_subcommand_from remap custom" -xa "(ak980 keys 2>/dev/null | awk '{print \$2}')"
complete -c ak980 -l lang -xa "auto en uk" -d 'Interface language'
