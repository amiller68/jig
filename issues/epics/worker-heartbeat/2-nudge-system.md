# Nudge System

**Status:** Planned  
**Priority:** High  
**Category:** Features  
**Epic:** issues/epics/worker-heartbeat/index.md  
**Depends-On:** issues/epics/worker-heartbeat/1-tmux-detection.md

## Objective

Implement nudge system that sends contextual messages to idle/stuck workers and escalates after max attempts.

## Background

When workers are detected as idle/stuck:
- Send appropriate message via tmux
- Track nudge count per type
- Escalate to human after max nudges (default 3)
- Auto-approve safe stuck prompts

## Design

### Nudge Logic

```rust
pub fn should_nudge_worker(
    worker: &WorkerHealth,
    state: WorkerState,
    config: &HealthConfig
) -> Option<NudgeType> {
    let age_hours = worker.age_hours();
    let hours_since_commit = worker.hours_since_commit();
    
    match state {
        WorkerState::Stuck => {
            if worker.get_nudge_count("stuck") < config.max_nudges {
                Some(NudgeType::Stuck)
            } else {
                None  // Max nudges reached, escalate
            }
        },
        WorkerState::Idle => {
            // New worker with no commits after threshold
            if worker.commit_count == 0 && age_hours >= config.idle_new_worker_hours {
                if worker.get_nudge_count("idle") < config.max_nudges {
                    return Some(NudgeType::Idle);
                }
            }
            // Existing worker, no commits in threshold
            if worker.commit_count > 0 && hours_since_commit >= config.idle_existing_hours {
                if worker.get_nudge_count("idle") < config.max_nudges {
                    return Some(NudgeType::Idle);
                }
            }
            None
        },
        WorkerState::Working => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NudgeType {
    Stuck,  // Stuck at interactive prompt
    Idle,   // Idle at shell, no activity
}
```

### Sending Nudges

```rust
use std::process::Command;
use std::thread;
use std::time::Duration;

pub fn send_nudge(
    session: &str,
    window: &str,
    message: &str
) -> Result<()> {
    let target = format!("{}:{}", session, window);
    
    // Send message as literal text
    Command::new("tmux")
        .args(&["send-keys", "-t", &target, "-l", message])
        .status()?;
    
    // Small delay before Enter (prevents paste interpretation)
    thread::sleep(Duration::from_millis(100));
    
    // Send Enter
    Command::new("tmux")
        .args(&["send-keys", "-t", &target, "Enter"])
        .status()?;
    
    Ok(())
}

pub fn auto_approve(session: &str, window: &str) -> Result<()> {
    let target = format!("{}:{}", session, window);
    
    // Send "1" + Enter to approve
    Command::new("tmux")
        .args(&["send-keys", "-t", &target, "1", "Enter"])
        .status()?;
    
    Ok(())
}
```

### Nudge Messages

```rust
pub fn build_nudge_message(
    nudge_type: NudgeType,
    worker: &WorkerHealth,
    max_nudges: u32
) -> String {
    let nudge_count = match nudge_type {
        NudgeType::Stuck => worker.get_nudge_count("stuck"),
        NudgeType::Idle => worker.get_nudge_count("idle"),
    };
    
    match nudge_type {
        NudgeType::Stuck => {
            format!(
                "STUCK PROMPT DETECTED: You appear to be waiting at an interactive prompt. \
                Auto-approving... (nudge {}/{})",
                nudge_count + 1, max_nudges
            )
        },
        NudgeType::Idle => {
            let reason = if worker.commit_count == 0 {
                format!("no commits after {}h", worker.age_hours())
            } else {
                format!("no commits in {}h", worker.hours_since_commit())
            };
            
            format!(
                "STATUS CHECK: You've been idle for a while ({}). \n\
                What's the current state? (nudge {}/{})\n\n\
                1. If ready: commit (conventional format), push, create PR, update issue, call /review\n\
                2. If stuck: explain the blocker\n\
                3. If complete: commit, push, create PR, update issue, call /review",
                reason, nudge_count + 1, max_nudges
            )
        },
    }
}
```

### Configuration

```toml
[health]
maxNudges = 3
idleNewWorkerHours = 3  # Nudge new workers after 3h with no commits
idleExistingHours = 6   # Nudge existing workers after 6h since last commit
autoApproveStuck = true  # Auto-approve stuck prompts
```

## Implementation

**Files:**
- `crates/jig-core/src/health/nudge.rs` - nudge logic
- `crates/jig-cli/src/commands/health.rs` - health command

**Health check integration:**
```rust
pub fn check_and_nudge_worker(
    repo_name: &str,
    worker_name: &str,
    health: &mut WorkerHealth,
    config: &HealthConfig,
    detector: &WorkerDetector
) -> Result<Option<Alert>> {
    let state = check_worker(repo_name, worker_name, detector)?;
    
    if let Some(nudge_type) = should_nudge_worker(health, state, config) {
        let nudge_count = match nudge_type {
            NudgeType::Stuck => health.get_nudge_count("stuck"),
            NudgeType::Idle => health.get_nudge_count("idle"),
        };
        
        if nudge_count < config.max_nudges {
            // Send nudge
            if nudge_type == NudgeType::Stuck && config.auto_approve_stuck {
                auto_approve(&format!("jig-{}", repo_name), worker_name)?;
            } else {
                let message = build_nudge_message(nudge_type, health, config.max_nudges);
                send_nudge(&format!("jig-{}", repo_name), worker_name, &message)?;
            }
            
            // Increment nudge count
            let key = match nudge_type {
                NudgeType::Stuck => "stuck",
                NudgeType::Idle => "idle",
            };
            health.increment_nudge(key);
            
            Ok(None)
        } else {
            // Max nudges reached, escalate
            Ok(Some(Alert::MaxNudges {
                worker: worker_name.to_string(),
                nudge_type,
                count: nudge_count,
            }))
        }
    } else {
        Ok(None)
    }
}
```

## Acceptance Criteria

- [ ] `should_nudge_worker()` decides when to nudge
- [ ] `send_nudge()` sends message via tmux
- [ ] `auto_approve()` sends "1" + Enter for stuck prompts
- [ ] `build_nudge_message()` creates contextual messages
- [ ] Nudge counts incremented after sending
- [ ] Escalate (return Alert) after max nudges
- [ ] Auto-approval configurable via `jig.toml`
- [ ] Idle thresholds configurable

## Testing

```rust
#[test]
fn test_should_nudge_idle_new_worker() {
    let config = HealthConfig {
        max_nudges: 3,
        idle_new_worker_hours: 3,
        ..Default::default()
    };
    
    let mut worker = WorkerHealth::new(now() - 4 * 3600);  // 4 hours old
    worker.commit_count = 0;
    
    let nudge = should_nudge_worker(&worker, WorkerState::Idle, &config);
    assert_eq!(nudge, Some(NudgeType::Idle));
}

#[test]
fn test_max_nudges_reached() {
    let config = HealthConfig {
        max_nudges: 3,
        ..Default::default()
    };
    
    let mut worker = WorkerHealth::new(now() - 4 * 3600);
    worker.nudges.insert("idle".to_string(), 3);  // Already nudged 3 times
    
    let nudge = should_nudge_worker(&worker, WorkerState::Idle, &config);
    assert_eq!(nudge, None);  // Should escalate, not nudge
}

#[test]
fn test_build_idle_message() {
    let mut worker = WorkerHealth::new(now() - 4 * 3600);
    worker.commit_count = 0;
    
    let message = build_nudge_message(NudgeType::Idle, &worker, 3);
    assert!(message.contains("no commits after"));
    assert!(message.contains("nudge 1/3"));
}
```

## Next Steps

After this ticket:
- Move to ticket 3 (git hooks integration)
- Git hooks will update metrics and reset nudge counts
