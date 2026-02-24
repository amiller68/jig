# Notification Hooks

**Status:** Complete
**Priority:** Medium
**Category:** Features
**Epic:** issues/epics/event-system/index.md
**Depends-On:** issues/epics/event-system/6-notification-queue.md

## Objective

Execute user-defined hooks when notifications are emitted, enabling direct alerts to Telegram, Discord, etc.

## Background

Two ways to consume notifications:
1. External process polls/tails the queue
2. jig executes hooks on emit (this ticket)

Hooks provide real-time alerts without external polling.

## Design

### Configuration

`~/.config/jig/config.toml`:
```toml
[notify]
# Execute script with notification JSON on stdin
exec = "~/.config/jig/hooks/notify.sh"

# POST notification JSON to webhook URL
webhook = "http://localhost:8080/notify"

# Filter which notification types trigger hooks
events = ["needs_intervention", "pr_opened", "work_started"]

# Timeout for hook execution (seconds)
timeout = 10
```

### Hook Execution

```rust
pub struct Notifier {
    config: NotifyConfig,
    queue: NotificationQueue,
}

impl Notifier {
    pub fn emit(&self, event: NotificationEvent) -> Result<()> {
        // Always write to queue
        self.queue.emit(event.clone())?;

        // Check if this event type should trigger hooks
        if !self.should_trigger(&event) {
            return Ok(());
        }

        let json = serde_json::to_string(&event)?;

        // Execute script hook
        if let Some(exec) = &self.config.exec {
            self.exec_hook(exec, &json)?;
        }

        // POST to webhook
        if let Some(webhook) = &self.config.webhook {
            self.post_webhook(webhook, &json)?;
        }

        Ok(())
    }

    fn should_trigger(&self, event: &NotificationEvent) -> bool {
        let event_type = event.type_name();
        self.config.events.iter().any(|e| e == event_type)
    }

    fn exec_hook(&self, exec: &str, json: &str) -> Result<()> {
        let expanded = shellexpand::tilde(exec);

        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg(expanded.as_ref())
            .stdin(std::process::Stdio::piped())
            .spawn()?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(json.as_bytes())?;
        }

        let timeout = std::time::Duration::from_secs(self.config.timeout.unwrap_or(10));
        match child.wait_timeout(timeout)? {
            Some(status) if !status.success() => {
                eprintln!("Notification hook failed: {}", status);
            }
            None => {
                child.kill()?;
                eprintln!("Notification hook timed out");
            }
            _ => {}
        }

        Ok(())
    }

    fn post_webhook(&self, url: &str, json: &str) -> Result<()> {
        let client = ureq::agent();

        match client
            .post(url)
            .set("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(self.config.timeout.unwrap_or(10)))
            .send_string(json)
        {
            Ok(_) => Ok(()),
            Err(e) => {
                eprintln!("Webhook failed: {}", e);
                Ok(()) // Don't fail the whole operation
            }
        }
    }
}
```

### Example Hook Script

`~/.config/jig/hooks/notify.sh`:
```bash
#!/bin/bash
# Read notification JSON from stdin
EVENT=$(cat)

# Parse fields
TYPE=$(echo "$EVENT" | jq -r '.type')
REPO=$(echo "$EVENT" | jq -r '.repo')
WORKER=$(echo "$EVENT" | jq -r '.worker')

# Format message based on type
case "$TYPE" in
  work_started)
    MSG="🚀 Started: $REPO/$WORKER"
    ;;
  pr_opened)
    PR_URL=$(echo "$EVENT" | jq -r '.pr_url')
    MSG="📬 PR opened: $PR_URL"
    ;;
  needs_intervention)
    REASON=$(echo "$EVENT" | jq -r '.reason')
    MSG="🚨 BLOCKED: $REPO/$WORKER - $REASON"
    ;;
  *)
    MSG="📣 $TYPE: $REPO/$WORKER"
    ;;
esac

# Send to Telegram (example)
# telegram-send "$MSG"

# Or send to Discord webhook
# curl -X POST -H "Content-Type: application/json" \
#   -d "{\"content\": \"$MSG\"}" \
#   "$DISCORD_WEBHOOK_URL"

# Or just log
echo "$MSG" >> ~/.config/jig/notifications.log
```

## Implementation

**Files:**
- `crates/jig-core/src/notify/hook.rs` — hook execution
- `crates/jig-core/src/notify/config.rs` — NotifyConfig
- `crates/jig-core/Cargo.toml` — add `ureq` for webhooks

**Dependencies:**
```toml
ureq = "2.9"
shellexpand = "3.1"
wait-timeout = "0.2"
```

## Acceptance Criteria

- [ ] Hooks execute on notification emit
- [ ] Event type filtering works
- [ ] Exec hook receives JSON on stdin
- [ ] Webhook POSTs JSON body
- [ ] Hooks timeout after configured duration
- [ ] Hook failures don't block notification queue
- [ ] `~` expansion works in exec path
- [ ] Missing hooks/webhooks are skipped gracefully

## Testing

```rust
#[test]
fn test_event_filtering() {
    let config = NotifyConfig {
        events: vec!["needs_intervention".to_string()],
        ..Default::default()
    };
    let notifier = Notifier::new(config);

    assert!(notifier.should_trigger(&NotificationEvent::NeedsIntervention { .. }));
    assert!(!notifier.should_trigger(&NotificationEvent::WorkStarted { .. }));
}

#[test]
fn test_exec_hook() {
    let temp = tempfile::tempdir().unwrap();
    let script_path = temp.path().join("hook.sh");
    std::fs::write(&script_path, "#!/bin/bash\ncat > /tmp/hook-test.json").unwrap();
    make_executable(&script_path).unwrap();

    let config = NotifyConfig {
        exec: Some(script_path.to_string_lossy().to_string()),
        events: vec!["work_started".to_string()],
        ..Default::default()
    };
    let notifier = Notifier::new(config);

    notifier.emit(NotificationEvent::WorkStarted {
        repo: "test".to_string(),
        worker: "feature".to_string(),
        issue: None,
    }).unwrap();

    let content = std::fs::read_to_string("/tmp/hook-test.json").unwrap();
    assert!(content.contains("work_started"));
}
```

## Next Steps

This completes the event-system epic. Integration with:
- GitHub integration epic (CI, reviews as notifications)
- Nudge system (actions trigger notifications)
