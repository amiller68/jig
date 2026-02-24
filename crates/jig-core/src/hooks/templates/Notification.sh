#!/bin/bash
# jig: write notification event to event log

REPO=$(basename "$(git rev-parse --show-toplevel 2>/dev/null)" || echo "unknown")
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
WORKER_ID="${REPO}-${BRANCH//\//-}"

MESSAGE=$(cat)

JIG_STATE_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/jig/state"
EVENT_DIR="$JIG_STATE_DIR/events/$WORKER_ID"
mkdir -p "$EVENT_DIR"

# Escape message for JSON: use jq if available, otherwise basic escaping
if command -v jq &>/dev/null; then
  MESSAGE_JSON=$(printf '%s' "$MESSAGE" | jq -Rs .)
else
  MESSAGE_JSON=$(printf '%s' "$MESSAGE" | sed 's/\\/\\\\/g; s/"/\\"/g; s/\t/\\t/g; s/$/\\n/' | tr -d '\n' | sed 's/\\n$//')
  MESSAGE_JSON="\"$MESSAGE_JSON\""
fi

printf '{"ts":%d,"type":"notification","message":%s}\n' \
  "$(date +%s)" "$MESSAGE_JSON" >> "$EVENT_DIR/events.jsonl"
