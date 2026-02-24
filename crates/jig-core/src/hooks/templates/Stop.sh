#!/bin/bash
# jig: write stop event to event log

REPO=$(basename "$(git rev-parse --show-toplevel 2>/dev/null)" || echo "unknown")
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
WORKER_ID="${REPO}-${BRANCH//\//-}"

REASON="${CLAUDE_STOP_REASON:-unknown}"

JIG_STATE_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/jig/state"
EVENT_DIR="$JIG_STATE_DIR/events/$WORKER_ID"
mkdir -p "$EVENT_DIR"

printf '{"ts":%d,"type":"stop","reason":"%s"}\n' \
  "$(date +%s)" "$REASON" >> "$EVENT_DIR/events.jsonl"
