#!/bin/bash
# jig: write notification event to event log
# Claude Code passes JSON on stdin with message, notification_type, cwd, etc.

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

if command -v jq &>/dev/null; then
  MESSAGE_JSON=$(printf '%s' "$INPUT" | jq -r '.message // "unknown"' | jq -Rs .)
else
  MESSAGE_JSON="\"notification\""
fi

printf '{"ts":%d,"type":"notification","message":%s}\n' \
  "$(date +%s)" "$MESSAGE_JSON" >> "$EVENT_DIR/events.jsonl"
