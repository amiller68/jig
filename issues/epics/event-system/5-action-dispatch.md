# Action Dispatch

**Status:** Complete
**Priority:** Medium
**Category:** Features
**Epic:** issues/epics/event-system/index.md
**Depends-On:** issues/epics/event-system/4-state-derivation.md

## Objective

Trigger controller actions (nudge, restart, alert) based on state transitions.

## Background

When state changes, jig may need to act:
- `WaitingInput` → send nudge via tmux
- `Stalled` → alert human, maybe restart
- `Failed` → notify, cleanup

## Design

### Action Types

```rust
pub enum Action {
    /// Send message to worker via tmux
    Nudge { worker_id: String, message: String },

    /// Auto-approve stuck prompt
    AutoApprove { worker_id: String },

    /// Emit notification event
    Notify { event: NotificationEvent },

    /// Kill and restart worker
    Restart { worker_id: String, reason: String },

    /// Archive worker and cleanup
    Cleanup { worker_id: String },
}
```

### Dispatch Rules

```rust
pub fn dispatch_actions(
    worker_id: &str,
    old_state: &WorkerState,
    new_state: &WorkerState,
    config: &GlobalConfig,
) -> Vec<Action> {
    let mut actions = vec![];

    // State changed to WaitingInput
    if old_state.status != WorkerStatus::WaitingInput
        && new_state.status == WorkerStatus::WaitingInput
    {
        let nudge_count = new_state.nudge_counts.get("waiting_input").unwrap_or(&0);

        if *nudge_count < config.health.max_nudges {
            if config.health.auto_approve_prompts {
                actions.push(Action::AutoApprove {
                    worker_id: worker_id.to_string(),
                });
            } else {
                actions.push(Action::Nudge {
                    worker_id: worker_id.to_string(),
                    message: "Waiting for input. Please respond or exit.".to_string(),
                });
            }
        } else {
            actions.push(Action::Notify {
                event: NotificationEvent::needs_intervention(
                    worker_id,
                    "Max nudges reached, needs human attention",
                ),
            });
        }
    }

    // State changed to Stalled
    if old_state.status != WorkerStatus::Stalled
        && new_state.status == WorkerStatus::Stalled
    {
        actions.push(Action::Notify {
            event: NotificationEvent::needs_intervention(
                worker_id,
                "Worker stalled - no activity",
            ),
        });
    }

    // PR opened
    if old_state.pr_url.is_none() && new_state.pr_url.is_some() {
        actions.push(Action::Notify {
            event: NotificationEvent::pr_opened(
                worker_id,
                new_state.pr_url.as_ref().unwrap(),
            ),
        });
    }

    actions
}
```

### Action Executor

```rust
pub struct ActionExecutor {
    tmux: TmuxController,
    notifier: Notifier,
}

impl ActionExecutor {
    pub fn execute(&self, action: Action) -> Result<()> {
        match action {
            Action::Nudge { worker_id, message } => {
                self.tmux.send_keys(&worker_id, &message)?;
                self.tmux.send_keys(&worker_id, "Enter")?;
            }
            Action::AutoApprove { worker_id } => {
                self.tmux.send_keys(&worker_id, "1")?;
                self.tmux.send_keys(&worker_id, "Enter")?;
            }
            Action::Notify { event } => {
                self.notifier.emit(event)?;
            }
            Action::Restart { worker_id, reason } => {
                self.tmux.kill_window(&worker_id)?;
                // Respawn logic here
            }
            Action::Cleanup { worker_id } => {
                self.tmux.kill_window(&worker_id)?;
                // Archive worktree
            }
        }
        Ok(())
    }
}
```

## Implementation

**Files:**
- `crates/jig-core/src/dispatch/mod.rs` — module
- `crates/jig-core/src/dispatch/actions.rs` — Action enum
- `crates/jig-core/src/dispatch/rules.rs` — dispatch_actions()
- `crates/jig-core/src/dispatch/executor.rs` — ActionExecutor

## Acceptance Criteria

- [ ] Action enum with all action types
- [ ] `dispatch_actions()` returns actions for state transitions
- [ ] WaitingInput triggers nudge or auto-approve
- [ ] Stalled triggers notification
- [ ] PR opened triggers notification
- [ ] Max nudges triggers escalation notification
- [ ] ActionExecutor executes all action types
- [ ] Actions are logged for debugging

## Testing

```rust
#[test]
fn test_dispatch_waiting_input() {
    let old = WorkerState { status: WorkerStatus::Running, ..Default::default() };
    let new = WorkerState { status: WorkerStatus::WaitingInput, ..Default::default() };
    let config = GlobalConfig::default();

    let actions = dispatch_actions("test", &old, &new, &config);

    assert!(actions.iter().any(|a| matches!(a, Action::Nudge { .. })));
}

#[test]
fn test_dispatch_max_nudges() {
    let old = WorkerState { status: WorkerStatus::Running, ..Default::default() };
    let mut new = WorkerState { status: WorkerStatus::WaitingInput, ..Default::default() };
    new.nudge_counts.insert("waiting_input".to_string(), 3);

    let config = GlobalConfig {
        health: HealthConfig { max_nudges: 3, ..Default::default() },
        ..Default::default()
    };

    let actions = dispatch_actions("test", &old, &new, &config);

    assert!(actions.iter().any(|a| matches!(a, Action::Notify { .. })));
}
```

## Next Steps

After this ticket:
- Move to ticket 6 (notification queue)
- Notifications from dispatch are written to queue
