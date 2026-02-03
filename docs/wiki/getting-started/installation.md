# Installation

## Requirements

- Git
- Bash or Zsh

For `wt spawn` (multi-agent orchestration):
- tmux
- jq
- `claude` CLI
- `gh` CLI

## Install

```bash
curl -sSf https://raw.githubusercontent.com/amiller68/worktree/main/install.sh | bash
```

Restart your shell or run:

```bash
source ~/.zshrc  # or ~/.bashrc
```

## Verify

```bash
wt version
```

## Update

```bash
wt update
```

## Uninstall

```bash
rm -rf ~/.local/share/worktree
rm ~/.local/bin/_wt
# Remove source lines from ~/.bashrc and ~/.zshrc
```
