//! Notification hook execution.

use std::io::Write;
use std::process::{Command, Stdio};

use crate::error::Result;
use crate::global::NotifyConfig;

use super::{NotificationEvent, NotificationQueue};

/// Notifier wraps the queue and executes hooks on emit.
pub struct Notifier {
    config: NotifyConfig,
    queue: NotificationQueue,
}

impl Notifier {
    pub fn new(config: NotifyConfig, queue: NotificationQueue) -> Self {
        Self { config, queue }
    }

    /// Access the underlying config.
    pub fn config(&self) -> &NotifyConfig {
        &self.config
    }

    /// Access the underlying queue.
    pub fn queue(&self) -> &NotificationQueue {
        &self.queue
    }

    /// Emit a notification: write to queue, then trigger hooks.
    /// Hook failures are logged but swallowed (best-effort, for daemon use).
    pub fn emit(&self, event: NotificationEvent) -> Result<()> {
        // Always write to queue
        self.queue.emit(event.clone())?;

        // Check if this event type should trigger hooks
        if !self.should_trigger(&event) {
            return Ok(());
        }

        let json = serde_json::to_string(&event)?;

        // Execute script hook (best-effort, don't fail the operation)
        if let Some(exec) = &self.config.exec {
            if let Err(e) = self.exec_hook(exec, &json) {
                tracing::warn!("notification hook failed: {}", e);
            } else {
                tracing::debug!(event_type = event.type_name(), "notification hook executed");
            }
        }

        Ok(())
    }

    /// Emit a notification strictly: write to queue, then trigger hooks.
    /// Hook failures are returned as errors (for CLI use).
    pub fn emit_strict(&self, event: NotificationEvent) -> Result<()> {
        // Always write to queue
        self.queue.emit(event.clone())?;

        // Check if this event type should trigger hooks
        if !self.should_trigger(&event) {
            return Ok(());
        }

        let json = serde_json::to_string(&event)?;

        // Execute script hook — propagate errors to caller
        if let Some(exec) = &self.config.exec {
            self.exec_hook_strict(exec, &json)?;
        }

        Ok(())
    }

    /// Check if this event type is in the configured filter list.
    /// Empty list means trigger for all events.
    pub fn should_trigger(&self, event: &NotificationEvent) -> bool {
        if self.config.events.is_empty() {
            return true;
        }
        let event_type = event.type_name();
        self.config.events.iter().any(|e| e == event_type)
    }

    fn exec_hook(&self, exec: &str, json: &str) -> Result<()> {
        let expanded = expand_tilde(exec);

        let mut child = Command::new("sh")
            .arg("-c")
            .arg(&expanded)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(json.as_bytes());
        }

        let status = child.wait()?;
        if !status.success() {
            tracing::warn!("notification hook exited with: {}", status);
        }

        Ok(())
    }

    /// Like `exec_hook` but captures stderr and returns errors on non-zero exit.
    fn exec_hook_strict(&self, exec: &str, json: &str) -> Result<()> {
        let expanded = expand_tilde(exec);

        let mut child = Command::new("sh")
            .arg("-c")
            .arg(&expanded)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(json.as_bytes());
        }

        let output = child.wait_with_output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let msg = if stderr.trim().is_empty() {
                format!("notification hook exited with: {}", output.status)
            } else {
                format!(
                    "notification hook exited with {}: {}",
                    output.status,
                    stderr.trim()
                )
            };
            return Err(crate::error::Error::Custom(msg));
        }

        Ok(())
    }
}

/// Expand `~` at the start of a path to the home directory.
fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    path.to_string()
}

impl NotificationEvent {
    /// Return the snake_case type name for filtering.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::WorkStarted { .. } => "work_started",
            Self::PrOpened { .. } => "pr_opened",
            Self::FeedbackReceived { .. } => "feedback_received",
            Self::FeedbackAddressed { .. } => "feedback_addressed",
            Self::NeedsIntervention { .. } => "needs_intervention",
            Self::WorkCompleted { .. } => "work_completed",
            Self::ReviewApproved { .. } => "review_approved",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::global::NotifyConfig;

    fn make_event() -> NotificationEvent {
        NotificationEvent::NeedsIntervention {
            repo: "jig".to_string(),
            worker: "feat".to_string(),
            reason: "stalled".to_string(),
        }
    }

    #[test]
    fn event_type_name() {
        assert_eq!(make_event().type_name(), "needs_intervention");
        assert_eq!(
            NotificationEvent::WorkStarted {
                repo: "r".into(),
                worker: "w".into(),
                issue: None
            }
            .type_name(),
            "work_started"
        );
    }

    #[test]
    fn should_trigger_with_matching_filter() {
        let tmp = tempfile::tempdir().unwrap();
        let queue = NotificationQueue::new(tmp.path().join("n.jsonl"));
        let config = NotifyConfig {
            events: vec!["needs_intervention".to_string()],
            ..Default::default()
        };
        let notifier = Notifier::new(config, queue);

        assert!(notifier.should_trigger(&make_event()));
    }

    #[test]
    fn should_not_trigger_with_non_matching_filter() {
        let tmp = tempfile::tempdir().unwrap();
        let queue = NotificationQueue::new(tmp.path().join("n.jsonl"));
        let config = NotifyConfig {
            events: vec!["pr_opened".to_string()],
            ..Default::default()
        };
        let notifier = Notifier::new(config, queue);

        assert!(!notifier.should_trigger(&make_event()));
    }

    #[test]
    fn empty_filter_triggers_all() {
        let tmp = tempfile::tempdir().unwrap();
        let queue = NotificationQueue::new(tmp.path().join("n.jsonl"));
        let config = NotifyConfig::default();
        let notifier = Notifier::new(config, queue);

        assert!(notifier.should_trigger(&make_event()));
    }

    #[test]
    fn expand_tilde_works() {
        let expanded = expand_tilde("~/foo/bar");
        assert!(!expanded.starts_with('~'));
        assert!(expanded.ends_with("foo/bar"));

        // Non-tilde paths unchanged
        assert_eq!(expand_tilde("/absolute/path"), "/absolute/path");
        assert_eq!(expand_tilde("relative"), "relative");
    }

    #[test]
    fn emit_writes_to_queue() {
        let tmp = tempfile::tempdir().unwrap();
        let queue_path = tmp.path().join("n.jsonl");
        let queue = NotificationQueue::new(queue_path.clone());
        let config = NotifyConfig::default();
        let notifier = Notifier::new(config, queue);

        notifier.emit(make_event()).unwrap();

        let read_queue = NotificationQueue::new(queue_path);
        let notifications = read_queue.tail(10).unwrap();
        assert_eq!(notifications.len(), 1);
    }

    #[test]
    fn emit_strict_returns_err_on_hook_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let queue = NotificationQueue::new(tmp.path().join("n.jsonl"));
        let config = NotifyConfig {
            exec: Some("exit 1".to_string()),
            ..Default::default()
        };
        let notifier = Notifier::new(config, queue);

        let err = notifier.emit_strict(make_event()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("notification hook exited with"), "got: {msg}");
    }

    #[test]
    fn emit_strict_captures_stderr() {
        let tmp = tempfile::tempdir().unwrap();
        let queue = NotificationQueue::new(tmp.path().join("n.jsonl"));
        let config = NotifyConfig {
            exec: Some("echo 'bad config' >&2; exit 1".to_string()),
            ..Default::default()
        };
        let notifier = Notifier::new(config, queue);

        let err = notifier.emit_strict(make_event()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("bad config"), "got: {msg}");
    }

    #[test]
    fn emit_strict_succeeds_on_hook_success() {
        let tmp = tempfile::tempdir().unwrap();
        let queue = NotificationQueue::new(tmp.path().join("n.jsonl"));
        let config = NotifyConfig {
            exec: Some("cat > /dev/null".to_string()),
            ..Default::default()
        };
        let notifier = Notifier::new(config, queue);

        notifier.emit_strict(make_event()).unwrap();
        // Also verify the event was queued
        let read_queue = NotificationQueue::new(tmp.path().join("n.jsonl"));
        assert_eq!(read_queue.tail(10).unwrap().len(), 1);
    }

    #[test]
    fn exec_hook_receives_json() {
        let tmp = tempfile::tempdir().unwrap();
        let output_path = tmp.path().join("output.json");
        let script_path = tmp.path().join("hook.sh");

        std::fs::write(
            &script_path,
            format!("#!/bin/bash\ncat > '{}'", output_path.display()),
        )
        .unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(perms.mode() | 0o111);
            std::fs::set_permissions(&script_path, perms).unwrap();
        }

        let queue = NotificationQueue::new(tmp.path().join("n.jsonl"));
        let config = NotifyConfig {
            exec: Some(script_path.to_string_lossy().to_string()),
            ..Default::default()
        };
        let notifier = Notifier::new(config, queue);

        notifier.emit(make_event()).unwrap();

        let content = std::fs::read_to_string(&output_path).unwrap();
        assert!(content.contains("needs_intervention"));
        assert!(content.contains("jig"));
    }
}
