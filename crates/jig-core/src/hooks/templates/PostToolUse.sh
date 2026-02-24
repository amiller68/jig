#!/bin/bash
# jig: write tool_use_end event to event log

REPO=$(basename "$(git rev-parse --show-toplevel 2>/dev/null)" || echo "unknown")
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
WORKER_ID="${REPO}-${BRANCH//\//-}"

TOOL_NAME="${CLAUDE_TOOL_NAME:-unknown}"
EXIT_CODE="${CLAUDE_EXIT_CODE:-0}"

JIG_STATE_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/jig/state"
EVENT_DIR="$JIG_STATE_DIR/events/$WORKER_ID"
mkdir -p "$EVENT_DIR"

printf '{"ts":%d,"type":"tool_use_end","tool":"%s","exit_code":%s}\n' \
  "$(date +%s)" "$TOOL_NAME" "$EXIT_CODE" >> "$EVENT_DIR/events.jsonl"
