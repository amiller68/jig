//! JSONL event log reader/writer.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use crate::error::Result;
use crate::global::worker_events_dir;

use super::Event;

/// Append-only JSONL event log for a worker.
pub struct EventLog {
    path: PathBuf,
}

impl EventLog {
    /// Create an event log for a worker, using the global state directory.
    ///
    /// Branch slashes are replaced with dashes to match `worker_events_dir()` naming.
    pub fn for_worker(repo: &str, worker: &str) -> Result<Self> {
        let sanitized = worker.replace('/', "-");
        let dir = worker_events_dir(repo, &sanitized)?;
        Ok(Self {
            path: dir.join("events.jsonl"),
        })
    }

    /// Create an event log at a specific path (useful for testing).
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Append a single event to the log file.
    pub fn append(&self, event: &Event) -> Result<()> {
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

    /// Read all events from the log. Returns empty vec if file doesn't exist.
    pub fn read_all(&self) -> Result<Vec<Event>> {
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
    pub fn last_event(&self) -> Result<Option<Event>> {
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

    /// Remove the log file and its parent directory.
    pub fn remove(&self) -> Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        if let Some(parent) = self.path.parent() {
            let _ = fs::remove_dir(parent); // only succeeds if empty
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Event, EventType};

    #[test]
    fn append_and_read_all_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let log = EventLog::new(tmp.path().join("events.jsonl"));

        let event = Event::new(EventType::Spawn).with_field("branch", "main");
        log.append(&event).unwrap();

        let events = log.read_all().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::Spawn);
    }

    #[test]
    fn multiple_appends_accumulate() {
        let tmp = tempfile::tempdir().unwrap();
        let log = EventLog::new(tmp.path().join("events.jsonl"));

        log.append(&Event::new(EventType::Spawn)).unwrap();
        log.append(&Event::new(EventType::Commit)).unwrap();
        log.append(&Event::new(EventType::Stop)).unwrap();

        let events = log.read_all().unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].event_type, EventType::Spawn);
        assert_eq!(events[1].event_type, EventType::Commit);
        assert_eq!(events[2].event_type, EventType::Stop);
    }

    #[test]
    fn missing_file_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let log = EventLog::new(tmp.path().join("nonexistent.jsonl"));

        assert!(!log.exists());
        assert!(log.read_all().unwrap().is_empty());
        assert!(log.last_event().unwrap().is_none());
    }

    #[test]
    fn last_event_returns_final() {
        let tmp = tempfile::tempdir().unwrap();
        let log = EventLog::new(tmp.path().join("events.jsonl"));

        log.append(&Event::new(EventType::Spawn)).unwrap();
        log.append(&Event::new(EventType::Commit).with_field("sha", "abc"))
            .unwrap();
        log.append(&Event::new(EventType::Stop)).unwrap();

        let last = log.last_event().unwrap().unwrap();
        assert_eq!(last.event_type, EventType::Stop);
    }

    #[test]
    fn for_worker_path_format() {
        let log = EventLog::for_worker("myrepo", "feat/branch").unwrap();
        let path_str = log.path.to_string_lossy();
        assert!(path_str.ends_with("myrepo-feat-branch/events.jsonl"));
    }

    #[test]
    fn reset_clears_events() {
        let tmp = tempfile::tempdir().unwrap();
        let log = EventLog::new(tmp.path().join("events.jsonl"));

        log.append(&Event::new(EventType::Spawn)).unwrap();
        log.append(&Event::new(EventType::Commit)).unwrap();
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

        log.append(&Event::new(EventType::Spawn)).unwrap();
        assert!(log.exists());

        log.remove().unwrap();
        assert!(!log.exists());
        assert!(!subdir.exists());
    }
}
