#!/bin/bash
# jig: write tool_use_end event to event log
# Claude Code passes JSON on stdin with tool_name, tool_input, cwd, etc.

INPUT=$(cat)

# Find the main repo root (not the worktree) via git common dir
GIT_COMMON=$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)
if [ -n "$GIT_COMMON" ]; then
  REPO=$(basename "$(dirname "$GIT_COMMON")")
else
  REPO=$(basename "$(git rev-parse --show-toplevel 2>/dev/null)" 2>/dev/null || echo "unknown")
fi
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")

if command -v jq &>/dev/null; then
  TOOL_NAME=$(printf '%s' "$INPUT" | jq -r '.tool_name // "unknown"')
else
  TOOL_NAME="unknown"
fi

JIG_CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/jig"
EVENT_DIR="$JIG_CONFIG_DIR/$REPO/$BRANCH"
mkdir -p "$EVENT_DIR"

printf '{"ts":%d,"type":"tool_use_end","tool":"%s"}\n' \
  "$(date +%s)" "$TOOL_NAME" >> "$EVENT_DIR/events.jsonl"
