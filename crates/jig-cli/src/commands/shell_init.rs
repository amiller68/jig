//! Shell initialization command - prints shell integration code

use anyhow::Result;

const BASH_INIT: &str = r#"
# jig shell integration for bash
jig() {
    local output
    output=$(command jig "$@")
    local exit_code=$?
    if [[ "$output" == cd\ * ]]; then
        eval "$output"
    elif [[ -n "$output" ]]; then
        echo "$output"
    fi
    return $exit_code
}

_jig() {
    local cur prev words cword
    _init_completion 2>/dev/null || {
        cur="${COMP_WORDS[COMP_CWORD]}"
        prev="${COMP_WORDS[COMP_CWORD-1]}"
    }

    local commands="create list open remove exit config spawn ps attach review merge kill init update version which health tui status shell-init shell-setup"

    # Get worktrees for completion
    _jig_worktrees() {
        command jig list 2>/dev/null
    }

    case "$prev" in
        jig)
            COMPREPLY=($(compgen -W "$commands" -- "$cur"))
            return
            ;;
        open|o|attach|review|merge|kill|status|remove|rm)
            COMPREPLY=($(compgen -W "$(_jig_worktrees)" -- "$cur"))
            return
            ;;
        shell-init)
            COMPREPLY=($(compgen -W "bash zsh fish" -- "$cur"))
            return
            ;;
        config)
            COMPREPLY=($(compgen -W "base on-create show" -- "$cur"))
            return
            ;;
    esac

    if [[ "$cur" == -* ]]; then
        case "${COMP_WORDS[1]}" in
            create|c) COMPREPLY=($(compgen -W "-o --no-hooks" -- "$cur")) ;;
            list|ls) COMPREPLY=($(compgen -W "--all" -- "$cur")) ;;
            open|o) COMPREPLY=($(compgen -W "--all" -- "$cur")) ;;
            remove|rm) COMPREPLY=($(compgen -W "-f --force" -- "$cur")) ;;
            exit) COMPREPLY=($(compgen -W "-f --force" -- "$cur")) ;;
            init) COMPREPLY=($(compgen -W "-f --force --backup --audit" -- "$cur")) ;;
            spawn) COMPREPLY=($(compgen -W "-c --context --auto" -- "$cur")) ;;
            review) COMPREPLY=($(compgen -W "--full" -- "$cur")) ;;
            shell-setup) COMPREPLY=($(compgen -W "--dry-run" -- "$cur")) ;;
            *) COMPREPLY=($(compgen -W "-o --no-hooks -h --help" -- "$cur")) ;;
        esac
    fi
}

complete -F _jig jig
"#;

const ZSH_INIT: &str = r##"
# jig shell integration for zsh
jig() {
    local output
    output=$(command jig "$@")
    local exit_code=$?
    if [[ "$output" == cd\ * ]]; then
        eval "$output"
    elif [[ -n "$output" ]]; then
        echo "$output"
    fi
    return $exit_code
}

#compdef jig
_jig() {
    local -a commands
    commands=(
        'create:Create a new worktree'
        'list:List worktrees'
        'open:Open/cd into a worktree'
        'remove:Remove worktree(s)'
        'exit:Exit current worktree'
        'config:Manage configuration'
        'spawn:Launch Claude in tmux'
        'ps:Show spawned sessions'
        'attach:Attach to tmux session'
        'review:Show diff for review'
        'merge:Merge worktree'
        'kill:Kill tmux window'
        'init:Initialize repo'
        'update:Update jig'
        'version:Show version'
        'which:Show jig path'
        'health:Show status'
        'tui:Launch TUI'
        'status:Worker status'
        'shell-init:Print shell init'
        'shell-setup:Configure shell'
    )

    _jig_worktrees() {
        local -a wts
        wts=(${(f)"$(command jig list 2>/dev/null)"})
        _describe 'worktree' wts
    }

    _arguments -C \
        '-o[Open after creating]' \
        '--no-hooks[Skip hooks]' \
        '-h[Help]' \
        '--help[Help]' \
        '1: :->cmd' \
        '*:: :->args'

    case $state in
        cmd)
            _describe 'command' commands
            ;;
        args)
            case $words[1] in
                open|o|attach|review|merge|kill|status)
                    _jig_worktrees
                    ;;
                remove|rm)
                    _arguments \
                        '-f[Force]' \
                        '--force[Force]' \
                        '*:worktree:_jig_worktrees'
                    ;;
                create|c)
                    _arguments \
                        '1:name:' \
                        '2:branch:'
                    ;;
                list|ls)
                    _arguments '--all[Show all]'
                    ;;
                config)
                    local -a config_cmds
                    config_cmds=('base:Set base branch' 'on-create:Set hook' 'show:Show config')
                    _describe 'config command' config_cmds
                    ;;
                spawn)
                    _arguments \
                        '-c[Context]:context:' \
                        '--context=[Context]:context:' \
                        '--auto[Auto-start]' \
                        '1:name:'
                    ;;
                init)
                    _arguments \
                        '-f[Force]' \
                        '--force[Force]' \
                        '--backup[Backup]' \
                        '--audit[Audit]'
                    ;;
                shell-init)
                    _values 'shell' bash zsh fish
                    ;;
                shell-setup)
                    _arguments '--dry-run[Dry run]'
                    ;;
            esac
            ;;
    esac
}

compdef _jig jig
"##;

const FISH_INIT: &str = r#"
# jig shell integration for fish
function jig
    set -l output (command jig $argv)
    set -l exit_code $status
    if string match -q 'cd *' "$output"
        eval $output
    else if test -n "$output"
        echo $output
    end
    return $exit_code
end

# Completions
function __jig_worktrees
    command jig list 2>/dev/null
end

function __jig_needs_command
    set -l cmd (commandline -opc)
    test (count $cmd) -eq 1
end

function __jig_using_command
    set -l cmd (commandline -opc)
    test (count $cmd) -gt 1 -a "$cmd[2]" = "$argv[1]"
end

# Commands
complete -c jig -f
complete -c jig -n '__jig_needs_command' -a create -d 'Create worktree'
complete -c jig -n '__jig_needs_command' -a list -d 'List worktrees'
complete -c jig -n '__jig_needs_command' -a open -d 'Open worktree'
complete -c jig -n '__jig_needs_command' -a remove -d 'Remove worktree'
complete -c jig -n '__jig_needs_command' -a exit -d 'Exit worktree'
complete -c jig -n '__jig_needs_command' -a config -d 'Configuration'
complete -c jig -n '__jig_needs_command' -a spawn -d 'Spawn Claude'
complete -c jig -n '__jig_needs_command' -a ps -d 'Show sessions'
complete -c jig -n '__jig_needs_command' -a attach -d 'Attach session'
complete -c jig -n '__jig_needs_command' -a review -d 'Review diff'
complete -c jig -n '__jig_needs_command' -a merge -d 'Merge worktree'
complete -c jig -n '__jig_needs_command' -a kill -d 'Kill session'
complete -c jig -n '__jig_needs_command' -a init -d 'Initialize'
complete -c jig -n '__jig_needs_command' -a update -d 'Update jig'
complete -c jig -n '__jig_needs_command' -a version -d 'Version'
complete -c jig -n '__jig_needs_command' -a which -d 'Show path'
complete -c jig -n '__jig_needs_command' -a health -d 'Health check'
complete -c jig -n '__jig_needs_command' -a tui -d 'Launch TUI'
complete -c jig -n '__jig_needs_command' -a status -d 'Worker status'
complete -c jig -n '__jig_needs_command' -a shell-init -d 'Shell init'
complete -c jig -n '__jig_needs_command' -a shell-setup -d 'Shell setup'

# Worktree completions
complete -c jig -n '__jig_using_command open' -a '(__jig_worktrees)' -d 'Worktree'
complete -c jig -n '__jig_using_command attach' -a '(__jig_worktrees)' -d 'Worktree'
complete -c jig -n '__jig_using_command review' -a '(__jig_worktrees)' -d 'Worktree'
complete -c jig -n '__jig_using_command merge' -a '(__jig_worktrees)' -d 'Worktree'
complete -c jig -n '__jig_using_command kill' -a '(__jig_worktrees)' -d 'Worktree'
complete -c jig -n '__jig_using_command status' -a '(__jig_worktrees)' -d 'Worktree'
complete -c jig -n '__jig_using_command remove' -a '(__jig_worktrees)' -d 'Worktree'

# Flags
complete -c jig -n '__jig_using_command remove' -l force -s f -d 'Force'
complete -c jig -n '__jig_using_command init' -l force -s f -d 'Force'
complete -c jig -n '__jig_using_command init' -l backup -d 'Backup'
complete -c jig -n '__jig_using_command spawn' -l context -s c -d 'Context'
complete -c jig -n '__jig_using_command spawn' -l auto -d 'Auto-start'
complete -c jig -n '__jig_using_command shell-init' -a 'bash zsh fish' -d 'Shell'
complete -c jig -n '__jig_using_command shell-setup' -l dry-run -d 'Dry run'
complete -c jig -n '__jig_using_command config' -a 'base on-create show' -d 'Config cmd'
"#;

pub fn run(shell: &str) -> Result<()> {
    let init_code = match shell.to_lowercase().as_str() {
        "bash" => BASH_INIT,
        "zsh" => ZSH_INIT,
        "fish" => FISH_INIT,
        _ => {
            eprintln!("Unsupported shell: {}", shell);
            eprintln!("Supported shells: bash, zsh, fish");
            std::process::exit(1);
        }
    };

    println!("{}", init_code.trim());
    Ok(())
}
