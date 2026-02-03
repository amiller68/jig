//! Shell initialization command - prints shell integration code

use anyhow::Result;

const BASH_INIT: &str = r#"
# jig shell integration for bash
jig() {
    local output
    output=$(command jig "$@")
    local exit_code=$?

    # Check if output starts with 'cd ' - if so, eval it
    if [[ "$output" == cd\ * ]]; then
        eval "$output"
    elif [[ -n "$output" ]]; then
        echo "$output"
    fi

    return $exit_code
}
"#;

const ZSH_INIT: &str = r#"
# jig shell integration for zsh
jig() {
    local output
    output=$(command jig "$@")
    local exit_code=$?

    # Check if output starts with 'cd ' - if so, eval it
    if [[ "$output" == cd\ * ]]; then
        eval "$output"
    elif [[ -n "$output" ]]; then
        echo "$output"
    fi

    return $exit_code
}
"#;

const FISH_INIT: &str = r#"
# jig shell integration for fish
function jig
    set -l output (command jig $argv)
    set -l exit_code $status

    # Check if output starts with 'cd ' - if so, eval it
    if string match -q 'cd *' "$output"
        eval $output
    else if test -n "$output"
        echo $output
    end

    return $exit_code
end
"#;

const BASH_COMPLETIONS: &str = r#"
# jig bash completions
_jig_completions() {
    local cur words cword
    if type _init_completion &>/dev/null; then
        _init_completion || return
    else
        cur="${COMP_WORDS[COMP_CWORD]}"
        words=("${COMP_WORDS[@]}")
        cword=$COMP_CWORD
    fi

    # Find the subcommand position, skipping global flags (-o, --no-hooks)
    local subcmd="" subcmd_pos=0
    local i
    for (( i=1; i < cword; i++ )); do
        case "${words[i]}" in
            -o|--no-hooks) continue ;;
            -*) continue ;;
            *)
                subcmd="${words[i]}"
                subcmd_pos=$i
                break
                ;;
        esac
    done

    local subcommands="create c list ls open o remove rm exit config spawn ps attach review merge kill init update version which health tui status shell-init"

    # Helper: get worktree names
    _jig_worktrees() {
        command jig list 2>/dev/null
    }

    # No subcommand yet — complete subcommands or global flags
    if [[ -z "$subcmd" ]]; then
        if [[ "$cur" == -* ]]; then
            COMPREPLY=($(compgen -W "-o --no-hooks" -- "$cur"))
        else
            COMPREPLY=($(compgen -W "$subcommands" -- "$cur"))
        fi
        return
    fi

    # Subcommand-specific completions
    case "$subcmd" in
        create|c)
            # No special completions for create (name is free-form)
            ;;
        list|ls)
            if [[ "$cur" == -* ]]; then
                COMPREPLY=($(compgen -W "--all" -- "$cur"))
            fi
            ;;
        open|o)
            if [[ "$cur" == -* ]]; then
                COMPREPLY=($(compgen -W "--all" -- "$cur"))
            else
                COMPREPLY=($(compgen -W "$(_jig_worktrees)" -- "$cur"))
            fi
            ;;
        remove|rm)
            if [[ "$cur" == -* ]]; then
                COMPREPLY=($(compgen -W "--force -f" -- "$cur"))
            else
                COMPREPLY=($(compgen -W "$(_jig_worktrees)" -- "$cur"))
            fi
            ;;
        exit)
            if [[ "$cur" == -* ]]; then
                COMPREPLY=($(compgen -W "--force -f" -- "$cur"))
            fi
            ;;
        config)
            # Find config subcommand
            local config_subcmd=""
            for (( i=subcmd_pos+1; i < cword; i++ )); do
                case "${words[i]}" in
                    -*) continue ;;
                    *)
                        config_subcmd="${words[i]}"
                        break
                        ;;
                esac
            done
            if [[ -z "$config_subcmd" ]]; then
                if [[ "$cur" == -* ]]; then
                    COMPREPLY=($(compgen -W "--list" -- "$cur"))
                else
                    COMPREPLY=($(compgen -W "base on-create show" -- "$cur"))
                fi
            else
                case "$config_subcmd" in
                    base)
                        if [[ "$cur" == -* ]]; then
                            COMPREPLY=($(compgen -W "--global -g --unset" -- "$cur"))
                        fi
                        ;;
                    on-create)
                        if [[ "$cur" == -* ]]; then
                            COMPREPLY=($(compgen -W "--unset" -- "$cur"))
                        fi
                        ;;
                esac
            fi
            ;;
        spawn)
            if [[ "$cur" == -* ]]; then
                COMPREPLY=($(compgen -W "--context -c --auto" -- "$cur"))
            fi
            ;;
        attach)
            if [[ "$cur" != -* ]]; then
                COMPREPLY=($(compgen -W "$(_jig_worktrees)" -- "$cur"))
            fi
            ;;
        review)
            if [[ "$cur" == -* ]]; then
                COMPREPLY=($(compgen -W "--full" -- "$cur"))
            else
                COMPREPLY=($(compgen -W "$(_jig_worktrees)" -- "$cur"))
            fi
            ;;
        merge)
            if [[ "$cur" != -* ]]; then
                COMPREPLY=($(compgen -W "$(_jig_worktrees)" -- "$cur"))
            fi
            ;;
        kill)
            if [[ "$cur" != -* ]]; then
                COMPREPLY=($(compgen -W "$(_jig_worktrees)" -- "$cur"))
            fi
            ;;
        status)
            if [[ "$cur" != -* ]]; then
                COMPREPLY=($(compgen -W "$(_jig_worktrees)" -- "$cur"))
            fi
            ;;
        init)
            if [[ "$cur" == -* ]]; then
                COMPREPLY=($(compgen -W "--force -f --backup --audit" -- "$cur"))
            fi
            ;;
        update)
            if [[ "$cur" == -* ]]; then
                COMPREPLY=($(compgen -W "--force -f" -- "$cur"))
            fi
            ;;
        shell-init)
            if [[ "$cur" != -* ]]; then
                COMPREPLY=($(compgen -W "bash zsh fish" -- "$cur"))
            fi
            ;;
    esac
}

complete -F _jig_completions jig
"#;

const ZSH_COMPLETIONS: &str = r#"
# jig zsh completions
_jig() {
    local -a subcommands
    subcommands=(
        'create:Create a new worktree'
        'c:Create a new worktree'
        'list:List worktrees'
        'ls:List worktrees'
        'open:Open/cd into a worktree'
        'o:Open/cd into a worktree'
        'remove:Remove worktree(s)'
        'rm:Remove worktree(s)'
        'exit:Exit current worktree and remove it'
        'config:Manage configuration'
        'spawn:Create worktree and launch Claude in tmux'
        'ps:Show status of spawned sessions'
        'attach:Attach to tmux session'
        'review:Show diff for parent review'
        'merge:Merge reviewed worktree into current branch'
        'kill:Kill a running tmux window'
        'init:Initialize repository for jig'
        'update:Update jig to latest version'
        'version:Show version information'
        'which:Show path to jig executable'
        'health:Show terminal and dependency status'
        'tui:Launch the terminal UI'
        'status:Show detailed worker status'
        'shell-init:Print shell integration code'
    )

    _jig_worktrees() {
        local -a worktrees
        worktrees=(${(f)"$(command jig list 2>/dev/null)"})
        _describe 'worktree' worktrees
    }

    # Find the subcommand, skipping global flags
    local subcmd=""
    local i
    for (( i=1; i < CURRENT; i++ )); do
        case "${words[i]}" in
            -o|--no-hooks) continue ;;
            -*) continue ;;
            *)
                subcmd="${words[i]}"
                break
                ;;
        esac
    done

    if [[ -z "$subcmd" ]]; then
        if [[ "$PREFIX" == -* ]]; then
            _values 'global flags' '-o[Open worktree after creating]' '--no-hooks[Skip on-create hooks]'
        else
            _describe 'command' subcommands
        fi
        return
    fi

    case "$subcmd" in
        create|c)
            ;;
        list|ls)
            _arguments '--all[Show all git worktrees]'
            ;;
        open|o)
            _arguments '--all[Open all worktrees in tabs]' '*:worktree:_jig_worktrees'
            ;;
        remove|rm)
            _arguments '(-f --force)'{-f,--force}'[Force removal]' '*:worktree:_jig_worktrees'
            ;;
        exit)
            _arguments '(-f --force)'{-f,--force}'[Force removal]'
            ;;
        config)
            local -a config_subcmds
            config_subcmds=(
                'base:Get or set base branch'
                'on-create:Get or set on-create hook'
                'show:Show current configuration'
            )
            # Find config subcommand
            local config_subcmd=""
            for (( i=i+1; i < CURRENT; i++ )); do
                case "${words[i]}" in
                    -*) continue ;;
                    *)
                        config_subcmd="${words[i]}"
                        break
                        ;;
                esac
            done
            if [[ -z "$config_subcmd" ]]; then
                _arguments '--list[List all configuration]'
                _describe 'config command' config_subcmds
            else
                case "$config_subcmd" in
                    base)
                        _arguments '(-g --global)'{-g,--global}'[Use global default]' '--unset[Remove the setting]'
                        ;;
                    on-create)
                        _arguments '--unset[Remove the hook]'
                        ;;
                esac
            fi
            ;;
        spawn)
            _arguments '(-c --context)'{-c,--context}'[Task context]:context' '--auto[Auto-start Claude]'
            ;;
        attach)
            _jig_worktrees
            ;;
        review)
            _arguments '--full[Show full diff]' '*:worktree:_jig_worktrees'
            ;;
        merge)
            _jig_worktrees
            ;;
        kill)
            _jig_worktrees
            ;;
        status)
            _jig_worktrees
            ;;
        init)
            _arguments '(-f --force)'{-f,--force}'[Reinitialize]' '--backup[Backup existing files]' '--audit[Run Claude audit]'
            ;;
        update)
            _arguments '(-f --force)'{-f,--force}'[Force update]'
            ;;
        shell-init)
            _values 'shell' bash zsh fish
            ;;
    esac
}

compdef _jig jig
"#;

const FISH_COMPLETIONS: &str = r#"
# jig fish completions

# Disable file completions by default
complete -c jig -f

# Helper function for worktree names
function __jig_worktrees
    command jig list 2>/dev/null
end

# Condition: no subcommand yet
function __jig_needs_command
    set -l cmd (commandline -opc)
    for i in (seq 2 (count $cmd))
        switch $cmd[$i]
            case '-*'
                continue
            case '*'
                return 1
        end
    end
    return 0
end

# Condition: current subcommand matches
function __jig_using_command
    set -l cmd (commandline -opc)
    set -l target $argv[1]
    for i in (seq 2 (count $cmd))
        switch $cmd[$i]
            case '-*'
                continue
            case '*'
                if test "$cmd[$i]" = "$target"
                    return 0
                end
                return 1
        end
    end
    return 1
end

# Global flags
complete -c jig -n '__jig_needs_command' -s o -d 'Open worktree after creating'
complete -c jig -n '__jig_needs_command' -l no-hooks -d 'Skip on-create hooks'

# Subcommands
complete -c jig -n '__jig_needs_command' -a create -d 'Create a new worktree'
complete -c jig -n '__jig_needs_command' -a c -d 'Create a new worktree'
complete -c jig -n '__jig_needs_command' -a list -d 'List worktrees'
complete -c jig -n '__jig_needs_command' -a ls -d 'List worktrees'
complete -c jig -n '__jig_needs_command' -a open -d 'Open/cd into a worktree'
complete -c jig -n '__jig_needs_command' -a o -d 'Open/cd into a worktree'
complete -c jig -n '__jig_needs_command' -a remove -d 'Remove worktree(s)'
complete -c jig -n '__jig_needs_command' -a rm -d 'Remove worktree(s)'
complete -c jig -n '__jig_needs_command' -a exit -d 'Exit current worktree and remove it'
complete -c jig -n '__jig_needs_command' -a config -d 'Manage configuration'
complete -c jig -n '__jig_needs_command' -a spawn -d 'Create worktree and launch Claude in tmux'
complete -c jig -n '__jig_needs_command' -a ps -d 'Show status of spawned sessions'
complete -c jig -n '__jig_needs_command' -a attach -d 'Attach to tmux session'
complete -c jig -n '__jig_needs_command' -a review -d 'Show diff for parent review'
complete -c jig -n '__jig_needs_command' -a merge -d 'Merge reviewed worktree into current branch'
complete -c jig -n '__jig_needs_command' -a kill -d 'Kill a running tmux window'
complete -c jig -n '__jig_needs_command' -a init -d 'Initialize repository for jig'
complete -c jig -n '__jig_needs_command' -a update -d 'Update jig to latest version'
complete -c jig -n '__jig_needs_command' -a version -d 'Show version information'
complete -c jig -n '__jig_needs_command' -a which -d 'Show path to jig executable'
complete -c jig -n '__jig_needs_command' -a health -d 'Show terminal and dependency status'
complete -c jig -n '__jig_needs_command' -a tui -d 'Launch the terminal UI'
complete -c jig -n '__jig_needs_command' -a status -d 'Show detailed worker status'
complete -c jig -n '__jig_needs_command' -a shell-init -d 'Print shell integration code'

# list / ls
complete -c jig -n '__jig_using_command list; or __jig_using_command ls' -l all -d 'Show all git worktrees'

# open / o — worktree names + --all
complete -c jig -n '__jig_using_command open; or __jig_using_command o' -l all -d 'Open all worktrees in tabs'
complete -c jig -n '__jig_using_command open; or __jig_using_command o' -a '(__jig_worktrees)' -d 'Worktree'

# remove / rm — worktree names + --force
complete -c jig -n '__jig_using_command remove; or __jig_using_command rm' -l force -s f -d 'Force removal'
complete -c jig -n '__jig_using_command remove; or __jig_using_command rm' -a '(__jig_worktrees)' -d 'Worktree'

# exit
complete -c jig -n '__jig_using_command exit' -l force -s f -d 'Force removal'

# config subcommands
complete -c jig -n '__jig_using_command config' -a base -d 'Get or set base branch'
complete -c jig -n '__jig_using_command config' -a on-create -d 'Get or set on-create hook'
complete -c jig -n '__jig_using_command config' -a show -d 'Show current configuration'
complete -c jig -n '__jig_using_command config' -l list -d 'List all configuration'

# spawn
complete -c jig -n '__jig_using_command spawn' -l context -s c -d 'Task context' -r
complete -c jig -n '__jig_using_command spawn' -l auto -d 'Auto-start Claude'

# attach — worktree names
complete -c jig -n '__jig_using_command attach' -a '(__jig_worktrees)' -d 'Worktree'

# review — worktree names + --full
complete -c jig -n '__jig_using_command review' -l full -d 'Show full diff'
complete -c jig -n '__jig_using_command review' -a '(__jig_worktrees)' -d 'Worktree'

# merge — worktree names
complete -c jig -n '__jig_using_command merge' -a '(__jig_worktrees)' -d 'Worktree'

# kill — worktree names
complete -c jig -n '__jig_using_command kill' -a '(__jig_worktrees)' -d 'Worktree'

# status — worktree names
complete -c jig -n '__jig_using_command status' -a '(__jig_worktrees)' -d 'Worktree'

# init
complete -c jig -n '__jig_using_command init' -l force -s f -d 'Reinitialize'
complete -c jig -n '__jig_using_command init' -l backup -d 'Backup existing files'
complete -c jig -n '__jig_using_command init' -l audit -d 'Run Claude audit'

# update
complete -c jig -n '__jig_using_command update' -l force -s f -d 'Force update'

# shell-init
complete -c jig -n '__jig_using_command shell-init' -a 'bash zsh fish' -d 'Shell type'
"#;

pub fn run(shell: &str) -> Result<()> {
    let (init_code, completions) = match shell.to_lowercase().as_str() {
        "bash" => (BASH_INIT, BASH_COMPLETIONS),
        "zsh" => (ZSH_INIT, ZSH_COMPLETIONS),
        "fish" => (FISH_INIT, FISH_COMPLETIONS),
        _ => {
            eprintln!("Unsupported shell: {}", shell);
            eprintln!("Supported shells: bash, zsh, fish");
            std::process::exit(1);
        }
    };

    println!("{}", init_code.trim());
    println!();
    println!("{}", completions.trim());
    Ok(())
}
