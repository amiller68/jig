# Claude Code Hooks

**Status:** Complete
**Priority:** High
**Category:** Features
**Epic:** issues/epics/event-system/index.md
**Depends-On:** issues/epics/event-system/1-event-log-format.md

## Objective

Create user-level Claude Code hook templates that emit events to jig's global state.

## Background

Claude Code supports hooks at `~/.claude/hooks/`. These run on events like tool use and notifications. We use them to write structured events that jig can process.

## Design

### Hook Types

| Hook | Claude Event | jig Event |
|------|--------------|-----------|
| `PreToolUse` | Before tool call | `tool_use_start` |
| `PostToolUse` | After tool call | `tool_use_end` |
| `Notification` | Agent notification | `notification` |
| `Stop` | Agent exits | `stop` |

### Hook Scripts

`~/.claude/hooks/PostToolUse`:
```bash
#!/bin/bash
# Write tool_use_end event to jig event log

# Get worker context from environment or git
REPO=$(basename "$(git rev-parse --show-toplevel 2>/dev/null)" || echo "unknown")
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
WORKER_ID="${REPO}-${BRANCH//\//-}"

# Event data from Claude (passed as JSON on stdin or args)
TOOL_NAME="${CLAUDE_TOOL_NAME:-unknown}"
EXIT_CODE="${CLAUDE_EXIT_CODE:-0}"

# Write event
JIG_STATE_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/jig/state"
EVENT_DIR="$JIG_STATE_DIR/events/$WORKER_ID"
mkdir -p "$EVENT_DIR"

cat >> "$EVENT_DIR/events.jsonl" <<EOF
{"ts": $(date +%s), "type": "tool_use_end", "tool": "$TOOL_NAME", "exit_code": $EXIT_CODE}
EOF
```

`~/.claude/hooks/Notification`:
```bash
#!/bin/bash
# Write notification event - this signals agent needs attention

REPO=$(basename "$(git rev-parse --show-toplevel 2>/dev/null)" || echo "unknown")
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
WORKER_ID="${REPO}-${BRANCH//\//-}"

# Message from stdin
MESSAGE=$(cat)

JIG_STATE_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/jig/state"
EVENT_DIR="$JIG_STATE_DIR/events/$WORKER_ID"
mkdir -p "$EVENT_DIR"

# Escape message for JSON
MESSAGE_JSON=$(echo "$MESSAGE" | jq -Rs .)

cat >> "$EVENT_DIR/events.jsonl" <<EOF
{"ts": $(date +%s), "type": "notification", "message": $MESSAGE_JSON}
EOF
```

`~/.claude/hooks/Stop`:
```bash
#!/bin/bash
# Write stop event when agent exits

REPO=$(basename "$(git rev-parse --show-toplevel 2>/dev/null)" || echo "unknown")
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
WORKER_ID="${REPO}-${BRANCH//\//-}"

REASON="${CLAUDE_STOP_REASON:-unknown}"

JIG_STATE_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/jig/state"
EVENT_DIR="$JIG_STATE_DIR/events/$WORKER_ID"
mkdir -p "$EVENT_DIR"

cat >> "$EVENT_DIR/events.jsonl" <<EOF
{"ts": $(date +%s), "type": "stop", "reason": "$REASON"}
EOF
```

### Installation

`jig hooks install-claude`:
```rust
pub fn install_claude_hooks() -> Result<()> {
    let hooks_dir = dirs::home_dir()
        .ok_or(Error::NoHomeDir)?
        .join(".claude/hooks");

    std::fs::create_dir_all(&hooks_dir)?;

    for (name, content) in CLAUDE_HOOK_TEMPLATES {
        let path = hooks_dir.join(name);
        if !path.exists() {
            std::fs::write(&path, content)?;
            make_executable(&path)?;
        }
    }

    Ok(())
}
```

## Implementation

**Files:**
- `crates/jig-core/src/hooks/claude.rs` — Claude hook templates
- `crates/jig-cli/src/commands/hooks.rs` — `install-claude` subcommand

**Templates stored as constants:**
```rust
pub const CLAUDE_HOOK_TEMPLATES: &[(&str, &str)] = &[
    ("PostToolUse", include_str!("templates/PostToolUse.sh")),
    ("Notification", include_str!("templates/Notification.sh")),
    ("Stop", include_str!("templates/Stop.sh")),
];
```

## Acceptance Criteria

- [ ] Hook templates write valid JSONL events
- [ ] Hooks determine repo/branch from git context
- [ ] Worker ID sanitizes branch names
- [ ] `jig hooks install-claude` installs to `~/.claude/hooks/`
- [ ] Hooks are executable after installation
- [ ] Existing hooks not overwritten (warn instead)
- [ ] Works without jq installed (fallback escaping)

## Testing

```bash
# Manual testing
cd /tmp/test-repo && git init && git checkout -b feature/test

# Simulate hook
export CLAUDE_TOOL_NAME="bash"
export CLAUDE_EXIT_CODE="0"
~/.claude/hooks/PostToolUse

# Check event was written
cat ~/.config/jig/state/events/test-repo-feature-test/events.jsonl
```

```rust
#[test]
fn test_install_claude_hooks() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_var("HOME", temp.path());

    install_claude_hooks().unwrap();

    let hook_path = temp.path().join(".claude/hooks/PostToolUse");
    assert!(hook_path.exists());
    assert!(hook_path.metadata().unwrap().permissions().mode() & 0o111 != 0);
}
```

## Next Steps

After this ticket:
- Move to ticket 3 (worker status states)
- State derivation will read these events
