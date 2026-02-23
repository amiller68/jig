# Notification Queue

**Status:** Planned
**Priority:** Medium
**Category:** Features
**Epic:** issues/epics/event-system/index.md
**Depends-On:** issues/epics/event-system/5-action-dispatch.md

## Objective

Implement append-only notification queue that external processes can poll or tail.

## Background

Notifications are high-level events for human consumption:
- Work started on issue
- PR opened
- Needs intervention
- Feedback received

External processes watch this queue to trigger alerts.

## Design

### Notification Schema

`~/.config/jig/state/notifications.jsonl`:
```jsonl
{"ts": 1708358400, "id": "abc123", "type": "work_started", "repo": "jig", "worker": "feature-auth", "issue": "ABC-123"}
{"ts": 1708359000, "id": "def456", "type": "pr_opened", "repo": "jig", "worker": "feature-auth", "pr_url": "https://github.com/..."}
{"ts": 1708360000, "id": "ghi789", "type": "needs_intervention", "repo": "jig", "worker": "feature-auth", "reason": "Stalled - no activity"}
```

### Notification Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotificationEvent {
    WorkStarted {
        repo: String,
        worker: String,
        issue: Option<String>,
    },
    PrOpened {
        repo: String,
        worker: String,
        pr_url: String,
        pr_number: u32,
    },
    FeedbackReceived {
        repo: String,
        worker: String,
        pr_url: String,
        comment_count: u32,
    },
    FeedbackAddressed {
        repo: String,
        worker: String,
        pr_url: String,
    },
    NeedsIntervention {
        repo: String,
        worker: String,
        reason: String,
    },
    WorkCompleted {
        repo: String,
        worker: String,
        pr_url: Option<String>,
    },
}

impl NotificationEvent {
    pub fn needs_intervention(worker: &str, reason: &str) -> Self {
        let (repo, branch) = parse_worker_id(worker);
        Self::NeedsIntervention {
            repo,
            worker: branch,
            reason: reason.to_string(),
        }
    }
}
```

### Notification Queue

```rust
pub struct NotificationQueue {
    path: PathBuf,
}

impl NotificationQueue {
    pub fn global() -> Self {
        Self {
            path: global_state_dir().join("notifications.jsonl"),
        }
    }

    pub fn emit(&self, event: NotificationEvent) -> Result<()> {
        let notification = Notification {
            ts: chrono::Utc::now().timestamp(),
            id: uuid::Uuid::new_v4().to_string(),
            event,
        };

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        writeln!(file, "{}", serde_json::to_string(&notification)?)?;
        Ok(())
    }

    pub fn read_since(&self, since_ts: i64) -> Result<Vec<Notification>> {
        if !self.path.exists() {
            return Ok(vec![]);
        }

        let content = std::fs::read_to_string(&self.path)?;
        content
            .lines()
            .filter_map(|line| serde_json::from_str::<Notification>(line).ok())
            .filter(|n| n.ts > since_ts)
            .collect::<Vec<_>>()
            .pipe(Ok)
    }

    pub fn tail(&self, n: usize) -> Result<Vec<Notification>> {
        if !self.path.exists() {
            return Ok(vec![]);
        }

        let content = std::fs::read_to_string(&self.path)?;
        let notifications: Vec<Notification> = content
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();

        Ok(notifications.into_iter().rev().take(n).rev().collect())
    }
}
```

### CLI Commands

```bash
# Show recent notifications
jig notify list

# Show notifications since timestamp
jig notify list --since 1708358400

# Tail notifications (for scripting)
jig notify tail -n 10

# Watch notifications (continuous)
jig notify watch
```

## Implementation

**Files:**
- `crates/jig-core/src/notify/mod.rs` — module
- `crates/jig-core/src/notify/events.rs` — NotificationEvent enum
- `crates/jig-core/src/notify/queue.rs` — NotificationQueue
- `crates/jig-cli/src/commands/notify.rs` — CLI commands

## Acceptance Criteria

- [ ] NotificationEvent enum with all types
- [ ] `NotificationQueue.emit()` appends to JSONL
- [ ] `NotificationQueue.read_since()` filters by timestamp
- [ ] `NotificationQueue.tail()` returns last N
- [ ] Each notification has unique ID
- [ ] `jig notify list` shows recent notifications
- [ ] `jig notify tail` outputs raw JSONL (for piping)
- [ ] Queue file created on first write

## Testing

```rust
#[test]
fn test_emit_notification() {
    let temp = tempfile::tempdir().unwrap();
    let queue = NotificationQueue { path: temp.path().join("notifications.jsonl") };

    queue.emit(NotificationEvent::WorkStarted {
        repo: "jig".to_string(),
        worker: "feature".to_string(),
        issue: Some("ABC-123".to_string()),
    }).unwrap();

    let notifications = queue.tail(10).unwrap();
    assert_eq!(notifications.len(), 1);
    assert!(matches!(notifications[0].event, NotificationEvent::WorkStarted { .. }));
}

#[test]
fn test_read_since() {
    let temp = tempfile::tempdir().unwrap();
    let queue = NotificationQueue { path: temp.path().join("notifications.jsonl") };

    // Emit two notifications
    queue.emit(NotificationEvent::WorkStarted { .. }).unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1));
    let mid_ts = chrono::Utc::now().timestamp();
    queue.emit(NotificationEvent::PrOpened { .. }).unwrap();

    let recent = queue.read_since(mid_ts).unwrap();
    assert_eq!(recent.len(), 1);
    assert!(matches!(recent[0].event, NotificationEvent::PrOpened { .. }));
}
```

## Next Steps

After this ticket:
- Move to ticket 7 (notification hooks)
- Hooks trigger alerts when notifications are emitted
