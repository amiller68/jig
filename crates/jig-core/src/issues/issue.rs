//! Core issue types.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use crate::git::Branch;
use crate::prompt::Prompt;

use super::providers::IssueProvider;

/// A reference to an issue in an external tracker (e.g. "ENG-123").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IssueRef(String);

impl IssueRef {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Display for IssueRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::ops::Deref for IssueRef {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl From<String> for IssueRef {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for IssueRef {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<IssueRef> for String {
    fn from(r: IssueRef) -> Self {
        r.0
    }
}

impl PartialEq<str> for IssueRef {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for IssueRef {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl AsRef<str> for IssueRef {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Issue status values matching the convention in `issues/README.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(ascii_case_insensitive)]
pub enum IssueStatus {
    Triage,
    Backlog,
    Planned,
    InProgress,
    Complete,
    Blocked,
}

/// Issue priority levels.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Display, EnumString,
)]
#[serde(rename_all = "lowercase")]
#[strum(ascii_case_insensitive)]
pub enum IssuePriority {
    Urgent,
    High,
    Medium,
    Low,
}

/// A parsed issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    id: IssueRef,
    title: String,
    status: IssueStatus,
    priority: IssuePriority,
    depends_on: Vec<IssueRef>,
    body: String,
    children: Vec<IssueRef>,
    labels: Vec<String>,
    branch: Branch,
    parent: Option<IssueRef>,
}

impl Issue {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<IssueRef>,
        title: impl Into<String>,
        status: IssueStatus,
        priority: IssuePriority,
        branch: Branch,
        body: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            status,
            priority,
            depends_on: vec![],
            body: body.into(),
            children: vec![],
            labels: vec![],
            branch,
            parent: None,
        }
    }

    pub fn with_parent(mut self, parent: impl Into<IssueRef>) -> Self {
        self.parent = Some(parent.into());
        self
    }

    pub fn with_children(mut self, children: Vec<IssueRef>) -> Self {
        self.children = children;
        self
    }

    pub fn with_depends_on(mut self, depends_on: Vec<IssueRef>) -> Self {
        self.depends_on = depends_on;
        self
    }

    pub fn with_labels(mut self, labels: Vec<String>) -> Self {
        self.labels = labels;
        self
    }

    // -- Accessors --

    pub fn id(&self) -> &IssueRef {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn status(&self) -> &IssueStatus {
        &self.status
    }

    pub fn priority(&self) -> &IssuePriority {
        &self.priority
    }

    pub fn depends_on(&self) -> &[IssueRef] {
        &self.depends_on
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn children(&self) -> &[IssueRef] {
        &self.children
    }

    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    pub fn branch(&self) -> &Branch {
        &self.branch
    }

    pub fn parent(&self) -> Option<&IssueRef> {
        self.parent.as_ref()
    }

    pub fn into_id(self) -> IssueRef {
        self.id
    }

    // -- Predicates --

    pub fn is_parent(&self) -> bool {
        !self.children.is_empty()
    }

    pub fn is_child(&self) -> bool {
        self.parent.is_some()
    }

    pub fn has_label(&self, label: &str) -> bool {
        self.labels.iter().any(|l| l.eq_ignore_ascii_case(label))
    }

    /// Whether this issue is eligible for auto-spawn given the repo's
    /// `auto_spawn_labels` config.
    pub fn auto(&self, spawn_labels: &[String]) -> bool {
        if spawn_labels.is_empty() {
            return true;
        }
        spawn_labels.iter().all(|required| self.has_label(required))
    }

    /// Build a [`Prompt`] from this issue, resolving the parent if present.
    ///
    /// The returned prompt's template is the fully-formed task context string.
    /// Callers render it and inject the result into their own framing template.
    pub fn to_prompt(&self, provider: &IssueProvider) -> Prompt {
        let parent = self.parent().and_then(|r| provider.get(r).ok().flatten());

        let parent_section = match &parent {
            Some(p) => format!(
                "PARENT ISSUE ({}): {}\n{}\n\n---\n\nSUB-TASK:\n",
                p.id(),
                p.title(),
                p.body()
            ),
            None => String::new(),
        };

        let task_context = format!(
            "{}{}\n\n{}\n\nISSUE COMPLETION: This issue is tracked by Linear. \
             Status sync is handled automatically — no manual status update is needed.",
            parent_section,
            self.title(),
            self.body(),
        );

        Prompt::new(&task_context)
    }

    /// Whether this issue matches the given filter.
    pub fn matches(&self, filter: &IssueFilter) -> bool {
        if let Some(ref status) = filter.status {
            if &self.status != status {
                return false;
            }
        }
        if let Some(ref priority) = filter.priority {
            if &self.priority != priority {
                return false;
            }
        }
        for label in &filter.labels {
            if !self.has_label(label) {
                return false;
            }
        }
        true
    }
}

/// Filter criteria for listing issues.
#[derive(Debug, Default)]
pub struct IssueFilter {
    pub status: Option<IssueStatus>,
    pub priority: Option<IssuePriority>,
    pub labels: Vec<String>,
}

impl IssueFilter {
    pub fn apply(&self, issues: Vec<Issue>) -> Vec<Issue> {
        issues.into_iter().filter(|i| i.matches(self)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrip() {
        for status in [
            IssueStatus::Triage,
            IssueStatus::Backlog,
            IssueStatus::Planned,
            IssueStatus::InProgress,
            IssueStatus::Complete,
            IssueStatus::Blocked,
        ] {
            let s = status.to_string();
            assert_eq!(s.parse::<IssueStatus>().ok(), Some(status));
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
            priority: IssuePriority::High,
            depends_on: vec![],
            body: String::new(),
            children: vec![],
            labels: vec![],
            branch: Branch::new("test-branch"),
            parent: None,
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
            priority: IssuePriority::Medium,
            depends_on: vec![],
            body: String::new(),
            children: vec![],
            labels: vec!["backend".into(), "Auth".into()],
            branch: Branch::new("test-branch"),
            parent: None,
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
            priority: IssuePriority::Medium,
            depends_on: vec![],
            body: String::new(),
            children: vec![],
            labels: vec!["backend".into(), "sprint-1".into()],
            branch: Branch::new("test-branch"),
            parent: None,
        };

        // Empty spawn_labels → auto = true (all issues eligible)
        assert!(issue.auto(&[]));

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

    #[test]
    fn parse_case_insensitive() {
        assert_eq!(
            "triage".parse::<IssueStatus>().ok(),
            Some(IssueStatus::Triage)
        );
        assert_eq!(
            "Triage".parse::<IssueStatus>().ok(),
            Some(IssueStatus::Triage)
        );
        assert_eq!(
            "TRIAGE".parse::<IssueStatus>().ok(),
            Some(IssueStatus::Triage)
        );
        assert_eq!(
            "backlog".parse::<IssueStatus>().ok(),
            Some(IssueStatus::Backlog)
        );
        assert_eq!(
            "Backlog".parse::<IssueStatus>().ok(),
            Some(IssueStatus::Backlog)
        );
        assert_eq!(
            "BACKLOG".parse::<IssueStatus>().ok(),
            Some(IssueStatus::Backlog)
        );
    }

    #[test]
    fn triage_backlog_excluded_from_planned_filter() {
        let triage_issue = Issue {
            id: "triage-1".into(),
            title: "Triage issue".into(),
            status: IssueStatus::Triage,
            priority: IssuePriority::Medium,
            depends_on: vec![],
            body: String::new(),
            children: vec![],
            labels: vec![],
            branch: Branch::new("test-branch"),
            parent: None,
        };
        let backlog_issue = Issue {
            id: "backlog-1".into(),
            title: "Backlog issue".into(),
            status: IssueStatus::Backlog,
            priority: IssuePriority::Medium,
            depends_on: vec![],
            body: String::new(),
            children: vec![],
            labels: vec![],
            branch: Branch::new("test-branch"),
            parent: None,
        };
        let planned_issue = Issue {
            id: "planned-1".into(),
            title: "Planned issue".into(),
            status: IssueStatus::Planned,
            priority: IssuePriority::Medium,
            depends_on: vec![],
            body: String::new(),
            children: vec![],
            labels: vec![],
            branch: Branch::new("test-branch"),
            parent: None,
        };

        let planned_filter = IssueFilter {
            status: Some(IssueStatus::Planned),
            ..Default::default()
        };

        // Triage and Backlog should NOT match the Planned filter used by auto-spawn
        assert!(!triage_issue.matches(&planned_filter));
        assert!(!backlog_issue.matches(&planned_filter));
        assert!(planned_issue.matches(&planned_filter));
    }
}
