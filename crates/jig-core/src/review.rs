//! Review data model and markdown serialization
//!
//! Reviews are markdown files that live in a worker's worktree at `.jig/reviews/`.
//! A review agent writes structured findings, and the implementation agent responds.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ReviewError {
    #[error("Missing required section: {0}")]
    MissingSection(String),
    #[error("No VERDICT line found in Summary section")]
    MissingVerdict,
    #[error("Invalid status marker '{found}' on line {line}, expected [PASS], [WARN], or [FAIL]")]
    InvalidStatus { found: String, line: usize },
    #[error("Missing 'Reviewed:' header line")]
    MissingHeader,
    #[error("Invalid review response header")]
    InvalidResponseHeader,
    #[error("Invalid response category: {0}")]
    InvalidResponseCategory(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// Review types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    pub reviewed_sha: String,
    pub timestamp: i64,
    pub verdict: ReviewVerdict,
    pub sections: Vec<ReviewSection>,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Approve,
    ChangesRequested,
}

impl fmt::Display for ReviewVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReviewVerdict::Approve => write!(f, "approve"),
            ReviewVerdict::ChangesRequested => write!(f, "changes_requested"),
        }
    }
}

impl ReviewVerdict {
    fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "approve" => Some(Self::Approve),
            "changes_requested" => Some(Self::ChangesRequested),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSection {
    pub category: ReviewCategory,
    pub status: ReviewStatus,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewCategory {
    Correctness,
    Conventions,
    ErrorHandling,
    Security,
    TestCoverage,
    Documentation,
}

impl ReviewCategory {
    const ALL: &'static [ReviewCategory] = &[
        ReviewCategory::Correctness,
        ReviewCategory::Conventions,
        ReviewCategory::ErrorHandling,
        ReviewCategory::Security,
        ReviewCategory::TestCoverage,
        ReviewCategory::Documentation,
    ];

    fn heading(&self) -> &'static str {
        match self {
            Self::Correctness => "Correctness",
            Self::Conventions => "Conventions",
            Self::ErrorHandling => "Error Handling",
            Self::Security => "Security",
            Self::TestCoverage => "Test Coverage",
            Self::Documentation => "Documentation",
        }
    }

    fn from_heading(s: &str) -> Option<Self> {
        match s.trim() {
            "Correctness" => Some(Self::Correctness),
            "Conventions" => Some(Self::Conventions),
            "Error Handling" => Some(Self::ErrorHandling),
            "Security" => Some(Self::Security),
            "Test Coverage" => Some(Self::TestCoverage),
            "Documentation" => Some(Self::Documentation),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Pass,
    Warn,
    Fail,
}

impl ReviewStatus {
    fn tag(&self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
        }
    }

    fn from_tag(s: &str) -> Option<Self> {
        match s {
            "PASS" => Some(Self::Pass),
            "WARN" => Some(Self::Warn),
            "FAIL" => Some(Self::Fail),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub file: Option<String>,
    pub line: Option<u32>,
    pub message: String,
    pub severity: ReviewStatus,
}

// ---------------------------------------------------------------------------
// ReviewResponse types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewResponse {
    pub review_number: u32,
    pub responses: Vec<FindingResponse>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingResponse {
    pub finding: String,
    pub action: ResponseAction,
    pub explanation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseAction {
    Addressed,
    Disputed,
    Deferred,
}

impl ResponseAction {
    fn heading(&self) -> &'static str {
        match self {
            Self::Addressed => "Addressed",
            Self::Disputed => "Disputed",
            Self::Deferred => "Deferred",
        }
    }

    fn from_heading(s: &str) -> Option<Self> {
        match s.trim() {
            "Addressed" => Some(Self::Addressed),
            "Disputed" => Some(Self::Disputed),
            "Deferred" => Some(Self::Deferred),
            _ => None,
        }
    }

    const ALL: &'static [ResponseAction] = &[
        ResponseAction::Addressed,
        ResponseAction::Disputed,
        ResponseAction::Deferred,
    ];
}

// ---------------------------------------------------------------------------
// Review markdown serialization
// ---------------------------------------------------------------------------

impl Review {
    /// Serialize this review to its markdown representation.
    /// `number` is the review sequence number (1, 2, 3, ...).
    pub fn to_markdown(&self, number: u32) -> String {
        let dt = chrono::DateTime::from_timestamp(self.timestamp, 0)
            .map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string())
            .unwrap_or_else(|| self.timestamp.to_string());

        let mut out = String::new();
        out.push_str(&format!("# Review {:03}\n", number));
        out.push_str(&format!("Reviewed: {} | {}\n", self.reviewed_sha, dt));

        for section in &self.sections {
            out.push_str(&format!("\n## {}\n", section.category.heading()));
            for finding in &section.findings {
                let location = match (&finding.file, finding.line) {
                    (Some(f), Some(l)) => format!("`{}:{}` — ", f, l),
                    (Some(f), None) => format!("`{}` — ", f),
                    _ => String::new(),
                };
                out.push_str(&format!(
                    "- [{}] {}{}\n",
                    finding.severity.tag(),
                    location,
                    finding.message
                ));
            }
        }

        out.push_str("\n## Summary\n");
        out.push_str(&format!("VERDICT: {}\n", self.verdict));
        if !self.summary.is_empty() {
            out.push('\n');
            out.push_str(&self.summary);
            out.push('\n');
        }

        out
    }

    /// Parse a review from its markdown representation.
    pub fn from_markdown(md: &str) -> Result<Self, ReviewError> {
        let lines: Vec<&str> = md.lines().collect();

        // Parse header line: "Reviewed: <sha> | <timestamp>"
        let header_line = lines
            .iter()
            .find(|l| l.starts_with("Reviewed:"))
            .ok_or(ReviewError::MissingHeader)?;

        let after_reviewed = header_line.strip_prefix("Reviewed:").unwrap().trim();
        let (reviewed_sha, timestamp) = {
            let parts: Vec<&str> = after_reviewed.splitn(2, '|').collect();
            if parts.len() != 2 {
                return Err(ReviewError::MissingHeader);
            }
            let sha = parts[0].trim().to_string();
            let ts_str = parts[1].trim();
            let ts = chrono::DateTime::parse_from_rfc3339(ts_str)
                .map(|d| d.timestamp())
                .or_else(|_| ts_str.parse::<i64>())
                .map_err(|_| ReviewError::MissingHeader)?;
            (sha, ts)
        };

        // Split into sections by ## headings
        let mut sections: Vec<ReviewSection> = Vec::new();
        let mut summary_lines: Vec<&str> = Vec::new();
        let mut current_heading: Option<&str> = None;
        let mut current_findings: Vec<Finding> = Vec::new();
        let mut in_summary = false;

        for (i, line) in lines.iter().enumerate() {
            if let Some(heading) = line.strip_prefix("## ") {
                // Flush previous section
                if let Some(prev) = current_heading {
                    if prev == "Summary" {
                        in_summary = false;
                    } else if let Some(cat) = ReviewCategory::from_heading(prev) {
                        let status = current_findings
                            .iter()
                            .map(|f| f.severity)
                            .max_by_key(|s| match s {
                                ReviewStatus::Pass => 0,
                                ReviewStatus::Warn => 1,
                                ReviewStatus::Fail => 2,
                            })
                            .unwrap_or(ReviewStatus::Pass);
                        sections.push(ReviewSection {
                            category: cat,
                            status,
                            findings: std::mem::take(&mut current_findings),
                        });
                    }
                }
                current_heading = Some(heading.trim());
                if heading.trim() == "Summary" {
                    in_summary = true;
                }
                continue;
            }

            if in_summary {
                summary_lines.push(line);
                continue;
            }

            if current_heading.is_some() && line.starts_with("- [") {
                let finding = parse_finding(line, i + 1)?;
                current_findings.push(finding);
            }
        }

        // Flush last section
        if let Some(prev) = current_heading {
            if prev != "Summary" {
                if let Some(cat) = ReviewCategory::from_heading(prev) {
                    let status = current_findings
                        .iter()
                        .map(|f| f.severity)
                        .max_by_key(|s| match s {
                            ReviewStatus::Pass => 0,
                            ReviewStatus::Warn => 1,
                            ReviewStatus::Fail => 2,
                        })
                        .unwrap_or(ReviewStatus::Pass);
                    sections.push(ReviewSection {
                        category: cat,
                        status,
                        findings: std::mem::take(&mut current_findings),
                    });
                }
            }
        }

        // Validate all required sections present
        for cat in ReviewCategory::ALL {
            if !sections.iter().any(|s| s.category == *cat) {
                return Err(ReviewError::MissingSection(cat.heading().to_string()));
            }
        }

        // Parse verdict from summary
        let verdict_line = summary_lines
            .iter()
            .find(|l| l.starts_with("VERDICT:"))
            .ok_or(ReviewError::MissingVerdict)?;
        let verdict_str = verdict_line.strip_prefix("VERDICT:").unwrap().trim();
        let verdict = ReviewVerdict::from_str(verdict_str).ok_or(ReviewError::MissingVerdict)?;

        // Build summary text (everything after VERDICT line, trimmed)
        let summary = {
            let after_verdict: Vec<&str> = summary_lines
                .iter()
                .skip_while(|l| !l.starts_with("VERDICT:"))
                .skip(1)
                .copied()
                .collect();
            after_verdict.join("\n").trim().to_string()
        };

        Ok(Review {
            reviewed_sha,
            timestamp,
            verdict,
            sections,
            summary,
        })
    }
}

fn parse_finding(line: &str, line_number: usize) -> Result<Finding, ReviewError> {
    // Format: "- [STATUS] optional_location — message" or "- [STATUS] message"
    let after_dash = line.strip_prefix("- [").unwrap();
    let close_bracket = after_dash
        .find(']')
        .ok_or_else(|| ReviewError::InvalidStatus {
            found: line.to_string(),
            line: line_number,
        })?;
    let tag = &after_dash[..close_bracket];
    let severity = ReviewStatus::from_tag(tag).ok_or_else(|| ReviewError::InvalidStatus {
        found: format!("[{}]", tag),
        line: line_number,
    })?;

    let rest = after_dash[close_bracket + 1..].trim();

    // Try to parse file:line — message pattern
    let (file, line_num, message) = if let Some(after_tick) = rest.strip_prefix('`') {
        // Find closing backtick
        if let Some(end_tick) = after_tick.find('`') {
            let location = &after_tick[..end_tick];
            let after_location = after_tick[end_tick + 1..].trim();
            let message = after_location
                .strip_prefix("—")
                .unwrap_or(after_location)
                .trim();

            // Split location into file and optional line
            if let Some((f, l)) = location.rsplit_once(':') {
                if let Ok(ln) = l.parse::<u32>() {
                    (Some(f.to_string()), Some(ln), message.to_string())
                } else {
                    (Some(location.to_string()), None, message.to_string())
                }
            } else {
                (Some(location.to_string()), None, message.to_string())
            }
        } else {
            (None, None, rest.to_string())
        }
    } else {
        (None, None, rest.to_string())
    };

    Ok(Finding {
        file,
        line: line_num,
        message,
        severity,
    })
}

// ---------------------------------------------------------------------------
// ReviewResponse markdown serialization
// ---------------------------------------------------------------------------

impl ReviewResponse {
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# Response to Review {:03}\n", self.review_number));

        for action in ResponseAction::ALL {
            out.push_str(&format!("\n## {}\n", action.heading()));
            let entries: Vec<&FindingResponse> = self
                .responses
                .iter()
                .filter(|r| r.action == *action)
                .collect();
            if entries.is_empty() {
                out.push_str("(none)\n");
            } else {
                for entry in entries {
                    out.push_str(&format!("- {}: {}\n", entry.finding, entry.explanation));
                }
            }
        }

        if let Some(notes) = &self.notes {
            out.push_str("\n## Notes\n");
            out.push_str(notes);
            out.push('\n');
        }

        out
    }

    pub fn from_markdown(md: &str) -> Result<Self, ReviewError> {
        let lines: Vec<&str> = md.lines().collect();

        // Parse header: "# Response to Review NNN"
        let header = lines.first().ok_or(ReviewError::InvalidResponseHeader)?;
        let review_number = header
            .strip_prefix("# Response to Review ")
            .and_then(|s| s.trim().parse::<u32>().ok())
            .ok_or(ReviewError::InvalidResponseHeader)?;

        let mut responses: Vec<FindingResponse> = Vec::new();
        let mut notes_lines: Vec<&str> = Vec::new();
        let mut current_action: Option<ResponseAction> = None;
        let mut in_notes = false;

        for line in lines.iter().skip(1) {
            if let Some(heading) = line.strip_prefix("## ") {
                let heading = heading.trim();
                if heading == "Notes" {
                    in_notes = true;
                    current_action = None;
                    continue;
                }
                in_notes = false;
                current_action =
                    Some(ResponseAction::from_heading(heading).ok_or_else(|| {
                        ReviewError::InvalidResponseCategory(heading.to_string())
                    })?);
                continue;
            }

            if in_notes {
                notes_lines.push(line);
                continue;
            }

            if let Some(action) = current_action {
                if let Some(entry) = line.strip_prefix("- ") {
                    if entry.trim() == "(none)" {
                        continue;
                    }
                    // Parse "finding: explanation"
                    if let Some((finding, explanation)) = entry.split_once(": ") {
                        responses.push(FindingResponse {
                            finding: finding.to_string(),
                            action,
                            explanation: explanation.to_string(),
                        });
                    }
                }
            }
        }

        let notes = if notes_lines.is_empty() {
            None
        } else {
            let text = notes_lines.join("\n").trim().to_string();
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        };

        Ok(ReviewResponse {
            review_number,
            responses,
            notes,
        })
    }
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// `.jig/reviews/` directory in a worktree
pub fn reviews_dir(worktree: &Path) -> PathBuf {
    worktree.join(".jig/reviews")
}

/// Next review file path: count existing NNN.md files + 1, zero-padded to 3 digits
pub fn next_review_path(worktree: &Path) -> PathBuf {
    let n = review_count(worktree) + 1;
    reviews_dir(worktree).join(format!("{:03}.md", n))
}

/// Response path for a given review number
pub fn review_response_path(worktree: &Path, review_number: u32) -> PathBuf {
    reviews_dir(worktree).join(format!("{:03}-response.md", review_number))
}

/// Count of review files (NNN.md, not NNN-response.md)
pub fn review_count(worktree: &Path) -> u32 {
    let dir = reviews_dir(worktree);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return 0;
    };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.ends_with(".md") && !name.contains("-response") && name.len() == 6
            // NNN.md = 6 chars
        })
        .count() as u32
}

/// All review + response files in order
pub fn review_history(worktree: &Path) -> Vec<PathBuf> {
    let dir = reviews_dir(worktree);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "md").unwrap_or(false))
        .collect();
    files.sort();
    files
}

/// Parse the latest review file and return its verdict
pub fn latest_verdict(worktree: &Path) -> Option<ReviewVerdict> {
    let dir = reviews_dir(worktree);
    let entries = std::fs::read_dir(&dir).ok()?;
    let mut review_files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            name.ends_with(".md") && !name.contains("-response") && name.len() == 6
        })
        .collect();
    review_files.sort();
    let latest = review_files.last()?;
    let content = std::fs::read_to_string(latest).ok()?;
    let review = Review::from_markdown(&content).ok()?;
    Some(review.verdict)
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReviewConfig {
    #[serde(default)]
    pub enabled: bool,
    pub model: Option<String>,
    #[serde(default = "default_max_rounds")]
    pub max_rounds: u32,
}

fn default_max_rounds() -> u32 {
    5
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: None,
            max_rounds: default_max_rounds(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_review() -> Review {
        Review {
            reviewed_sha: "abc123".to_string(),
            timestamp: 1743681600, // 2025-04-03T12:00:00Z
            verdict: ReviewVerdict::ChangesRequested,
            sections: vec![
                ReviewSection {
                    category: ReviewCategory::Correctness,
                    status: ReviewStatus::Pass,
                    findings: vec![Finding {
                        file: None,
                        line: None,
                        message: "No issues found".to_string(),
                        severity: ReviewStatus::Pass,
                    }],
                },
                ReviewSection {
                    category: ReviewCategory::Conventions,
                    status: ReviewStatus::Warn,
                    findings: vec![Finding {
                        file: Some("crates/jig-core/src/foo.rs".to_string()),
                        line: Some(42),
                        message: "variable name doesn't follow snake_case".to_string(),
                        severity: ReviewStatus::Warn,
                    }],
                },
                ReviewSection {
                    category: ReviewCategory::ErrorHandling,
                    status: ReviewStatus::Pass,
                    findings: vec![Finding {
                        file: None,
                        line: None,
                        message: "Appropriate for context".to_string(),
                        severity: ReviewStatus::Pass,
                    }],
                },
                ReviewSection {
                    category: ReviewCategory::Security,
                    status: ReviewStatus::Pass,
                    findings: vec![Finding {
                        file: None,
                        line: None,
                        message: "No issues found".to_string(),
                        severity: ReviewStatus::Pass,
                    }],
                },
                ReviewSection {
                    category: ReviewCategory::TestCoverage,
                    status: ReviewStatus::Fail,
                    findings: vec![Finding {
                        file: Some("crates/jig-core/src/foo.rs".to_string()),
                        line: None,
                        message: "new public function `bar()` has no test".to_string(),
                        severity: ReviewStatus::Fail,
                    }],
                },
                ReviewSection {
                    category: ReviewCategory::Documentation,
                    status: ReviewStatus::Pass,
                    findings: vec![Finding {
                        file: None,
                        line: None,
                        message: "No updates needed".to_string(),
                        severity: ReviewStatus::Pass,
                    }],
                },
            ],
            summary: "Missing test coverage for `bar()`. One required change, one suggestion."
                .to_string(),
        }
    }

    #[test]
    fn review_roundtrip() {
        let review = sample_review();
        let md = review.to_markdown(1);
        let parsed = Review::from_markdown(&md).unwrap();

        assert_eq!(parsed.reviewed_sha, review.reviewed_sha);
        assert_eq!(parsed.timestamp, review.timestamp);
        assert_eq!(parsed.verdict, review.verdict);
        assert_eq!(parsed.sections.len(), review.sections.len());
        assert_eq!(parsed.summary, review.summary);

        for (orig, parsed) in review.sections.iter().zip(parsed.sections.iter()) {
            assert_eq!(orig.category, parsed.category);
            assert_eq!(orig.status, parsed.status);
            assert_eq!(orig.findings.len(), parsed.findings.len());
            for (of, pf) in orig.findings.iter().zip(parsed.findings.iter()) {
                assert_eq!(of.file, pf.file);
                assert_eq!(of.line, pf.line);
                assert_eq!(of.severity, pf.severity);
                assert_eq!(of.message, pf.message);
            }
        }
    }

    #[test]
    fn response_roundtrip() {
        let response = ReviewResponse {
            review_number: 1,
            responses: vec![
                FindingResponse {
                    finding: "`crates/jig-core/src/foo.rs` — missing test for `bar()`".to_string(),
                    action: ResponseAction::Addressed,
                    explanation: "Added test in commit def456".to_string(),
                },
                FindingResponse {
                    finding: "`crates/jig-core/src/foo.rs:42` — snake_case".to_string(),
                    action: ResponseAction::Disputed,
                    explanation:
                        "The variable follows the existing pattern in this module, see line 10-15"
                            .to_string(),
                },
            ],
            notes: Some(
                "Also fixed an unrelated typo noticed while addressing findings.".to_string(),
            ),
        };

        let md = response.to_markdown();
        let parsed = ReviewResponse::from_markdown(&md).unwrap();

        assert_eq!(parsed.review_number, 1);
        assert_eq!(parsed.responses.len(), 2);
        assert_eq!(parsed.responses[0].action, ResponseAction::Addressed);
        assert_eq!(parsed.responses[1].action, ResponseAction::Disputed);
        assert!(parsed.notes.is_some());
        assert!(parsed.notes.unwrap().contains("unrelated typo"));
    }

    #[test]
    fn from_markdown_missing_section() {
        let md = "\
# Review 001
Reviewed: abc123 | 2025-04-03T12:00:00Z

## Correctness
- [PASS] No issues found

## Summary
VERDICT: approve

All good.
";
        let err = Review::from_markdown(md).unwrap_err();
        assert!(err.to_string().contains("Missing required section"));
    }

    #[test]
    fn from_markdown_missing_verdict() {
        let md = "\
# Review 001
Reviewed: abc123 | 2025-04-03T12:00:00Z

## Correctness
- [PASS] No issues found

## Conventions
- [PASS] No issues found

## Error Handling
- [PASS] No issues found

## Security
- [PASS] No issues found

## Test Coverage
- [PASS] No issues found

## Documentation
- [PASS] No issues found

## Summary
All good but missing verdict line.
";
        let err = Review::from_markdown(md).unwrap_err();
        assert!(err.to_string().contains("VERDICT"));
    }

    #[test]
    fn from_markdown_invalid_status() {
        let md = "\
# Review 001
Reviewed: abc123 | 2025-04-03T12:00:00Z

## Correctness
- [FOO] Something weird

## Conventions
- [PASS] Ok

## Error Handling
- [PASS] Ok

## Security
- [PASS] Ok

## Test Coverage
- [PASS] Ok

## Documentation
- [PASS] Ok

## Summary
VERDICT: approve

Done.
";
        let err = Review::from_markdown(md).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("[FOO]"));
        assert!(msg.contains("expected [PASS], [WARN], or [FAIL]"));
    }

    #[test]
    fn from_markdown_missing_header() {
        let md = "\
# Review 001

## Correctness
- [PASS] No issues found
";
        let err = Review::from_markdown(md).unwrap_err();
        assert!(err.to_string().contains("Reviewed:"));
    }

    #[test]
    fn next_review_path_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let dir = reviews_dir(tmp.path());
        std::fs::create_dir_all(&dir).unwrap();

        let path = next_review_path(tmp.path());
        assert_eq!(path.file_name().unwrap(), "001.md");
    }

    #[test]
    fn next_review_path_after_one() {
        let tmp = TempDir::new().unwrap();
        let dir = reviews_dir(tmp.path());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("001.md"), "test").unwrap();

        let path = next_review_path(tmp.path());
        assert_eq!(path.file_name().unwrap(), "002.md");
    }

    #[test]
    fn review_count_excludes_responses() {
        let tmp = TempDir::new().unwrap();
        let dir = reviews_dir(tmp.path());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("001.md"), "test").unwrap();
        std::fs::write(dir.join("001-response.md"), "test").unwrap();
        std::fs::write(dir.join("002.md"), "test").unwrap();

        assert_eq!(review_count(tmp.path()), 2);
    }

    #[test]
    fn latest_verdict_empty_dir() {
        let tmp = TempDir::new().unwrap();
        assert!(latest_verdict(tmp.path()).is_none());
    }

    #[test]
    fn latest_verdict_returns_correct() {
        let tmp = TempDir::new().unwrap();
        let dir = reviews_dir(tmp.path());
        std::fs::create_dir_all(&dir).unwrap();

        let review = sample_review();
        let md = review.to_markdown(1);
        std::fs::write(dir.join("001.md"), &md).unwrap();

        assert_eq!(
            latest_verdict(tmp.path()),
            Some(ReviewVerdict::ChangesRequested)
        );
    }

    #[test]
    fn review_config_defaults() {
        let toml_str = "";
        let config: ReviewConfig = toml::from_str(toml_str).unwrap();
        assert!(!config.enabled);
        assert_eq!(config.max_rounds, 5);
        assert!(config.model.is_none());
    }

    #[test]
    fn review_config_from_toml() {
        let toml_str = r#"
enabled = true
max_rounds = 3
"#;
        let config: ReviewConfig = toml::from_str(toml_str).unwrap();
        assert!(config.enabled);
        assert_eq!(config.max_rounds, 3);
    }
}
