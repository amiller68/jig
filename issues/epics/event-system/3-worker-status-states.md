# Worker Status States

**Status:** Planned
**Priority:** High
**Category:** Features
**Epic:** issues/epics/event-system/index.md
**Depends-On:** issues/epics/event-system/2-claude-hooks.md

## Objective

Expand WorkerStatus enum to include event-driven states: `WaitingInput` and `Stalled`.

## Background

Current states are coarse. Event-driven approach enables finer granularity:

| Old | New | Trigger |
|-----|-----|---------|
| Running | `Running` | Tool use events flowing |
| — | `Idle` | Stop event, at shell prompt |
| — | `WaitingInput` | Notification event fired |
| — | `Stalled` | Silence threshold exceeded |
| WaitingReview | `WaitingReview` | PR opened |

## Design

### Status Enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    /// Worker just spawned, no events yet
    Spawned,

    /// Tool use events flowing, actively working
    Running,

    /// Stop event fired, agent at shell prompt
    Idle,

    /// Notification event fired, agent waiting for input
    WaitingInput,

    /// No events for silence_threshold, agent may be stuck
    Stalled,

    /// PR opened, waiting for human review
    WaitingReview,

    /// PR approved, ready to merge
    Approved,

    /// PR merged successfully
    Merged,

    /// Worker failed or was killed
    Failed,

    /// Worker archived/cleaned up
    Archived,
}

impl WorkerStatus {
    /// States that indicate worker needs attention
    pub fn needs_attention(&self) -> bool {
        matches!(self, Self::WaitingInput | Self::Stalled | Self::Failed)
    }

    /// States that indicate worker is actively working
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Running | Self::Spawned)
    }

    /// States that indicate work is complete
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Merged | Self::Archived | Self::Failed)
    }
}
```

### State Transitions

```
                    ┌─────────────────────────────────────────┐
                    │                                         │
                    ▼                                         │
Spawned ──▶ Running ──┬──▶ Idle ──┬──▶ WaitingReview ──▶ Approved ──▶ Merged
                      │           │         │
                      │           │         └──▶ Failed
                      │           │
                      ├──▶ WaitingInput ──┬──▶ Running (after input)
                      │                   │
                      │                   └──▶ Failed (max nudges)
                      │
                      └──▶ Stalled ──┬──▶ Running (resumed)
                                     │
                                     └──▶ Failed (timeout)
```

### Transition Rules

| From | To | Trigger |
|------|----|---------|
| Spawned | Running | First `tool_use_start` event |
| Running | Idle | `stop` event |
| Running | WaitingInput | `notification` event |
| Running | Stalled | No events for `silence_threshold` |
| Running | WaitingReview | PR opened (from git/GitHub) |
| Idle | Running | `tool_use_start` event |
| WaitingInput | Running | `tool_use_start` after nudge |
| WaitingInput | Failed | Max nudges exceeded |
| Stalled | Running | Any new event |
| Stalled | Failed | Timeout exceeded |
| WaitingReview | Approved | PR approved |
| Approved | Merged | PR merged |
| Any | Failed | Error condition |
| Any | Archived | Manual cleanup |

## Implementation

**Files:**
- `crates/jig-core/src/worker/status.rs` — WorkerStatus enum
- Update existing code to use new states

**Migration:**
```rust
impl WorkerStatus {
    /// Migrate old status strings to new enum
    pub fn from_legacy(s: &str) -> Self {
        match s {
            "spawned" => Self::Spawned,
            "running" => Self::Running,
            "waiting_review" => Self::WaitingReview,
            "approved" => Self::Approved,
            "merged" => Self::Merged,
            "failed" => Self::Failed,
            "archived" => Self::Archived,
            // New states default to Running for migration
            _ => Self::Running,
        }
    }
}
```

## Acceptance Criteria

- [ ] WorkerStatus enum with all new states
- [ ] `needs_attention()` returns true for WaitingInput/Stalled
- [ ] `is_active()` returns true for Running/Spawned
- [ ] `is_terminal()` returns true for Merged/Archived/Failed
- [ ] Serializes to snake_case strings
- [ ] Legacy status strings migrate correctly
- [ ] Existing tests pass with new enum

## Testing

```rust
#[test]
fn test_status_needs_attention() {
    assert!(WorkerStatus::WaitingInput.needs_attention());
    assert!(WorkerStatus::Stalled.needs_attention());
    assert!(!WorkerStatus::Running.needs_attention());
}

#[test]
fn test_status_serialization() {
    let status = WorkerStatus::WaitingInput;
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, "\"waiting_input\"");

    let parsed: WorkerStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, WorkerStatus::WaitingInput);
}

#[test]
fn test_legacy_migration() {
    assert_eq!(WorkerStatus::from_legacy("running"), WorkerStatus::Running);
    assert_eq!(WorkerStatus::from_legacy("unknown"), WorkerStatus::Running);
}
```

## Next Steps

After this ticket:
- Move to ticket 4 (state derivation)
- Derivation will apply events to produce these states
