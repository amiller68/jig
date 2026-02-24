#!/bin/bash
# jig: write tool_use_end event to event log
# Claude Code passes JSON on stdin with tool_name, tool_input, cwd, etc.

INPUT=$(cat)

REPO=$(basename "$(git rev-parse --show-toplevel 2>/dev/null)" 2>/dev/null || echo "unknown")
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
WORKER_ID="${REPO}-${BRANCH//\//-}"

if command -v jq &>/dev/null; then
  TOOL_NAME=$(printf '%s' "$INPUT" | jq -r '.tool_name // "unknown"')
else
  TOOL_NAME="unknown"
fi

JIG_STATE_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/jig/state"
EVENT_DIR="$JIG_STATE_DIR/events/$WORKER_ID"
mkdir -p "$EVENT_DIR"

printf '{"ts":%d,"type":"tool_use_end","tool":"%s"}\n' \
  "$(date +%s)" "$TOOL_NAME" >> "$EVENT_DIR/events.jsonl"
