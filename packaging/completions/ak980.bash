# bash completion for ak980                                  -*- shell-script -*-

_ak980() {
    local cur prev words cword
    _init_completion || return

    local commands="gui status battery list time agent daemon lcd effects modes light fx side custom options keys remap
                    macro launch profile auto service setup apply link reconnect"
    if [[ $cword -eq 1 ]]; then
        COMPREPLY=($(compgen -W "$commands --help --version --lang" -- "$cur"))
        return
    fi

    case ${words[1]} in
        lcd)
            if [[ $prev == --fit ]]; then
                COMPREPLY=($(compgen -W "fit fill stretch" -- "$cur"))
            elif [[ $prev == --text-pos ]]; then
                COMPREPLY=($(compgen -W "top-left top top-right center bottom-left bottom bottom-right" -- "$cur"))
            elif [[ $cur == -* ]]; then
                COMPREPLY=($(compgen -W "--fit --background --flip-h --flip-v --brightness --contrast --saturation
                    --speed --delay --trim-start --trim-end --step --text --text-color --text-size --text-pos
                    --no-dither --slot --preview --history" -- "$cur"))
            else
                _filedir '@(gif|png|jpg|jpeg|bmp|webp|apng)'
            fi
            ;;
        light)
            COMPREPLY=($(compgen -W "static breath spectrum rolling rotating flowing off --color --rainbow
                --brightness --speed --direction" -- "$cur"))
            ;;
        fx)
            COMPREPLY=($(compgen -W "wave drift breath starlight reactive fire spectrum pulse --list --brightness" -- "$cur"))
            ;;
        remap|custom)
            if [[ $cur == -* ]]; then
                COMPREPLY=($(compgen -W "--fn --reset --list --dry --all --key --wasd --clear --brightness" -- "$cur"))
            else
                COMPREPLY=($(compgen -W "$(ak980 keys 2>/dev/null | awk '{print $2}')
                    default off key: mouse: media: macro: multi:" -- "$cur"))
            fi
            ;;
        macro)
            [[ $cword -eq 2 ]] && COMPREPLY=($(compgen -W "list add rm clear upload" -- "$cur"))
            ;;
        profile)
            if [[ $cword -eq 2 ]]; then
                COMPREPLY=($(compgen -W "list use apply new rm rename export import" -- "$cur"))
            elif [[ $prev == import ]]; then
                _filedir json
            fi
            ;;
        auto)
            [[ $cword -eq 2 ]] && COMPREPLY=($(compgen -W "on off list add rm default" -- "$cur"))
            ;;
        service)
            if [[ $cword -eq 2 ]]; then
                COMPREPLY=($(compgen -W "list enable disable" -- "$cur"))
            else
                COMPREPLY=($(compgen -W "session systemd-user dinit-user runit-user system-systemd system-openrc system-dinit
                    system-runit system-s6" -- "$cur"))
            fi
            ;;
        agent|daemon)
            COMPREPLY=($(compgen -W "--system --once --interval" -- "$cur"))
            ;;
        options)
            COMPREPLY=($(compgen -W "--win-lock --alt-tab-lock --alt-f4-lock --fn-toggle --sleep --response on off" -- "$cur"))
            ;;
        side)
            COMPREPLY=($(compgen -W "flowing red yellow green iceblue blue pink white neon off" -- "$cur"))
            ;;
        launch)
            COMPREPLY=($(compgen -W "F13 F14 F15 F16 F17 F18 F19 F20 F21 F22 F23 F24 --rm --list" -- "$cur"))
            ;;
    esac
} &&
complete -F _ak980 ak980
