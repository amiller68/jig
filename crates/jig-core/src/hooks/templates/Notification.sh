#!/bin/bash
# jig: write notification event to event log
# Claude Code passes JSON on stdin with message, notification_type, cwd, etc.

INPUT=$(cat)

REPO=$(basename "$(git rev-parse --show-toplevel 2>/dev/null)" 2>/dev/null || echo "unknown")
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
WORKER_ID="${REPO}-${BRANCH//\//-}"

JIG_STATE_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/jig/state"
EVENT_DIR="$JIG_STATE_DIR/events/$WORKER_ID"
mkdir -p "$EVENT_DIR"

if command -v jq &>/dev/null; then
  MESSAGE_JSON=$(printf '%s' "$INPUT" | jq -r '.message // "unknown"' | jq -Rs .)
else
  MESSAGE_JSON="\"notification\""
fi

printf '{"ts":%d,"type":"notification","message":%s}\n' \
  "$(date +%s)" "$MESSAGE_JSON" >> "$EVENT_DIR/events.jsonl"
