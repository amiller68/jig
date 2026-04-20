use serde::{Deserialize, Serialize};

use super::error::Result;

/// Wrapper around a git2 diff.
pub struct Diff<'a>(git2::Diff<'a>);

impl<'a> Diff<'a> {
    pub(crate) fn new(inner: git2::Diff<'a>) -> Self {
        Self(inner)
    }

    pub fn stats(&self) -> Result<Stats> {
        let mut stats = Stats::default();
        let num_deltas = self.0.deltas().len();

        for i in 0..num_deltas {
            if let Some(patch) = git2::Patch::from_diff(&self.0, i)? {
                let (_, insertions, deletions) = patch.line_stats()?;
                let file_path = self
                    .0
                    .get_delta(i)
                    .and_then(|d| d.new_file().path().map(|p| p.to_string_lossy().to_string()))
                    .unwrap_or_default();

                stats.files_changed += 1;
                stats.insertions += insertions;
                stats.deletions += deletions;
                stats.files.push(FileDiff {
                    path: file_path,
                    insertions,
                    deletions,
                });
            }
        }

        Ok(stats)
    }

    pub fn stat_string(&self) -> Result<String> {
        let raw_stats = self.0.stats()?;
        let buf = raw_stats.to_buf(git2::DiffStatsFormat::FULL, 80)?;
        Ok(std::str::from_utf8(&buf).unwrap_or("").to_string())
    }

    pub fn patch(&self) -> Result<String> {
        let mut output = Vec::new();
        self.0
            .print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
                let origin = line.origin();
                match origin {
                    '+' | '-' | ' ' => output.push(origin as u8),
                    _ => {}
                }
                output.extend_from_slice(line.content());
                true
            })?;
        Ok(String::from_utf8_lossy(&output).to_string())
    }
}

/// Statistics about a diff against a base branch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Stats {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
    pub files: Vec<FileDiff>,
}

impl Stats {
    pub fn is_empty(&self) -> bool {
        self.files_changed == 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileDiff {
    pub path: String,
    pub insertions: usize,
    pub deletions: usize,
}
