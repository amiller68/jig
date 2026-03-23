//! File-based issue provider.
//!
//! Walks the `issues/` directory, parses `**Key:** Value` frontmatter from
//! markdown files, and returns structured `Issue` values.
//!
//! When a `git_ref` is configured, reads files from that ref (e.g.
//! `origin/main`) via `git ls-tree` / `git show` instead of the working tree.
//! This keeps issue discovery up-to-date with the remote without requiring a
//! local pull.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::{Error, Result};

use super::provider::IssueProvider;
use super::types::{Issue, IssueFilter, IssuePriority, IssueStatus};

/// File-based issue provider that reads from an `issues/` directory.
pub struct FileProvider {
    /// Root directory containing issue files (e.g. `<repo>/issues`).
    issues_dir: PathBuf,
    /// If set, read files from this git ref instead of the working tree.
    git_ref: Option<GitRefSource>,
}

/// Configuration for reading issues from a git ref.
struct GitRefSource {
    /// Repository root (for running git commands).
    repo_root: PathBuf,
    /// Git ref to read from (e.g. "origin/main").
    git_ref: String,
    /// Issues directory relative to repo root (e.g. "issues").
    rel_dir: String,
}

impl FileProvider {
    /// The provider kind for file-based issues.
    pub const PROVIDER_KIND: super::provider::ProviderKind = super::provider::ProviderKind::File;

    pub fn new(issues_dir: impl Into<PathBuf>) -> Self {
        Self {
            issues_dir: issues_dir.into(),
            git_ref: None,
        }
    }

    /// Configure this provider to read from a git ref instead of the working tree.
    pub fn with_git_ref(
        mut self,
        repo_root: impl Into<PathBuf>,
        git_ref: impl Into<String>,
        rel_dir: impl Into<String>,
    ) -> Self {
        self.git_ref = Some(GitRefSource {
            repo_root: repo_root.into(),
            git_ref: git_ref.into(),
            rel_dir: rel_dir.into(),
        });
        self
    }

    /// Scan all markdown files and parse them into issues.
    fn scan_all(&self) -> Result<Vec<Issue>> {
        let mut issues = if let Some(ref src) = self.git_ref {
            self.scan_all_from_ref(src)?
        } else {
            self.scan_all_from_disk()?
        };
        sort_issues(&mut issues);
        Ok(issues)
    }

    /// Scan issues from the working tree (original behavior).
    fn scan_all_from_disk(&self) -> Result<Vec<Issue>> {
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

        Ok(issues)
    }

    /// Scan issues from a git ref using `git ls-tree` and `git show`.
    fn scan_all_from_ref(&self, src: &GitRefSource) -> Result<Vec<Issue>> {
        let files = git_ls_tree(&src.repo_root, &src.git_ref, &src.rel_dir)?;

        let mut issues = Vec::new();
        for rel_path in &files {
            if should_skip(Path::new(rel_path)) {
                continue;
            }
            let blob_path = format!("{}:{}/{}", src.git_ref, src.rel_dir, rel_path);
            let content = match git_show(&src.repo_root, &blob_path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::debug!("skipping {}: {}", rel_path, e);
                    continue;
                }
            };
            match parse_issue_content(rel_path, &content) {
                Ok(issue) => issues.push(issue),
                Err(e) => {
                    tracing::debug!("skipping {}: {}", rel_path, e);
                }
            }
        }

        Ok(issues)
    }
}

impl FileProvider {
    /// Create a new issue file from a template.
    ///
    /// `template` is one of "standalone", "ticket", "epic-index" (or a custom name).
    /// Returns the ID of the newly created issue.
    pub fn create_issue(
        &self,
        title: &str,
        category: &str,
        template: &str,
        priority: Option<&IssuePriority>,
        labels: &[String],
    ) -> Result<String> {
        // Load template content
        let template_path = self
            .issues_dir
            .join("_templates")
            .join(format!("{}.md", template));
        let content = if template_path.exists() {
            std::fs::read_to_string(&template_path)?
        } else {
            // Fallback to standalone template
            let fallback = self.issues_dir.join("_templates/standalone.md");
            if fallback.exists() {
                std::fs::read_to_string(&fallback)?
            } else {
                // Minimal default
                "# [Title]\n\n**Status:** Planned\n\n## Objective\n\nDescribe the objective.\n"
                    .to_string()
            }
        };

        // Replace placeholder title
        let content = content.replace("[Title]", title);
        let content = content.replace("[Ticket Title]", title);
        let content = content.replace("[Epic Title]", title);

        // Inject priority if provided
        let content = if let Some(pri) = priority {
            if content.contains("**Priority:**") {
                content
            } else {
                // Insert after **Status:** line
                content.replace(
                    "**Status:** Planned",
                    &format!("**Status:** Planned\n**Priority:** {}", pri.as_str()),
                )
            }
        } else {
            content
        };

        // Inject labels if provided
        let content = if !labels.is_empty() {
            if content.contains("**Labels:**") {
                content
            } else {
                let labels_str = labels.join(", ");
                // Insert after **Status:** or **Priority:** line
                if content.contains("**Priority:**") {
                    // Find the priority line and insert after it
                    let mut lines: Vec<&str> = content.lines().collect();
                    let pos = lines
                        .iter()
                        .position(|l| l.trim().starts_with("**Priority:**"));
                    if let Some(idx) = pos {
                        lines.insert(idx + 1, &format!("**Labels:** {}", labels_str));
                        // Can't use format! result as &str reference, so rebuild differently
                    }
                    // Rebuild with labels
                    content.replace(
                        &format!(
                            "**Priority:** {}",
                            priority.map(|p| p.as_str()).unwrap_or("Medium")
                        ),
                        &format!(
                            "**Priority:** {}\n**Labels:** {}",
                            priority.map(|p| p.as_str()).unwrap_or("Medium"),
                            labels_str,
                        ),
                    )
                } else {
                    content.replace(
                        "**Status:** Planned",
                        &format!("**Status:** Planned\n**Labels:** {}", labels_str),
                    )
                }
            }
        } else {
            content
        };

        // Derive filename from title (kebab-case)
        let slug: String = title
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");

        let dir = self.issues_dir.join(category);
        std::fs::create_dir_all(&dir)?;

        let file_path = dir.join(format!("{}.md", slug));
        if file_path.exists() {
            return Err(Error::Custom(format!(
                "issue file already exists: {}",
                file_path.display()
            )));
        }

        std::fs::write(&file_path, &content)?;

        let id = format!("{}/{}", category, slug);
        Ok(id)
    }

    /// Update the status field in an issue file.
    pub fn update_status(&self, id: &str, new_status: &IssueStatus) -> Result<()> {
        let path = self.resolve_path(id)?;
        let content = std::fs::read_to_string(&path)?;
        let updated = replace_field(&content, "Status", new_status.as_str());
        std::fs::write(&path, updated)?;
        Ok(())
    }

    /// Update fields of an existing issue file.
    ///
    /// Only fields that are `Some` / non-empty are updated.
    pub fn update_issue(
        &self,
        id: &str,
        title: Option<&str>,
        body: Option<&str>,
        priority: Option<&IssuePriority>,
        labels: &[String],
        category: Option<&str>,
    ) -> Result<()> {
        let path = self.resolve_path(id)?;
        let mut content = std::fs::read_to_string(&path)?;

        if let Some(new_title) = title {
            content = replace_title(&content, new_title);
        }

        if let Some(pri) = priority {
            content = replace_field(&content, "Priority", pri.as_str());
        }

        if !labels.is_empty() {
            let labels_str = labels.join(", ");
            content = replace_field(&content, "Labels", &labels_str);
        }

        if let Some(cat) = category {
            content = replace_field(&content, "Category", cat);
        }

        if let Some(new_body) = body {
            content = replace_body(&content, new_body);
        }

        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Delete an issue file.
    pub fn delete_issue(&self, id: &str) -> Result<()> {
        let path = self.resolve_path(id)?;
        std::fs::remove_file(&path)?;
        Ok(())
    }

    /// Resolve an issue ID to its file path on disk.
    fn resolve_path(&self, id: &str) -> Result<PathBuf> {
        let candidates = [
            self.issues_dir.join(format!("{}.md", id)),
            self.issues_dir.join(id),
        ];
        for path in &candidates {
            if path.exists() && path.extension().map(|e| e == "md").unwrap_or(false) {
                return Ok(path.clone());
            }
        }
        Err(Error::Custom(format!("issue file not found: {}", id)))
    }
}

/// Replace a `**Key:** Value` field in markdown content.
fn replace_field(content: &str, key: &str, new_value: &str) -> String {
    let prefix = format!("**{}:**", key);
    let mut result = Vec::new();
    let mut found = false;

    for line in content.lines() {
        if line.trim().starts_with(&prefix) {
            result.push(format!("**{}:** {}", key, new_value));
            found = true;
        } else {
            result.push(line.to_string());
        }
    }

    if !found {
        // Insert after first heading if field didn't exist
        let mut inserted = false;
        let mut final_result = Vec::new();
        for line in &result {
            final_result.push(line.clone());
            if !inserted && line.starts_with("# ") {
                final_result.push(String::new());
                final_result.push(format!("**{}:** {}", key, new_value));
                inserted = true;
            }
        }
        if inserted {
            return final_result.join("\n");
        }
    }

    result.join("\n")
}

/// Replace the first `# Heading` in markdown content with a new title.
fn replace_title(content: &str, new_title: &str) -> String {
    let mut result = Vec::new();
    let mut replaced = false;

    for line in content.lines() {
        if !replaced && line.trim().starts_with("# ") {
            result.push(format!("# {}", new_title));
            replaced = true;
        } else {
            result.push(line.to_string());
        }
    }

    result.join("\n")
}

/// Replace the body content (everything after the frontmatter section) with new text.
///
/// Frontmatter is defined as: the title heading, blank lines, and `**Key:** Value` lines.
/// Everything after the last frontmatter line is replaced.
fn replace_body(content: &str, new_body: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut last_frontmatter_idx = 0;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") || trimmed.starts_with("**") || trimmed.is_empty() {
            last_frontmatter_idx = i;
        } else {
            break;
        }
    }

    let mut result: Vec<String> = lines[..=last_frontmatter_idx]
        .iter()
        .map(|l| l.to_string())
        .collect();

    // Ensure blank line before body
    if !result.last().map(|l| l.is_empty()).unwrap_or(true) {
        result.push(String::new());
    }

    result.push(new_body.to_string());

    // Ensure trailing newline
    let joined = result.join("\n");
    if joined.ends_with('\n') {
        joined
    } else {
        format!("{}\n", joined)
    }
}

impl IssueProvider for FileProvider {
    fn name(&self) -> &str {
        "file"
    }

    fn kind(&self) -> super::provider::ProviderKind {
        Self::PROVIDER_KIND
    }

    fn update_status(&self, id: &str, new_status: &IssueStatus) -> Result<()> {
        self.update_status(id, new_status)
    }

    fn list(&self, filter: &IssueFilter) -> Result<Vec<Issue>> {
        let all = self.scan_all()?;
        Ok(all.into_iter().filter(|i| i.matches(filter)).collect())
    }

    fn get(&self, id: &str) -> Result<Option<Issue>> {
        if let Some(ref src) = self.git_ref {
            // Try with and without .md extension
            for rel_path in &[format!("{}.md", id), id.to_string()] {
                if !rel_path.ends_with(".md") {
                    continue;
                }
                let blob_path = format!("{}:{}/{}", src.git_ref, src.rel_dir, rel_path);
                if let Ok(content) = git_show(&src.repo_root, &blob_path) {
                    return parse_issue_content(rel_path, &content).map(Some);
                }
            }
            return Ok(None);
        }

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

/// List `.md` files under a directory in a git ref.
/// Returns paths relative to `dir` (e.g. "features/my-feature.md").
fn git_ls_tree(repo_root: &Path, git_ref: &str, dir: &str) -> Result<Vec<String>> {
    let tree_path = format!("{}:{}", git_ref, dir);
    let output = Command::new("git")
        .args(["ls-tree", "-r", "--name-only", &tree_path])
        .current_dir(repo_root)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()?;

    if !output.status.success() {
        // Ref or dir doesn't exist — not an error, just no issues
        tracing::debug!(
            "git ls-tree failed for {} (ref may not exist yet)",
            tree_path
        );
        return Ok(Vec::new());
    }

    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text
        .lines()
        .filter(|l| l.ends_with(".md"))
        .map(|l| l.to_string())
        .collect())
}

/// Read a file from a git ref via `git show <blob_path>`.
fn git_show(repo_root: &Path, blob_path: &str) -> Result<String> {
    let output = Command::new("git")
        .args(["show", blob_path])
        .current_dir(repo_root)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()?;

    if !output.status.success() {
        return Err(crate::error::Error::Custom(format!(
            "git show {} failed",
            blob_path
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Parse issue content from a string (used for git-ref-based reading).
/// `rel_path` is relative to the issues directory (e.g. "features/my-feature.md").
fn parse_issue_content(rel_path: &str, content: &str) -> Result<Issue> {
    let rel = Path::new(rel_path);
    let id = rel.with_extension("").to_string_lossy().replace('\\', "/");

    let title = extract_title(content).unwrap_or_else(|| id.clone());

    let status = extract_field(content, "Status")
        .and_then(|s| IssueStatus::from_str_loose(&s))
        .unwrap_or(IssueStatus::Planned);

    let priority =
        extract_field(content, "Priority").and_then(|s| IssuePriority::from_str_loose(&s));

    let category = extract_field(content, "Category").or_else(|| infer_category(rel));

    let depends_on = extract_field(content, "Depends-On")
        .map(|s| {
            s.split(',')
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let children = extract_children(content, rel);

    let labels = extract_field(content, "Labels")
        .map(|s| {
            s.split(',')
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default();

    Ok(Issue {
        id,
        title,
        status,
        priority,
        category,
        depends_on,
        body: content.to_string(),
        source: rel_path.to_string(),
        children,
        labels,
        branch_name: None,
    })
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

    let labels = extract_field(&content, "Labels")
        .map(|s| {
            s.split(',')
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default();

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
        labels,
        branch_name: None,
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
    fn list_spawnable_respects_depends_on() {
        let tmp = tempfile::tempdir().unwrap();
        let bugs = tmp.path().join("bugs");
        std::fs::create_dir_all(&bugs).unwrap();

        // Issue B: no dependencies, labeled, planned → spawnable
        std::fs::write(
            bugs.join("fix-first.md"),
            "# Fix First\n\n**Status:** Planned\n**Labels:** auto\n",
        )
        .unwrap();

        // Issue A: depends on B, labeled, planned → NOT spawnable (B is Planned, not Complete)
        std::fs::write(
            bugs.join("depends-on-b.md"),
            "# Depends On B\n\n**Status:** Planned\n**Labels:** auto\n**Depends-On:** bugs/fix-first\n",
        )
        .unwrap();

        let provider = FileProvider::new(tmp.path());
        let labels = vec!["auto".to_string()];

        // Only B should be spawnable
        let spawnable = provider.list_spawnable(&labels).unwrap();
        assert_eq!(spawnable.len(), 1);
        assert_eq!(spawnable[0].id, "bugs/fix-first");

        // Mark B as Complete → A should now also be spawnable
        std::fs::write(
            bugs.join("fix-first.md"),
            "# Fix First\n\n**Status:** Complete\n**Labels:** auto\n",
        )
        .unwrap();

        let spawnable = provider.list_spawnable(&labels).unwrap();
        assert_eq!(spawnable.len(), 1);
        assert_eq!(spawnable[0].id, "bugs/depends-on-b");
    }

    #[test]
    fn list_spawnable_missing_dependency_blocks() {
        let tmp = tempfile::tempdir().unwrap();
        let features = tmp.path().join("features");
        std::fs::create_dir_all(&features).unwrap();

        std::fs::write(
            features.join("needs-ghost.md"),
            "# Needs Ghost\n\n**Status:** Planned\n**Labels:** auto\n**Depends-On:** nonexistent/issue\n",
        )
        .unwrap();

        let provider = FileProvider::new(tmp.path());
        let spawnable = provider.list_spawnable(&["auto".to_string()]).unwrap();
        assert!(spawnable.is_empty());
    }

    #[test]
    fn list_spawnable_filters_by_spawn_labels() {
        let tmp = tempfile::tempdir().unwrap();
        let features = tmp.path().join("features");
        std::fs::create_dir_all(&features).unwrap();

        std::fs::write(
            features.join("backend-task.md"),
            "# Backend Task\n\n**Status:** Planned\n**Labels:** backend, sprint-1\n",
        )
        .unwrap();
        std::fs::write(
            features.join("frontend-task.md"),
            "# Frontend Task\n\n**Status:** Planned\n**Labels:** frontend, sprint-1\n",
        )
        .unwrap();
        std::fs::write(
            features.join("unlabeled.md"),
            "# Unlabeled\n\n**Status:** Planned\n",
        )
        .unwrap();

        let provider = FileProvider::new(tmp.path());

        // Empty spawn_labels → all planned issues
        let all = provider.list_spawnable(&[]).unwrap();
        assert_eq!(all.len(), 3);

        // Filter to backend only
        let backend = provider.list_spawnable(&["backend".to_string()]).unwrap();
        assert_eq!(backend.len(), 1);
        assert_eq!(backend[0].title, "Backend Task");

        // Filter requiring both backend + sprint-1
        let both = provider
            .list_spawnable(&["backend".to_string(), "sprint-1".to_string()])
            .unwrap();
        assert_eq!(both.len(), 1);

        // Filter to sprint-1 → matches both labeled issues
        let sprint = provider.list_spawnable(&["sprint-1".to_string()]).unwrap();
        assert_eq!(sprint.len(), 2);

        // Filter to nonexistent label → empty
        let none = provider
            .list_spawnable(&["nonexistent".to_string()])
            .unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn parse_labels_from_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("labeled.md"),
            "# Labeled Issue\n\n**Status:** Planned\n**Labels:** backend, auth, sprint-12\n",
        )
        .unwrap();

        let provider = FileProvider::new(tmp.path());
        let issues = provider.list(&IssueFilter::default()).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].labels, vec!["backend", "auth", "sprint-12"]);
    }

    #[test]
    fn is_spawnable_with_deps_no_deps() {
        let tmp = tempfile::tempdir().unwrap();
        let features = tmp.path().join("features");
        std::fs::create_dir_all(&features).unwrap();

        std::fs::write(
            features.join("standalone.md"),
            "# Standalone\n\n**Status:** Planned\n",
        )
        .unwrap();

        let provider = FileProvider::new(tmp.path());
        let issues = provider.list(&IssueFilter::default()).unwrap();
        assert_eq!(issues.len(), 1);
        assert!(provider.is_spawnable_with_deps(&issues[0]));
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
                labels: vec![],
                branch_name: None,
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
                labels: vec![],
                branch_name: None,
            },
        ];
        sort_issues(&mut issues);
        assert_eq!(issues[0].id, "a-urgent");
        assert_eq!(issues[1].id, "b-low");
    }

    #[test]
    fn create_issue_basic() {
        let tmp = tempfile::tempdir().unwrap();
        let templates = tmp.path().join("_templates");
        std::fs::create_dir_all(&templates).unwrap();
        std::fs::write(
            templates.join("standalone.md"),
            "# [Title]\n\n**Status:** Planned\n\n## Objective\n\nDescribe.\n",
        )
        .unwrap();

        let provider = FileProvider::new(tmp.path());
        let id = provider
            .create_issue("Add verbose flag", "features", "standalone", None, &[])
            .unwrap();

        assert_eq!(id, "features/add-verbose-flag");

        // Verify the file was created and can be read back
        let issue = provider.get(&id).unwrap().unwrap();
        assert_eq!(issue.title, "Add verbose flag");
        assert_eq!(issue.status, IssueStatus::Planned);
    }

    #[test]
    fn create_issue_with_priority_and_labels() {
        let tmp = tempfile::tempdir().unwrap();
        let templates = tmp.path().join("_templates");
        std::fs::create_dir_all(&templates).unwrap();
        std::fs::write(
            templates.join("standalone.md"),
            "# [Title]\n\n**Status:** Planned\n\n## Objective\n\nDescribe.\n",
        )
        .unwrap();

        let provider = FileProvider::new(tmp.path());
        let id = provider
            .create_issue(
                "Fix crash on exit",
                "bugs",
                "standalone",
                Some(&IssuePriority::High),
                &["auto".to_string(), "backend".to_string()],
            )
            .unwrap();

        let issue = provider.get(&id).unwrap().unwrap();
        assert_eq!(issue.title, "Fix crash on exit");
        assert_eq!(issue.priority, Some(IssuePriority::High));
        assert_eq!(issue.labels, vec!["auto", "backend"]);
    }

    #[test]
    fn create_issue_duplicate_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let provider = FileProvider::new(tmp.path());
        provider
            .create_issue("My Feature", "features", "standalone", None, &[])
            .unwrap();

        let result = provider.create_issue("My Feature", "features", "standalone", None, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn update_status_changes_field() {
        let tmp = tempfile::tempdir().unwrap();
        let features = tmp.path().join("features");
        std::fs::create_dir_all(&features).unwrap();
        std::fs::write(
            features.join("my-feature.md"),
            "# My Feature\n\n**Status:** Planned\n\n## Objective\n\nDo stuff.\n",
        )
        .unwrap();

        let provider = FileProvider::new(tmp.path());
        provider
            .update_status("features/my-feature", &IssueStatus::InProgress)
            .unwrap();

        let issue = provider.get("features/my-feature").unwrap().unwrap();
        assert_eq!(issue.status, IssueStatus::InProgress);
    }

    #[test]
    fn update_status_to_complete() {
        let tmp = tempfile::tempdir().unwrap();
        let bugs = tmp.path().join("bugs");
        std::fs::create_dir_all(&bugs).unwrap();
        std::fs::write(
            bugs.join("crash.md"),
            "# Fix Crash\n\n**Status:** In Progress\n**Priority:** Urgent\n",
        )
        .unwrap();

        let provider = FileProvider::new(tmp.path());
        provider
            .update_status("bugs/crash", &IssueStatus::Complete)
            .unwrap();

        let issue = provider.get("bugs/crash").unwrap().unwrap();
        assert_eq!(issue.status, IssueStatus::Complete);
        // Priority should be unchanged
        assert_eq!(issue.priority, Some(IssuePriority::Urgent));
    }

    #[test]
    fn delete_issue_removes_file() {
        let tmp = tempfile::tempdir().unwrap();
        let features = tmp.path().join("features");
        std::fs::create_dir_all(&features).unwrap();
        std::fs::write(features.join("test.md"), "# Test\n\n**Status:** Complete\n").unwrap();

        let provider = FileProvider::new(tmp.path());
        assert!(provider.get("features/test").unwrap().is_some());

        provider.delete_issue("features/test").unwrap();
        assert!(provider.get("features/test").unwrap().is_none());
    }

    #[test]
    fn replace_field_in_content() {
        let content = "# Title\n\n**Status:** Planned\n**Priority:** High\n\n## Body\n";
        let updated = replace_field(content, "Status", "Complete");
        assert!(updated.contains("**Status:** Complete"));
        assert!(updated.contains("**Priority:** High"));
    }

    #[test]
    fn replace_field_inserts_when_missing() {
        let content = "# Title\n\n## Body\n";
        let updated = replace_field(content, "Status", "Planned");
        assert!(updated.contains("**Status:** Planned"));
    }

    #[test]
    fn replace_title_changes_heading() {
        let content = "# Old Title\n\n**Status:** Planned\n\n## Objective\n";
        let updated = replace_title(content, "New Title");
        assert!(updated.contains("# New Title"));
        assert!(!updated.contains("# Old Title"));
        assert!(updated.contains("**Status:** Planned"));
    }

    #[test]
    fn replace_body_replaces_content_after_frontmatter() {
        let content =
            "# Title\n\n**Status:** Planned\n**Priority:** High\n\n## Objective\n\nOld body.\n";
        let updated = replace_body(content, "New body content.");
        assert!(updated.contains("# Title"));
        assert!(updated.contains("**Status:** Planned"));
        assert!(updated.contains("**Priority:** High"));
        assert!(updated.contains("New body content."));
        assert!(!updated.contains("Old body."));
    }

    #[test]
    fn update_issue_title_and_priority() {
        let tmp = tempfile::tempdir().unwrap();
        let features = tmp.path().join("features");
        std::fs::create_dir_all(&features).unwrap();
        std::fs::write(
            features.join("my-feature.md"),
            "# My Feature\n\n**Status:** Planned\n**Priority:** High\n\n## Objective\n\nDo stuff.\n",
        )
        .unwrap();

        let provider = FileProvider::new(tmp.path());
        provider
            .update_issue(
                "features/my-feature",
                Some("Renamed Feature"),
                None,
                Some(&IssuePriority::Urgent),
                &[],
                None,
            )
            .unwrap();

        let issue = provider.get("features/my-feature").unwrap().unwrap();
        assert_eq!(issue.title, "Renamed Feature");
        assert_eq!(issue.priority, Some(IssuePriority::Urgent));
    }

    #[test]
    fn update_issue_labels() {
        let tmp = tempfile::tempdir().unwrap();
        let features = tmp.path().join("features");
        std::fs::create_dir_all(&features).unwrap();
        std::fs::write(
            features.join("my-feature.md"),
            "# My Feature\n\n**Status:** Planned\n\n## Objective\n\nDo stuff.\n",
        )
        .unwrap();

        let provider = FileProvider::new(tmp.path());
        provider
            .update_issue(
                "features/my-feature",
                None,
                None,
                None,
                &["backend".to_string(), "auto".to_string()],
                None,
            )
            .unwrap();

        let issue = provider.get("features/my-feature").unwrap().unwrap();
        assert_eq!(issue.labels, vec!["backend", "auto"]);
    }
}
