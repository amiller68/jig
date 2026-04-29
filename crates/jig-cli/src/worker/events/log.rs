//! JSONL event log reader/writer, generic over event type.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::Serialize;

use jig_core::error::Result;

/// Append-only JSONL event log, generic over event type.
pub struct EventLog<E = super::Event> {
    path: PathBuf,
    _phantom: std::marker::PhantomData<E>,
}

impl<E> EventLog<E> {
    /// Create an event log at a specific path.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Check if the log file exists.
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Truncate the log file (clear all events).
    pub fn reset(&self) -> Result<()> {
        if self.path.exists() {
            fs::write(&self.path, "")?;
        }
        Ok(())
    }

    /// Remove the log file and its parent directory (if empty).
    pub fn remove(&self) -> Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        if let Some(parent) = self.path.parent() {
            let _ = fs::remove_dir(parent);
        }
        Ok(())
    }
}

impl<E: Serialize> EventLog<E> {
    /// Append a single event to the log file.
    pub fn append(&self, event: &E) -> Result<()> {
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
}

impl<E: DeserializeOwned> EventLog<E> {
    /// Read all events from the log. Returns empty vec if file doesn't exist.
    pub fn read_all(&self) -> Result<Vec<E>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = fs::File::open(&self.path)?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if !line.is_empty() {
                events.push(serde_json::from_str(&line)?);
            }
        }
        Ok(events)
    }

    /// Return the last event without loading all events into memory.
    pub fn last_event(&self) -> Result<Option<E>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let file = fs::File::open(&self.path)?;
        let reader = BufReader::new(file);
        let mut last_line = None;
        for line in reader.lines() {
            let line = line?;
            if !line.is_empty() {
                last_line = Some(line);
            }
        }
        match last_line {
            Some(line) => Ok(Some(serde_json::from_str(&line)?)),
            None => Ok(None),
        }
    }
}

impl EventLog<super::Event> {
    /// Create an event log for a worker, using the global config directory.
    ///
    /// Path: `~/.config/jig/<repo>/<branch>/events.jsonl`
    /// Branch slashes are preserved as real directory nesting.
    pub fn for_worker(repo: &str, branch: &str) -> Result<Self> {
        let dir = crate::config::worker_events_dir(repo, branch)?;
        Ok(Self::new(dir.join("events.jsonl")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::events::{Event, EventKind, EventType};
    use jig_core::issues::issue::IssueRef;

    #[test]
    fn append_and_read_all_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let log = EventLog::new(tmp.path().join("events.jsonl"));

        let event = Event::now(EventKind::Spawn {
            branch: "main".into(),
            repo: "r".into(),
            issue: IssueRef::new("JIG-1"),
        });
        log.append(&event).unwrap();

        let events = log.read_all().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type(), EventType::Spawn);
    }

    #[test]
    fn multiple_appends_accumulate() {
        let tmp = tempfile::tempdir().unwrap();
        let log = EventLog::new(tmp.path().join("events.jsonl"));

        log.append(&Event::now(EventKind::Spawn {
            branch: "main".into(),
            repo: "r".into(),
            issue: IssueRef::new("JIG-1"),
        }))
        .unwrap();
        log.append(&Event::now(EventKind::Commit {
            sha: "abc".into(),
            repo: "r".into(),
        }))
        .unwrap();
        log.append(&Event::now(EventKind::Stop)).unwrap();

        let events = log.read_all().unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].event_type(), EventType::Spawn);
        assert_eq!(events[1].event_type(), EventType::Commit);
        assert_eq!(events[2].event_type(), EventType::Stop);
    }

    #[test]
    fn missing_file_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let log: EventLog = EventLog::new(tmp.path().join("nonexistent.jsonl"));

        assert!(!log.exists());
        assert!(log.read_all().unwrap().is_empty());
        assert!(log.last_event().unwrap().is_none());
    }

    #[test]
    fn last_event_returns_final() {
        let tmp = tempfile::tempdir().unwrap();
        let log = EventLog::new(tmp.path().join("events.jsonl"));

        log.append(&Event::now(EventKind::Spawn {
            branch: "main".into(),
            repo: "r".into(),
            issue: IssueRef::new("JIG-1"),
        }))
        .unwrap();
        log.append(&Event::now(EventKind::Commit {
            sha: "abc".into(),
            repo: "r".into(),
        }))
        .unwrap();
        log.append(&Event::now(EventKind::Stop)).unwrap();

        let last = log.last_event().unwrap().unwrap();
        assert_eq!(last.event_type(), EventType::Stop);
    }

    #[test]
    fn for_worker_path_format() {
        let log = EventLog::for_worker("myrepo", "feat/branch").unwrap();
        let path_str = log.path.to_string_lossy();
        assert!(path_str.ends_with("myrepo/feat/branch/events.jsonl"));
    }

    #[test]
    fn reset_clears_events() {
        let tmp = tempfile::tempdir().unwrap();
        let log = EventLog::new(tmp.path().join("events.jsonl"));

        log.append(&Event::now(EventKind::Spawn {
            branch: "main".into(),
            repo: "r".into(),
            issue: IssueRef::new("JIG-1"),
        }))
        .unwrap();
        log.append(&Event::now(EventKind::Commit {
            sha: "abc".into(),
            repo: "r".into(),
        }))
        .unwrap();
        assert_eq!(log.read_all().unwrap().len(), 2);

        log.reset().unwrap();
        assert!(log.exists());
        assert!(log.read_all().unwrap().is_empty());
    }

    #[test]
    fn remove_deletes_file_and_empty_parent() {
        let tmp = tempfile::tempdir().unwrap();
        let subdir = tmp.path().join("worker-dir");
        std::fs::create_dir_all(&subdir).unwrap();
        let log = EventLog::new(subdir.join("events.jsonl"));

        log.append(&Event::now(EventKind::Spawn {
            branch: "main".into(),
            repo: "r".into(),
            issue: IssueRef::new("JIG-1"),
        }))
        .unwrap();
        assert!(log.exists());

        log.remove().unwrap();
        assert!(!log.exists());
        assert!(!subdir.exists());
    }

    #[test]
    fn generic_event_log_works_with_custom_type() {
        use serde::{Deserialize, Serialize};

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        #[serde(tag = "event")]
        enum TestEvent {
            Started { ts: i64 },
            Stopped { ts: i64 },
        }

        let tmp = tempfile::tempdir().unwrap();
        let log: EventLog<TestEvent> = EventLog::new(tmp.path().join("test.jsonl"));

        log.append(&TestEvent::Started { ts: 1000 }).unwrap();
        log.append(&TestEvent::Stopped { ts: 2000 }).unwrap();

        let events = log.read_all().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], TestEvent::Started { ts: 1000 });
        assert_eq!(events[1], TestEvent::Stopped { ts: 2000 });

        let last = log.last_event().unwrap().unwrap();
        assert_eq!(last, TestEvent::Stopped { ts: 2000 });
    }
}
