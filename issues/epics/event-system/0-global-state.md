# Global State Structure

**Status:** Planned
**Priority:** High
**Category:** Features
**Epic:** issues/epics/event-system/index.md
**Depends-On:** issues/features/global-commands.md

## Objective

Establish the global state directory structure at `~/.config/jig/` for cross-repo state aggregation.

## Background

State must live outside VCS so:
- Hooks can be configured once, work everywhere
- External processes watch ONE location, not N repos
- Notifications aggregate across all projects

## Design

### Directory Structure

```
~/.config/jig/
├── config.toml              # Global jig configuration
├── repos.json               # Repository registry (from global-commands)
├── hooks/                   # User hook scripts
│   └── notify.sh            # Example notification script
└── state/
    ├── workers.json         # Aggregated worker state (all repos)
    ├── notifications.jsonl  # Notification event queue
    └── events/              # Per-worker event logs
        └── <repo>-<worker>/
            └── events.jsonl
```

### Config Schema

`~/.config/jig/config.toml`:
```toml
[health]
silence_threshold_seconds = 300    # Stalled after 5 min silence
max_nudges = 3

[notify]
exec = "~/.config/jig/hooks/notify.sh"
# webhook = "http://localhost:8080/notify"
events = ["needs_intervention", "pr_opened", "work_started"]
```

### Workers State Schema

`~/.config/jig/state/workers.json`:
```json
{
  "version": "1",
  "workers": {
    "jig/feature-auth": {
      "repo": "jig",
      "branch": "feature-auth",
      "status": "running",
      "issue": "ABC-123",
      "pr_url": null,
      "started_at": 1708358400,
      "last_event_at": 1708362000,
      "nudge_counts": {}
    }
  }
}
```

## Implementation

**Files:**
- `crates/jig-core/src/global/mod.rs` — module
- `crates/jig-core/src/global/config.rs` — global config
- `crates/jig-core/src/global/state.rs` — workers state
- `crates/jig-core/src/global/paths.rs` — XDG path helpers

**Key functions:**
```rust
pub fn global_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("jig")
}

pub fn global_state_dir() -> PathBuf {
    global_config_dir().join("state")
}

pub fn ensure_global_dirs() -> Result<()> {
    let dirs = [
        global_config_dir(),
        global_state_dir(),
        global_state_dir().join("events"),
    ];
    for dir in dirs {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(())
}
```

## Acceptance Criteria

- [ ] `global_config_dir()` returns `~/.config/jig`
- [ ] `global_state_dir()` returns `~/.config/jig/state`
- [ ] `ensure_global_dirs()` creates directory structure
- [ ] GlobalConfig loads from `config.toml`
- [ ] WorkersState loads/saves `workers.json`
- [ ] State keyed by `repo/branch` for uniqueness
- [ ] Works on macOS and Linux (XDG compliance)

## Testing

```rust
#[test]
fn test_global_paths() {
    let config = global_config_dir();
    assert!(config.ends_with("jig"));

    let state = global_state_dir();
    assert!(state.ends_with("state"));
}

#[test]
fn test_workers_state_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("workers.json");

    let mut state = WorkersState::new();
    state.set_worker("jig/feature", WorkerState {
        status: "running".to_string(),
        ..Default::default()
    });
    state.save(&path).unwrap();

    let loaded = WorkersState::load(&path).unwrap();
    assert!(loaded.workers.contains_key("jig/feature"));
}
```

## Next Steps

After this ticket:
- Move to ticket 1 (event log format)
- Events will be stored in `state/events/<repo>-<worker>/`
