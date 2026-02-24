# State Derivation

**Status:** Complete
**Priority:** High
**Category:** Features
**Epic:** issues/epics/event-system/index.md
**Depends-On:** issues/epics/event-system/3-worker-status-states.md

## Objective

Derive WorkerStatus from event log by applying transition rules to the event stream.

## Background

State is computed, not stored. Given an event log, replay events to determine current status. This ensures consistency and enables time-travel debugging.

## Design

### State Derivation Function

```rust
pub fn derive_status(events: &[Event], config: &HealthConfig) -> WorkerStatus {
    if events.is_empty() {
        return WorkerStatus::Spawned;
    }

    let now = chrono::Utc::now().timestamp();
    let last_event = events.last().unwrap();
    let last_event_age = now - last_event.ts;

    // Check for terminal states first
    if let Some(status) = check_terminal_state(events) {
        return status;
    }

    // Check silence threshold (Stalled)
    if last_event_age > config.silence_threshold_seconds as i64 {
        return WorkerStatus::Stalled;
    }

    // Derive from last event type
    match last_event.event_type.as_str() {
        "stop" => WorkerStatus::Idle,
        "notification" => WorkerStatus::WaitingInput,
        "tool_use_start" | "tool_use_end" => WorkerStatus::Running,
        "commit" | "push" => WorkerStatus::Running,
        "pr_opened" => WorkerStatus::WaitingReview,
        "spawn" => WorkerStatus::Spawned,
        _ => WorkerStatus::Running,
    }
}

fn check_terminal_state(events: &[Event]) -> Option<WorkerStatus> {
    // Look for pr_merged, failed, archived events
    for event in events.iter().rev() {
        match event.event_type.as_str() {
            "pr_merged" => return Some(WorkerStatus::Merged),
            "pr_approved" => return Some(WorkerStatus::Approved),
            "failed" => return Some(WorkerStatus::Failed),
            "archived" => return Some(WorkerStatus::Archived),
            _ => {}
        }
    }
    None
}
```

### State Reducer

For more complex derivation, use a reducer pattern:

```rust
pub struct WorkerState {
    pub status: WorkerStatus,
    pub commit_count: u32,
    pub last_commit_at: Option<i64>,
    pub pr_url: Option<String>,
    pub nudge_counts: HashMap<String, u32>,
}

impl WorkerState {
    pub fn reduce(events: &[Event], config: &HealthConfig) -> Self {
        let mut state = Self::default();

        for event in events {
            state.apply(event);
        }

        // Apply silence check after all events
        state.check_silence(config);

        state
    }

    fn apply(&mut self, event: &Event) {
        match event.event_type.as_str() {
            "spawn" => {
                self.status = WorkerStatus::Spawned;
            }
            "tool_use_start" | "tool_use_end" => {
                self.status = WorkerStatus::Running;
            }
            "commit" => {
                self.status = WorkerStatus::Running;
                self.commit_count += 1;
                self.last_commit_at = Some(event.ts);
            }
            "notification" => {
                self.status = WorkerStatus::WaitingInput;
            }
            "stop" => {
                self.status = WorkerStatus::Idle;
            }
            "pr_opened" => {
                self.status = WorkerStatus::WaitingReview;
                if let Some(url) = event.data.get("pr_url") {
                    self.pr_url = url.as_str().map(String::from);
                }
            }
            "nudge" => {
                if let Some(nudge_type) = event.data.get("nudge_type").and_then(|v| v.as_str()) {
                    *self.nudge_counts.entry(nudge_type.to_string()).or_insert(0) += 1;
                }
            }
            _ => {}
        }
    }

    fn check_silence(&mut self, config: &HealthConfig) {
        // Called after all events processed
        // Implementation checks last event timestamp vs now
    }
}
```

### Periodic Update

State daemon runs periodically to update global workers.json:

```rust
pub fn update_all_workers(config: &GlobalConfig) -> Result<()> {
    let events_dir = global_state_dir().join("events");
    let mut workers_state = WorkersState::load_or_default()?;

    for entry in std::fs::read_dir(&events_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let worker_id = entry.file_name().to_string_lossy().to_string();
            let log = EventLog::new(entry.path().join("events.jsonl"));
            let events = log.read_all()?;
            let state = WorkerState::reduce(&events, &config.health);

            workers_state.update(&worker_id, state);
        }
    }

    workers_state.save()?;
    Ok(())
}
```

## Implementation

**Files:**
- `crates/jig-core/src/events/derive.rs` — state derivation
- `crates/jig-core/src/events/reducer.rs` — WorkerState reducer
- `crates/jig-cli/src/commands/daemon.rs` — periodic update command

**Commands:**
- `jig daemon` — run state derivation loop
- `jig status --derive` — derive status from events (debug)

## Acceptance Criteria

- [ ] `derive_status()` returns correct status from events
- [ ] Silence threshold triggers `Stalled` status
- [ ] Terminal states (Merged, Failed) are sticky
- [ ] `WorkerState::reduce()` computes full state
- [ ] Commit count and PR URL extracted from events
- [ ] `update_all_workers()` scans event dirs and updates state
- [ ] Works with empty event log (returns Spawned)

## Testing

```rust
#[test]
fn test_derive_running() {
    let events = vec![
        Event::new("spawn"),
        Event::new("tool_use_start").with_field("tool", "bash"),
    ];
    let config = HealthConfig::default();

    assert_eq!(derive_status(&events, &config), WorkerStatus::Running);
}

#[test]
fn test_derive_waiting_input() {
    let events = vec![
        Event::new("spawn"),
        Event::new("notification").with_field("message", "Need approval"),
    ];
    let config = HealthConfig::default();

    assert_eq!(derive_status(&events, &config), WorkerStatus::WaitingInput);
}

#[test]
fn test_derive_stalled() {
    let old_ts = chrono::Utc::now().timestamp() - 600; // 10 min ago
    let events = vec![
        Event { ts: old_ts, event_type: "tool_use_end".to_string(), data: Default::default() },
    ];
    let config = HealthConfig { silence_threshold_seconds: 300, ..Default::default() };

    assert_eq!(derive_status(&events, &config), WorkerStatus::Stalled);
}

#[test]
fn test_reducer_commit_count() {
    let events = vec![
        Event::new("spawn"),
        Event::new("commit").with_field("sha", "abc"),
        Event::new("commit").with_field("sha", "def"),
    ];
    let config = HealthConfig::default();
    let state = WorkerState::reduce(&events, &config);

    assert_eq!(state.commit_count, 2);
}
```

## Next Steps

After this ticket:
- Move to ticket 5 (action dispatch)
- Dispatch triggers actions based on derived state
