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

# Zsh completion
_scribe() {
    local -a commands
    commands=(
        'create:Create a new worktree'
        'list:List worktrees'
        'open:Open/cd into a worktree'
        'remove:Remove worktree(s)'
        'exit:Exit current worktree'
        'config:Manage configuration'
        'spawn:Create worktree and launch Claude in tmux'
        'ps:Show status of spawned sessions'
        'attach:Attach to tmux session'
        'review:Show diff for parent review'
        'merge:Merge reviewed worktree into current branch'
        'kill:Kill a running tmux window'
        'init:Initialize repository for scribe'
        'update:Update scribe to latest version'
        'version:Show version information'
        'which:Show path to scribe executable'
        'health:Show terminal and dependency status'
    )

    _arguments -C \
        '-o[Open/cd into worktree after creating]' \
        '--no-hooks[Skip on-create hook execution]' \
        '1: :->command' \
        '*: :->args'

    case $state in
        command)
            _describe 'command' commands
            ;;
        args)
            case $words[2] in
                open|remove|review|merge|kill|attach)
                    local worktrees
                    worktrees=($(_scribe_get_worktrees))
                    if [[ $words[2] == "open" ]]; then
                        _describe 'worktree' worktrees
                        _arguments '--all[Open all worktrees in tabs]'
                    else
                        _describe 'worktree' worktrees
                    fi
                    ;;
                list)
                    _arguments '--all[Show all git worktrees]'
                    ;;
                init|update)
                    _arguments '--force[Force overwrite]'
                    ;;
                exit)
                    _arguments '--force[Force removal]'
                    ;;
                config)
                    local -a config_commands
                    config_commands=(
                        'base:Get or set base branch'
                        'on-create:Get or set on-create hook'
                        'show:Show current configuration'
                    )
                    _describe 'config command' config_commands
                    _arguments '--list[List all configuration]'
                    ;;
                spawn)
                    _arguments \
                        '--context[Task context/description]:context:' \
                        '--auto[Auto-start Claude with full prompt]'
                    ;;
            esac
            ;;
    esac
}

compdef _scribe scribe
compdef _scribe sc
