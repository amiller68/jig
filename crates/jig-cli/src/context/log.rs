//! Tails the latest daemon log file for the ps --watch UI.

use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;

pub struct LogTailer {
    path: Option<PathBuf>,
    offset: u64,
}

impl LogTailer {
    pub fn new() -> Self {
        let path = super::latest_daemon_log().ok().flatten();
        let offset = path
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .unwrap_or(0);
        Self { path, offset }
    }

    pub fn poll(&mut self, max_lines: usize) -> Vec<String> {
        let path = match &self.path {
            Some(p) if p.exists() => p,
            _ => {
                self.path = super::latest_daemon_log().ok().flatten();
                match &self.path {
                    Some(p) => {
                        self.offset = 0;
                        p
                    }
                    None => return vec![],
                }
            }
        };

        let file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return vec![],
        };
        let mut reader = BufReader::new(file);
        if reader.seek(SeekFrom::Start(self.offset)).is_err() {
            return vec![];
        }

        let mut lines = Vec::new();
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            let trimmed = line.trim_end().to_string();
            if !trimmed.is_empty() {
                lines.push(trimmed);
            }
            line.clear();
        }
        self.offset = reader.stream_position().unwrap_or(self.offset);

        if lines.len() > max_lines {
            lines.split_off(lines.len() - max_lines)
        } else {
            lines
        }
    }
}
