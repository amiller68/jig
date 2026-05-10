#!/bin/bash
# jig: write stop event to event log
# Claude Code passes JSON on stdin with stop_hook_active, last_assistant_message, cwd, etc.

INPUT=$(cat)

# Find the main repo root (not the worktree) via git common dir
GIT_COMMON=$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)
if [ -n "$GIT_COMMON" ]; then
  REPO=$(basename "$(dirname "$GIT_COMMON")")
else
  REPO=$(basename "$(git rev-parse --show-toplevel 2>/dev/null)" 2>/dev/null || echo "unknown")
fi
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")

JIG_CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/jig"
EVENT_DIR="$JIG_CONFIG_DIR/$REPO/$BRANCH"
mkdir -p "$EVENT_DIR"

printf '{"ts":%d,"type":"stop"}\n' \
  "$(date +%s)" >> "$EVENT_DIR/events.jsonl"
