#!/bin/bash
# jig: write stop event to event log
# Claude Code passes JSON on stdin with stop_hook_active, last_assistant_message, cwd, etc.

INPUT=$(cat)

REPO=$(basename "$(git rev-parse --show-toplevel 2>/dev/null)" 2>/dev/null || echo "unknown")
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
WORKER_ID="${REPO}-${BRANCH//\//-}"

JIG_STATE_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/jig/state"
EVENT_DIR="$JIG_STATE_DIR/events/$WORKER_ID"
mkdir -p "$EVENT_DIR"

printf '{"ts":%d,"type":"stop"}\n' \
  "$(date +%s)" >> "$EVENT_DIR/events.jsonl"
