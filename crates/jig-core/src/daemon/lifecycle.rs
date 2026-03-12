//! Daemon lifecycle log — records Started/Stopped events in a JSONL file.
//!
//! Stored at `~/.config/jig/state/daemon.jsonl`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::global::paths::daemon_log_path;

/// A daemon lifecycle event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum DaemonEvent {
    Started { ts: i64, pid: u32 },
    Stopped { ts: i64, pid: u32, reason: String },
}

/// Wrapper around the lifecycle log, providing structured access to daemon events.
#[derive(Debug)]
pub struct DaemonLifecycleLog {
    path: PathBuf,
}

impl Deref for DaemonLifecycleLog {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.path
    }
}

impl DaemonLifecycleLog {
    /// Open the global lifecycle log at the default path.
    pub fn global() -> Result<Self> {
        Ok(Self {
            path: daemon_log_path()?,
        })
    }

    /// Open a lifecycle log at a specific path (for testing).
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    /// Record a daemon start event.
    pub fn record_started(&self) -> Result<()> {
        self.append(&DaemonEvent::Started {
            ts: chrono::Utc::now().timestamp(),
            pid: std::process::id(),
        })
    }

    /// Record a daemon stop event.
    pub fn record_stopped(&self, reason: &str) -> Result<()> {
        self.append(&DaemonEvent::Stopped {
            ts: chrono::Utc::now().timestamp(),
            pid: std::process::id(),
            reason: reason.to_string(),
        })
    }

    /// Append a lifecycle event to the log.
    pub fn append(&self, event: &DaemonEvent) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let line = serde_json::to_string(event)?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    /// Read the last event from the log.
    pub fn last_event(&self) -> Result<Option<DaemonEvent>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&self.path)?;
        let last_line = content.lines().rev().find(|l| !l.trim().is_empty());
        match last_line {
            Some(line) => {
                let event: DaemonEvent = serde_json::from_str(line)?;
                Ok(Some(event))
            }
            None => Ok(None),
        }
    }

    /// Check if the previous daemon run crashed (last event is Started, not Stopped).
    pub fn previous_run_crashed(&self) -> Result<bool> {
        match self.last_event()? {
            Some(DaemonEvent::Started { .. }) => Ok(true),
            _ => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_events() {
        let tmp = tempfile::tempdir().unwrap();
        let log = DaemonLifecycleLog::at(tmp.path().join("daemon.jsonl"));

        log.record_started().unwrap();
        let last = log.last_event().unwrap().unwrap();
        assert!(matches!(last, DaemonEvent::Started { .. }));

        log.record_stopped("signal").unwrap();
        let last = log.last_event().unwrap().unwrap();
        assert!(matches!(last, DaemonEvent::Stopped { .. }));
    }

    #[test]
    fn empty_log_not_crashed() {
        let tmp = tempfile::tempdir().unwrap();
        let log = DaemonLifecycleLog::at(tmp.path().join("daemon.jsonl"));

        assert!(log.last_event().unwrap().is_none());
        assert!(!log.previous_run_crashed().unwrap());
    }
}
