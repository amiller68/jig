//! File-based issue provider.
//!
//! Walks the `issues/` directory, parses `**Key:** Value` frontmatter from
//! markdown files, and returns structured `Issue` values.

use std::path::{Path, PathBuf};

use crate::error::Result;

use super::provider::IssueProvider;
use super::types::{Issue, IssueFilter, IssuePriority, IssueStatus};

/// File-based issue provider that reads from an `issues/` directory.
pub struct FileProvider {
    /// Root directory containing issue files (e.g. `<repo>/issues`).
    issues_dir: PathBuf,
}

impl FileProvider {
    pub fn new(issues_dir: impl Into<PathBuf>) -> Self {
        Self {
            issues_dir: issues_dir.into(),
        }
    }

    /// Scan all markdown files and parse them into issues.
    fn scan_all(&self) -> Result<Vec<Issue>> {
        if !self.issues_dir.is_dir() {
            return Ok(Vec::new());
        }
        let canonical = self.issues_dir.canonicalize()?;
        let pattern = canonical.join("**/*.md");
        let pattern_str = pattern.to_string_lossy();

        let mut issues = Vec::new();

        for entry in glob::glob(&pattern_str)
            .map_err(|e| crate::error::Error::Custom(format!("invalid glob pattern: {}", e)))?
        {
            let path = match entry {
                Ok(p) => p,
                Err(_) => continue,
            };

            let rel = path.strip_prefix(&canonical).unwrap_or(&path);
            if should_skip(rel) {
                continue;
            }

            match parse_issue_file(&path, &canonical) {
                Ok(issue) => issues.push(issue),
                Err(e) => {
                    tracing::debug!("skipping {}: {}", path.display(), e);
                }
            }
        }

        sort_issues(&mut issues);
        Ok(issues)
    }
}

impl IssueProvider for FileProvider {
    fn name(&self) -> &str {
        "file"
    }

    fn list(&self, filter: &IssueFilter) -> Result<Vec<Issue>> {
        let all = self.scan_all()?;
        Ok(all.into_iter().filter(|i| i.matches(filter)).collect())
    }

    fn get(&self, id: &str) -> Result<Option<Issue>> {
        if !self.issues_dir.is_dir() {
            return Ok(None);
        }
        let canonical = self.issues_dir.canonicalize()?;

        // Try with and without .md extension
        let candidates = [canonical.join(format!("{}.md", id)), canonical.join(id)];

        for path in &candidates {
            if path.exists() && path.extension().map(|e| e == "md").unwrap_or(false) {
                return parse_issue_file(path, &canonical).map(Some);
            }
        }

        Ok(None)
    }
}

/// Check if a file should be skipped during scanning.
/// `rel` is the path relative to the issues directory.
fn should_skip(rel: &Path) -> bool {
    let rel_str = rel.to_string_lossy();

    // Skip templates directory
    if rel_str.contains("_templates") {
        return true;
    }

    // Skip README.md
    if rel.file_name().map(|n| n == "README.md").unwrap_or(false) {
        return true;
    }

    // Skip hidden files/directories (only check relative components)
    for component in rel.components() {
        if let std::path::Component::Normal(name) = component {
            if name.to_string_lossy().starts_with('.') {
                return true;
            }
        }
    }

    false
}

/// Parse a single markdown file into an Issue.
fn parse_issue_file(path: &Path, issues_dir: &Path) -> Result<Issue> {
    let content = std::fs::read_to_string(path)?;

    // Derive ID from relative path (strip issues_dir prefix and .md suffix)
    let rel = path.strip_prefix(issues_dir).unwrap_or(path);
    let id = rel.with_extension("").to_string_lossy().replace('\\', "/");

    // Parse title from first # heading
    let title = extract_title(&content).unwrap_or_else(|| id.clone());

    // Parse frontmatter fields
    let status = extract_field(&content, "Status")
        .and_then(|s| IssueStatus::from_str_loose(&s))
        .unwrap_or(IssueStatus::Planned);

    let priority =
        extract_field(&content, "Priority").and_then(|s| IssuePriority::from_str_loose(&s));

    let category = extract_field(&content, "Category").or_else(|| infer_category(rel));

    let depends_on = extract_field(&content, "Depends-On")
        .map(|s| {
            s.split(',')
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let children = extract_children(&content, rel);

    Ok(Issue {
        id,
        title,
        status,
        priority,
        category,
        depends_on,
        body: content,
        source: path.to_string_lossy().to_string(),
        children,
    })
}

/// Extract the first `# Heading` from markdown content.
fn extract_title(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix("# ") {
            return Some(heading.trim().to_string());
        }
    }
    None
}

/// Extract a `**Key:** Value` field from markdown frontmatter.
fn extract_field(content: &str, key: &str) -> Option<String> {
    let prefix = format!("**{}:**", key);
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(&prefix) {
            let value = rest.trim().trim_end_matches("  ").trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Infer category from the parent directory name.
fn infer_category(rel_path: &Path) -> Option<String> {
    let components: Vec<_> = rel_path.components().collect();
    if components.len() >= 2 {
        if let std::path::Component::Normal(dir) = components[0] {
            return Some(dir.to_string_lossy().to_string());
        }
    }
    None
}

/// Extract child ticket IDs from a `## Tickets` table in an epic index.
fn extract_children(content: &str, parent_rel: &Path) -> Vec<String> {
    let parent_dir = parent_rel.parent().unwrap_or(Path::new(""));
    let mut children = Vec::new();
    let mut in_tickets = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## Tickets") {
            in_tickets = true;
            continue;
        }
        if in_tickets && trimmed.starts_with("## ") {
            break;
        }
        if !in_tickets {
            continue;
        }

        // Parse table rows like: | 0 | [Title](./0-file.md) | Status |
        if trimmed.starts_with('|') && trimmed.contains("](") {
            if let Some(link) = extract_markdown_link(trimmed) {
                // Strip leading "./" from the link before joining
                let link = link.strip_prefix("./").unwrap_or(link);
                let child_path = parent_dir.join(link);
                let child_id = child_path
                    .with_extension("")
                    .to_string_lossy()
                    .replace('\\', "/");
                children.push(child_id);
            }
        }
    }

    children
}

/// Extract the first markdown link target from a string: `[text](target)`.
fn extract_markdown_link(s: &str) -> Option<&str> {
    let start = s.find("](")?;
    let rest = &s[start + 2..];
    let end = rest.find(')')?;
    Some(&rest[..end])
}

/// Sort issues: by priority (urgent first), then alphabetical by id.
fn sort_issues(issues: &mut [Issue]) {
    issues.sort_by(|a, b| {
        let pa = a.priority.as_ref().map(|p| p.clone() as u8).unwrap_or(99);
        let pb = b.priority.as_ref().map(|p| p.clone() as u8).unwrap_or(99);
        pa.cmp(&pb).then_with(|| a.id.cmp(&b.id))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_title_from_heading() {
        assert_eq!(
            extract_title("# My Issue\n\nSome content"),
            Some("My Issue".to_string())
        );
    }

    #[test]
    fn extract_title_skips_non_h1() {
        assert_eq!(
            extract_title("## Subheading\n# Actual Title"),
            Some("Actual Title".to_string())
        );
    }

    #[test]
    fn extract_field_basic() {
        let content = "# Title\n\n**Status:** Planned  \n**Priority:** High\n";
        assert_eq!(
            extract_field(content, "Status"),
            Some("Planned".to_string())
        );
        assert_eq!(extract_field(content, "Priority"), Some("High".to_string()));
        assert_eq!(extract_field(content, "Missing"), None);
    }

    #[test]
    fn extract_field_trailing_spaces() {
        let content = "**Status:** In Progress  \n";
        assert_eq!(
            extract_field(content, "Status"),
            Some("In Progress".to_string())
        );
    }

    #[test]
    fn infer_category_from_path() {
        assert_eq!(
            infer_category(Path::new("features/my-feature.md")),
            Some("features".to_string())
        );
        assert_eq!(
            infer_category(Path::new("bugs/crash.md")),
            Some("bugs".to_string())
        );
        assert_eq!(infer_category(Path::new("standalone.md")), None);
    }

    #[test]
    fn extract_markdown_link_basic() {
        assert_eq!(
            extract_markdown_link("| 0 | [Title](./0-file.md) | Status |"),
            Some("./0-file.md")
        );
    }

    #[test]
    fn extract_children_from_table() {
        let content = "# Epic\n\n## Tickets\n\n| # | Ticket | Status |\n|---|--------|--------|\n| 0 | [First](./0-first.md) | Planned |\n| 1 | [Second](./1-second.md) | Planned |\n\n## Other\n";
        let children = extract_children(content, Path::new("epics/my-epic/index.md"));
        assert_eq!(
            children,
            vec!["epics/my-epic/0-first", "epics/my-epic/1-second"]
        );
    }

    #[test]
    fn should_skip_templates() {
        assert!(should_skip(Path::new("_templates/standalone.md")));
    }

    #[test]
    fn should_skip_readme() {
        assert!(should_skip(Path::new("README.md")));
    }

    #[test]
    fn should_skip_hidden() {
        assert!(should_skip(Path::new(".hidden/test.md")));
    }

    #[test]
    fn should_not_skip_regular() {
        assert!(!should_skip(Path::new("features/my-feature.md")));
    }

    #[test]
    fn parse_issue_file_full() {
        let tmp = tempfile::tempdir().unwrap();
        let features = tmp.path().join("features");
        std::fs::create_dir_all(&features).unwrap();
        let path = features.join("test-feature.md");
        std::fs::write(
            &path,
            "# Test Feature\n\n**Status:** Planned  \n**Priority:** High  \n**Category:** Features  \n**Depends-On:** bugs/fix-first.md\n\n## Objective\n\nDo the thing.\n",
        )
        .unwrap();

        let issue = parse_issue_file(&path, tmp.path()).unwrap();
        assert_eq!(issue.id, "features/test-feature");
        assert_eq!(issue.title, "Test Feature");
        assert_eq!(issue.status, IssueStatus::Planned);
        assert_eq!(issue.priority, Some(IssuePriority::High));
        assert_eq!(issue.category, Some("Features".to_string()));
        assert_eq!(issue.depends_on, vec!["bugs/fix-first.md"]);
    }

    #[test]
    fn file_provider_list_and_get() {
        let tmp = tempfile::tempdir().unwrap();
        let bugs = tmp.path().join("bugs");
        std::fs::create_dir_all(&bugs).unwrap();
        std::fs::write(
            bugs.join("crash.md"),
            "# Fix Crash\n\n**Status:** Planned\n**Priority:** Urgent\n",
        )
        .unwrap();
        std::fs::write(
            bugs.join("typo.md"),
            "# Fix Typo\n\n**Status:** Complete\n**Priority:** Low\n",
        )
        .unwrap();

        let provider = FileProvider::new(tmp.path());

        // List all
        let all = provider.list(&IssueFilter::default()).unwrap();
        assert_eq!(all.len(), 2);

        // Filter planned
        let planned = provider
            .list(&IssueFilter {
                status: Some(IssueStatus::Planned),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].title, "Fix Crash");

        // Get by ID
        let issue = provider.get("bugs/crash").unwrap();
        assert!(issue.is_some());
        assert_eq!(issue.unwrap().title, "Fix Crash");

        // Get non-existent
        let missing = provider.get("nope").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn sort_by_priority_then_id() {
        let mut issues = vec![
            Issue {
                id: "b-low".into(),
                title: String::new(),
                status: IssueStatus::Planned,
                priority: Some(IssuePriority::Low),
                category: None,
                depends_on: vec![],
                body: String::new(),
                source: String::new(),
                children: vec![],
            },
            Issue {
                id: "a-urgent".into(),
                title: String::new(),
                status: IssueStatus::Planned,
                priority: Some(IssuePriority::Urgent),
                category: None,
                depends_on: vec![],
                body: String::new(),
                source: String::new(),
                children: vec![],
            },
        ];
        sort_issues(&mut issues);
        assert_eq!(issues[0].id, "a-urgent");
        assert_eq!(issues[1].id, "b-low");
    }
}
