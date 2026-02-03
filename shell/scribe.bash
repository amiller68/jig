# scribe - git worktree manager
# https://github.com/amiller68/scribe-rs

# Wrapper function to handle cd operations
# The Rust binary outputs "cd 'path'" to stdout when appropriate
scribe() {
    local scribe_bin="${SCRIBE_BIN:-scribe}"

    # Eval output if command might cd (open, exit, or create with -o flag)
    # Don't eval when using --all flag (tabs are opened directly)
    if [[ "$1" == "open" && "$2" == "--all" ]]; then
        command "$scribe_bin" "$@"
    elif [[ "$1" == "open" || "$1" == "exit" || "$1" == "-o" || "$2" == "-o" || "$*" == *"-o"* ]]; then
        eval "$(command "$scribe_bin" "$@")"
    else
        command "$scribe_bin" "$@"
    fi
}

# Short alias
sc() {
    scribe "$@"
}

# Get worktree names for completion (handles nested paths like feature/auth/login)
_scribe_get_worktrees() {
    local repo=$(git rev-parse --show-toplevel 2>/dev/null)
    local wt_dir="$repo/.worktrees"
    [[ -d "$wt_dir" ]] || return

    # Recursively find worktrees (dirs with .git file) without entering their content
    _find_wt() {
        local dir="$1" prefix="$2"
        for entry in "$dir"/*/; do
            [[ -d "$entry" ]] || continue
            local name=$(basename "$entry")
            if [[ -f "$entry/.git" ]]; then
                echo "${prefix}${name}"
            else
                _find_wt "$entry" "${prefix}${name}/"
            fi
        done
    }
    _find_wt "$wt_dir" ""
}

# Completion
_scribe_complete() {
    local cur=${COMP_WORDS[COMP_CWORD]}
    local prev=${COMP_WORDS[COMP_CWORD-1]}

    if [[ $COMP_CWORD -eq 1 ]]; then
        COMPREPLY=($(compgen -W "create list remove open exit health config spawn ps attach review merge kill init update version which -o --no-hooks" -- "$cur"))
    elif [[ $COMP_CWORD -eq 2 ]]; then
        case $prev in
            open)
                local worktrees=$(_scribe_get_worktrees)
                [[ -n "$worktrees" ]] && COMPREPLY=($(compgen -W "--all $worktrees" -- "$cur"))
                [[ -z "$worktrees" ]] && COMPREPLY=($(compgen -W "--all" -- "$cur"))
                ;;
            remove|review|merge|kill|attach)
                local worktrees=$(_scribe_get_worktrees)
                [[ -n "$worktrees" ]] && COMPREPLY=($(compgen -W "$worktrees" -- "$cur"))
                ;;
            list)
                COMPREPLY=($(compgen -W "--all" -- "$cur"))
                ;;
            update|init)
                COMPREPLY=($(compgen -W "--force" -- "$cur"))
                ;;
            exit)
                COMPREPLY=($(compgen -W "--force" -- "$cur"))
                ;;
            config)
                COMPREPLY=($(compgen -W "base on-create show --list" -- "$cur"))
                ;;
            spawn)
                COMPREPLY=($(compgen -W "--context --auto" -- "$cur"))
                ;;
            -o|--no-hooks)
                COMPREPLY=($(compgen -W "create" -- "$cur"))
                ;;
        esac
    elif [[ $COMP_CWORD -eq 3 ]]; then
        case ${COMP_WORDS[2]} in
            base)
                COMPREPLY=($(compgen -W "--global --unset" -- "$cur"))
                ;;
            on-create)
                COMPREPLY=($(compgen -W "--unset" -- "$cur"))
                ;;
        esac
    fi
}
complete -F _scribe_complete scribe
complete -F _scribe_complete sc
