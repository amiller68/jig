//! Core issue types.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Issue status values matching the convention in `issues/README.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueStatus {
    Planned,
    InProgress,
    Complete,
    Blocked,
}

impl IssueStatus {
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "planned" => Some(Self::Planned),
            "in progress" | "in_progress" | "in-progress" | "inprogress" => Some(Self::InProgress),
            "complete" | "done" => Some(Self::Complete),
            "blocked" => Some(Self::Blocked),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Planned => "Planned",
            Self::InProgress => "In Progress",
            Self::Complete => "Complete",
            Self::Blocked => "Blocked",
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Planned => "[ ]",
            Self::InProgress => "[~]",
            Self::Complete => "[x]",
            Self::Blocked => "[!]",
        }
    }
}

impl fmt::Display for IssueStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Issue priority levels.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum IssuePriority {
    Urgent,
    High,
    Medium,
    Low,
}

impl IssuePriority {
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "urgent" => Some(Self::Urgent),
            "high" => Some(Self::High),
            "medium" | "med" => Some(Self::Medium),
            "low" => Some(Self::Low),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Urgent => "Urgent",
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
        }
    }
}

impl fmt::Display for IssuePriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A parsed issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    /// Relative path without `.md`, e.g. "features/smart-context-injection".
    pub id: String,
    /// Title from first `# Heading`.
    pub title: String,
    pub status: IssueStatus,
    pub priority: Option<IssuePriority>,
    /// Category inferred from parent directory or `**Category:**` field.
    pub category: Option<String>,
    /// Paths listed in `**Depends-On:**`.
    pub depends_on: Vec<String>,
    /// Full markdown body.
    pub body: String,
    /// Source file path.
    pub source: String,
    /// Child ticket IDs (for epic indices with a `## Tickets` table).
    pub children: Vec<String>,
    /// Labels/tags attached to this issue.
    pub labels: Vec<String>,
    /// Suggested branch name (e.g. from Linear's `branchName` field).
    pub branch_name: Option<String>,
}

/// Filter criteria for listing issues.
#[derive(Debug, Default)]
pub struct IssueFilter {
    pub status: Option<IssueStatus>,
    pub priority: Option<IssuePriority>,
    pub category: Option<String>,
    /// Filter by labels (all must match).
    pub labels: Vec<String>,
}

impl IssueFilter {
    /// Apply this filter to a list of issues, returning only those that match.
    pub fn apply(&self, issues: Vec<Issue>) -> Vec<Issue> {
        issues.into_iter().filter(|i| i.matches(self)).collect()
    }
}

impl Issue {
    /// Whether this issue is eligible for auto-spawn given the repo's
    /// `spawn_labels` config.
    ///
    /// Returns `true` when `spawn_labels` is non-empty and the issue carries
    /// all of the configured labels (case-insensitive). Returns `false` when
    /// `spawn_labels` is empty — auto-spawn is opt-in via labels.
    pub fn auto(&self, spawn_labels: &[String]) -> bool {
        if spawn_labels.is_empty() {
            return false;
        }
        spawn_labels
            .iter()
            .all(|required| self.labels.iter().any(|l| l.eq_ignore_ascii_case(required)))
    }

    /// Whether this issue matches the given filter.
    pub fn matches(&self, filter: &IssueFilter) -> bool {
        if let Some(ref status) = filter.status {
            if &self.status != status {
                return false;
            }
        }
        if let Some(ref priority) = filter.priority {
            if self.priority.as_ref() != Some(priority) {
                return false;
            }
        }
        if let Some(ref category) = filter.category {
            if self.category.as_deref() != Some(category.as_str()) {
                return false;
            }
        }
        for label in &filter.labels {
            if !self.labels.iter().any(|l| l.eq_ignore_ascii_case(label)) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrip() {
        for status in [
            IssueStatus::Planned,
            IssueStatus::InProgress,
            IssueStatus::Complete,
            IssueStatus::Blocked,
        ] {
            assert_eq!(IssueStatus::from_str_loose(status.as_str()), Some(status));
        }
    }

    #[test]
    fn priority_ordering() {
        assert!(IssuePriority::Urgent < IssuePriority::High);
        assert!(IssuePriority::High < IssuePriority::Medium);
        assert!(IssuePriority::Medium < IssuePriority::Low);
    }

    #[test]
    fn filter_matches() {
        let issue = Issue {
            id: "test".into(),
            title: "Test".into(),
            status: IssueStatus::Planned,
            priority: Some(IssuePriority::High),
            category: Some("features".into()),
            depends_on: vec![],
            body: String::new(),
            source: String::new(),
            children: vec![],
            labels: vec![],
            branch_name: None,
        };

        assert!(issue.matches(&IssueFilter::default()));
        assert!(issue.matches(&IssueFilter {
            status: Some(IssueStatus::Planned),
            ..Default::default()
        }));
        assert!(!issue.matches(&IssueFilter {
            status: Some(IssueStatus::Blocked),
            ..Default::default()
        }));
    }

    #[test]
    fn filter_matches_labels() {
        let issue = Issue {
            id: "test".into(),
            title: "Test".into(),
            status: IssueStatus::Planned,
            priority: None,
            category: None,
            depends_on: vec![],
            body: String::new(),
            source: String::new(),
            children: vec![],
            labels: vec!["backend".into(), "Auth".into()],
            branch_name: None,
        };

        // Single label match (case-insensitive)
        assert!(issue.matches(&IssueFilter {
            labels: vec!["Backend".into()],
            ..Default::default()
        }));

        // Multiple labels — all must match
        assert!(issue.matches(&IssueFilter {
            labels: vec!["backend".into(), "auth".into()],
            ..Default::default()
        }));

        // Missing label → no match
        assert!(!issue.matches(&IssueFilter {
            labels: vec!["frontend".into()],
            ..Default::default()
        }));

        // One present, one missing → no match
        assert!(!issue.matches(&IssueFilter {
            labels: vec!["backend".into(), "frontend".into()],
            ..Default::default()
        }));

        // Empty filter labels → matches everything
        assert!(issue.matches(&IssueFilter::default()));
    }

    #[test]
    fn auto_from_spawn_labels() {
        let issue = Issue {
            id: "test".into(),
            title: "Test".into(),
            status: IssueStatus::Planned,
            priority: None,
            category: None,
            depends_on: vec![],
            body: String::new(),
            source: String::new(),
            children: vec![],
            labels: vec!["backend".into(), "sprint-1".into()],
            branch_name: None,
        };

        // Empty spawn_labels → auto = false (opt-in via labels)
        assert!(!issue.auto(&[]));

        // Matching label → auto = true
        assert!(issue.auto(&["backend".into()]));

        // Case-insensitive match
        assert!(issue.auto(&["Backend".into()]));

        // All must match
        assert!(issue.auto(&["backend".into(), "sprint-1".into()]));

        // Missing label → auto = false
        assert!(!issue.auto(&["frontend".into()]));

        // One present, one missing → auto = false
        assert!(!issue.auto(&["backend".into(), "frontend".into()]));
    }
}
