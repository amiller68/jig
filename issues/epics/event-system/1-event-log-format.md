# Event Log Format

**Status:** Planned
**Priority:** High
**Category:** Features
**Epic:** issues/epics/event-system/index.md
**Depends-On:** issues/epics/event-system/0-global-state.md

## Objective

Define the JSONL event log schema that all event sources (Claude hooks, git hooks, GitHub) write to.

## Background

The event log is the source of truth for worker state. Events are append-only, state is derived by replaying/reducing events.

## Design

### Event Schema

Each line in `events.jsonl` is a JSON object:

```jsonl
{"ts": 1708358400, "type": "spawn", "repo": "jig", "worker": "feature-auth", "issue": "ABC-123"}
{"ts": 1708358401, "type": "tool_use_start", "tool": "bash", "input_preview": "cargo test"}
{"ts": 1708358450, "type": "tool_use_end", "tool": "bash", "exit_code": 0}
{"ts": 1708359000, "type": "commit", "sha": "abc123", "message": "feat: add auth"}
{"ts": 1708360000, "type": "notification", "message": "Waiting for approval..."}
{"ts": 1708361000, "type": "stop", "reason": "completed"}
```

### Event Types

| Type | Source | Fields | Meaning |
|------|--------|--------|---------|
| `spawn` | jig | repo, worker, issue | Worker started |
| `tool_use_start` | Claude hook | tool, input_preview | Tool call began |
| `tool_use_end` | Claude hook | tool, exit_code | Tool call finished |
| `commit` | git hook | sha, message | Commit created |
| `push` | git hook | remote, branch | Push completed |
| `pr_opened` | GitHub/jig | pr_url, pr_number | PR created |
| `notification` | Claude hook | message | Agent surfaced info |
| `stop` | Claude hook | reason | Agent exited |
| `nudge` | jig | nudge_type, count | Nudge sent |
| `ci_status` | GitHub | status, url | CI result |
| `review` | GitHub | state, comments | PR review |

### Storage Location

Per-worker event logs:
```
~/.config/jig/state/events/
└── jig-feature-auth/
    └── events.jsonl
```

Worker ID format: `<repo>-<branch>` with `/` replaced by `-`.

### Data Structures

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub ts: i64,
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(flatten)]
    pub data: serde_json::Value,
}

impl Event {
    pub fn new(event_type: &str) -> Self {
        Self {
            ts: chrono::Utc::now().timestamp(),
            event_type: event_type.to_string(),
            data: serde_json::Value::Object(Default::default()),
        }
    }

    pub fn with_field(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        if let serde_json::Value::Object(ref mut map) = self.data {
            map.insert(key.to_string(), value.into());
        }
        self
    }
}

pub struct EventLog {
    path: PathBuf,
}

impl EventLog {
    pub fn for_worker(repo: &str, worker: &str) -> Self {
        let worker_id = format!("{}-{}", repo, worker.replace('/', "-"));
        let path = global_state_dir()
            .join("events")
            .join(&worker_id)
            .join("events.jsonl");
        Self { path }
    }

    pub fn append(&self, event: &Event) -> Result<()> {
        std::fs::create_dir_all(self.path.parent().unwrap())?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{}", serde_json::to_string(event)?)?;
        Ok(())
    }

    pub fn read_all(&self) -> Result<Vec<Event>> {
        if !self.path.exists() {
            return Ok(vec![]);
        }
        let content = std::fs::read_to_string(&self.path)?;
        content
            .lines()
            .map(|line| serde_json::from_str(line).map_err(Into::into))
            .collect()
    }

    pub fn last_event(&self) -> Result<Option<Event>> {
        Ok(self.read_all()?.into_iter().last())
    }
}
```

## Implementation

**Files:**
- `crates/jig-core/src/events/mod.rs` — module
- `crates/jig-core/src/events/schema.rs` — Event struct
- `crates/jig-core/src/events/log.rs` — EventLog read/write

## Acceptance Criteria

- [ ] Event struct with ts, type, and flexible data fields
- [ ] EventLog.append() writes JSONL line
- [ ] EventLog.read_all() parses all events
- [ ] EventLog.last_event() returns most recent
- [ ] Events stored per-worker in global state dir
- [ ] Worker ID sanitizes branch names (no `/`)

## Testing

```rust
#[test]
fn test_event_builder() {
    let event = Event::new("commit")
        .with_field("sha", "abc123")
        .with_field("message", "feat: thing");

    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"commit\""));
    assert!(json.contains("\"sha\":\"abc123\""));
}

#[test]
fn test_event_log_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    let log = EventLog { path: temp.path().join("events.jsonl") };

    let event = Event::new("spawn").with_field("worker", "test");
    log.append(&event).unwrap();

    let events = log.read_all().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "spawn");
}
```

## Next Steps

After this ticket:
- Move to ticket 2 (Claude Code hooks)
- Hooks will write events in this format
