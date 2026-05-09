//! Shell detection, init scripts, and config-file integration.

use std::path::{Path, PathBuf};

const MARKER_START: &str = "# >>> jig shell integration >>>";
const MARKER_END: &str = "# <<< jig shell integration <<<";

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

    local commands="create list open remove exit config spawn ps attach kill issues init update version which health shell-init shell-setup"

    # Get worktrees for completion (context-aware)
    _jig_worktrees() {
        local wts
        if git rev-parse --is-inside-work-tree &>/dev/null; then
            wts=$(command jig list --plain 2>/dev/null)
        else
            wts=$(command jig list -gp 2>/dev/null | sed -n 's/^  //p')
        fi
        echo "$wts"
    }

    # Get issue IDs for completion
    _jig_issues() {
        command jig issues --ids 2>/dev/null
    }

    # Get branch names for completion
    __jig_branches() {
        git branch -a --format='%(refname:short)' 2>/dev/null
    }

    case "$prev" in
        --issue|-I)
            COMPREPLY=($(compgen -W "$(_jig_issues)" -- "$cur"))
            return
            ;;
        --base|-b)
            COMPREPLY=($(compgen -W "$(__jig_branches)" -- "$cur"))
            return
            ;;
        jig)
            COMPREPLY=($(compgen -W "$commands" -- "$cur"))
            return
            ;;
        open|o|attach|kill|remove|rm)
            COMPREPLY=($(compgen -W "$(_jig_worktrees)" -- "$cur"))
            return
            ;;
        issues)
            COMPREPLY=($(compgen -W "$(_jig_issues)" -- "$cur"))
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
            create|c) COMPREPLY=($(compgen -W "-o -b --base --no-hooks" -- "$cur")) ;;
            list|ls) COMPREPLY=($(compgen -W "--all -p --plain" -- "$cur")) ;;
            open|o) ;;
            remove|rm) COMPREPLY=($(compgen -W "-f --force" -- "$cur")) ;;
            exit) COMPREPLY=($(compgen -W "-f --force" -- "$cur")) ;;
            init) COMPREPLY=($(compgen -W "-f --force --backup --audit" -- "$cur")) ;;
            spawn) COMPREPLY=($(compgen -W "-c --context -b --base -I --issue --auto" -- "$cur")) ;;
            issues) COMPREPLY=($(compgen -W "-s --status -p --priority -c --category -l --label -i --interactive --ids" -- "$cur")) ;;
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
        'kill:Kill tmux window'
        'nuke:Nuke all workers'
        'issues:Browse issues'
        'init:Initialize repo'
        'update:Update jig'
        'version:Show version'
        'which:Show jig path'
        'health:Show status'
        'shell-init:Print shell init'
        'shell-setup:Configure shell'
    )

    _jig_worktrees() {
        local -a wts
        if git rev-parse --is-inside-work-tree &>/dev/null; then
            wts=(${(f)"$(command jig list --plain 2>/dev/null)"})
        else
            wts=(${(f)"$(command jig list -gp 2>/dev/null | sed -n 's/^  //p')"})
        fi
        _describe 'worktree' wts
    }

    _jig_issues() {
        local -a ids
        ids=(${(f)"$(command jig issues --ids 2>/dev/null)"})
        _describe 'issue' ids
    }

    _jig_branches() {
        local -a branches
        branches=(${(f)"$(git branch -a --format='%(refname:short)' 2>/dev/null)"})
        _describe 'branch' branches
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
                open|o|attach|kill)
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
                        '-b[Base branch]:branch:_jig_branches' \
                        '--base=[Base branch]:branch:_jig_branches' \
                        '1:name:' \
                        '2:branch:'
                    ;;
                list|ls)
                    _arguments '--all[Show all]' '-p[Plain output]' '--plain[Plain output]'
                    ;;
                issues)
                    _arguments \
                        '-s[Status]:status:(planned in-progress complete blocked)' \
                        '--status=[Status]:status:(planned in-progress complete blocked)' \
                        '-p[Priority]:priority:(urgent high medium low)' \
                        '--priority=[Priority]:priority:(urgent high medium low)' \
                        '-c[Category]:category:' \
                        '--category=[Category]:category:' \
                        '*-l[Label]:label:' \
                        '*--label=[Label]:label:' \
                        '-i[Interactive]' \
                        '--interactive[Interactive]' \
                        '--ids[IDs only]' \
                        '1:issue:_jig_issues'
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
                        '-b[Base branch]:branch:_jig_branches' \
                        '--base=[Base branch]:branch:_jig_branches' \
                        '-I[Issue]:issue:_jig_issues' \
                        '--issue=[Issue]:issue:_jig_issues' \
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
    set -l wts
    if git rev-parse --is-inside-work-tree &>/dev/null
        set wts (command jig list --plain 2>/dev/null)
    else
        set wts (command jig list -gp 2>/dev/null | sed -n 's/^  //p')
    end
    echo $wts
end

function __jig_issues
    command jig issues --ids 2>/dev/null
end

function __jig_branches
    git branch -a --format='%(refname:short)' 2>/dev/null
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
complete -c jig -n '__jig_needs_command' -a kill -d 'Kill session'
complete -c jig -n '__jig_needs_command' -a issues -d 'Browse issues'
complete -c jig -n '__jig_needs_command' -a init -d 'Initialize'
complete -c jig -n '__jig_needs_command' -a update -d 'Update jig'
complete -c jig -n '__jig_needs_command' -a version -d 'Version'
complete -c jig -n '__jig_needs_command' -a which -d 'Show path'
complete -c jig -n '__jig_needs_command' -a health -d 'Health check'
complete -c jig -n '__jig_needs_command' -a shell-init -d 'Shell init'
complete -c jig -n '__jig_needs_command' -a shell-setup -d 'Shell setup'

# Worktree completions
complete -c jig -n '__jig_using_command open' -a '(__jig_worktrees)' -d 'Worktree'
complete -c jig -n '__jig_using_command attach' -a '(__jig_worktrees)' -d 'Worktree'
complete -c jig -n '__jig_using_command kill' -a '(__jig_worktrees)' -d 'Worktree'
complete -c jig -n '__jig_using_command remove' -a '(__jig_worktrees)' -d 'Worktree'

# Flags
complete -c jig -n '__jig_using_command remove' -l force -s f -d 'Force'
complete -c jig -n '__jig_using_command init' -l force -s f -d 'Force'
complete -c jig -n '__jig_using_command init' -l backup -d 'Backup'
complete -c jig -n '__jig_using_command spawn' -l context -s c -d 'Context'
complete -c jig -n '__jig_using_command spawn' -l issue -s I -a '(__jig_issues)' -d 'Issue'
complete -c jig -n '__jig_using_command spawn' -l base -s b -a '(__jig_branches)' -d 'Base branch'
complete -c jig -n '__jig_using_command spawn' -l auto -d 'Auto-start'
complete -c jig -n '__jig_using_command shell-init' -a 'bash zsh fish' -d 'Shell'
complete -c jig -n '__jig_using_command shell-setup' -l dry-run -d 'Dry run'
complete -c jig -n '__jig_using_command create' -l base -s b -a '(__jig_branches)' -d 'Base branch'
complete -c jig -n '__jig_using_command issues' -a '(__jig_issues)' -d 'Issue'
complete -c jig -n '__jig_using_command issues' -l status -s s -a 'planned in-progress complete blocked' -d 'Status'
complete -c jig -n '__jig_using_command issues' -l priority -s p -a 'urgent high medium low' -d 'Priority'
complete -c jig -n '__jig_using_command issues' -l category -s c -d 'Category'
complete -c jig -n '__jig_using_command issues' -l label -s l -d 'Label'
complete -c jig -n '__jig_using_command issues' -l interactive -s i -d 'Interactive'
complete -c jig -n '__jig_using_command issues' -l ids -d 'IDs only'
complete -c jig -n '__jig_using_command config' -a 'base on-create show' -d 'Config cmd'
"#;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

impl Shell {
    /// Detect the current shell from the `SHELL` environment variable.
    pub fn detect() -> Result<Self, ShellError> {
        let shell_path = std::env::var("SHELL").map_err(|_| ShellError::NoShellEnv)?;
        Self::from_path(&shell_path).ok_or(ShellError::UnsupportedShell(shell_path))
    }

    fn from_path(path: &str) -> Option<Self> {
        let name = path.rsplit('/').next()?;
        match name {
            "bash" => Some(Shell::Bash),
            "zsh" => Some(Shell::Zsh),
            "fish" => Some(Shell::Fish),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Shell::Bash => "bash",
            Shell::Zsh => "zsh",
            Shell::Fish => "fish",
        }
    }

    /// Return the shell init script for this shell.
    pub fn init_script(&self) -> &'static str {
        match self {
            Shell::Bash => BASH_INIT.trim(),
            Shell::Zsh => ZSH_INIT.trim(),
            Shell::Fish => FISH_INIT.trim(),
        }
    }

    /// Path to the shell's config file (e.g. `~/.zshrc`).
    pub fn config_file(&self, home: &Path) -> PathBuf {
        match self {
            Shell::Bash => home.join(".bashrc"),
            Shell::Zsh => home.join(".zshrc"),
            Shell::Fish => home.join(".config/fish/config.fish"),
        }
    }

    /// The integration block to insert into the config file.
    pub fn integration_block(&self) -> String {
        let eval_line = match self {
            Shell::Bash | Shell::Zsh => {
                format!(r#"eval "$(jig shell-init {})""#, self.name())
            }
            Shell::Fish => "jig shell-init fish | source".to_string(),
        };
        format!("{MARKER_START}\n{eval_line}\n{MARKER_END}\n")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    #[error("SHELL environment variable not set")]
    NoShellEnv,
    #[error("Unsupported shell: {0}. Supported: bash, zsh, fish")]
    UnsupportedShell(String),
}

/// Check whether the config file content already has jig integration.
pub fn has_existing_integration(content: &str) -> bool {
    content.contains(MARKER_START)
        || content.contains("jig shell-init")
        || content.contains("eval \"$(jig")
}

/// Find the line number (0-indexed) of the last PATH-related line, if any.
pub fn find_last_path_line(content: &str) -> Option<usize> {
    let path_patterns = [
        "export PATH=",
        "PATH=",
        "path+=",
        "set -gx PATH",
        "fish_add_path",
        "cargo/env",
        "nvm.sh",
        "rbenv init",
        "pyenv init",
        "eval \"$(brew shellenv)\"",
        "eval (brew shellenv)",
    ];

    let mut last_path_line = None;

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        for pattern in &path_patterns {
            if trimmed.contains(pattern) {
                last_path_line = Some(i);
                break;
            }
        }
    }

    last_path_line
}

/// The marker comments used to identify the integration block.
pub fn markers() -> (&'static str, &'static str) {
    (MARKER_START, MARKER_END)
}
