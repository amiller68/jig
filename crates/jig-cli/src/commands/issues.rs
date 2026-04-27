//! Issues command — discover, browse, and manage file-based issues.

use std::fmt;
use std::io::{self, Write};

use clap::{Args, Subcommand};
use comfy_table::{Cell, Color};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal;

use jig_core::issues::{
    self, Issue as CoreIssue, IssueFilter, IssuePriority, IssueStatus, ProviderKind,
};

use crate::op::{GlobalCtx, Op, RepoCtx};
use crate::ui;

/// Discover and manage issues
#[derive(Args, Debug, Clone)]
pub struct Issues {
    #[command(subcommand)]
    pub command: Option<IssuesCommand>,

    /// Show a single issue by ID (e.g. "features/my-feature")
    #[arg(value_name = "ID")]
    pub id: Option<String>,

    /// Filter by status (planned, in-progress, complete, blocked)
    #[arg(short, long)]
    pub status: Option<String>,

    /// Filter by priority (urgent, high, medium, low)
    #[arg(short, long)]
    pub priority: Option<String>,

    /// Filter by category (features, bugs, chores, etc.)
    #[arg(short, long)]
    pub category: Option<String>,

    /// Filter by label (can specify multiple; all must match)
    #[arg(short, long)]
    pub label: Vec<String>,

    /// Show only issues with unresolved dependencies
    #[arg(long)]
    pub blocked: bool,

    /// Show only issues with all dependencies resolved (or no dependencies)
    #[arg(long)]
    pub unblocked: bool,

    /// Interactive expand/collapse mode
    #[arg(short, long)]
    pub interactive: bool,

    /// Show only auto-spawn candidates (planned, labeled, deps satisfied)
    #[arg(long)]
    pub auto: bool,

    /// Include completed/canceled issues (excluded by default)
    #[arg(long)]
    pub all: bool,

    /// Print issue IDs only (one per line, for scripting)
    #[arg(long)]
    pub ids: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum IssuesCommand {
    /// Create a new issue
    Create {
        /// Issue title
        title: String,

        /// Template to use (standalone, ticket, epic-index) — file provider only
        #[arg(short, long, default_value = "standalone")]
        template: String,

        /// Issue priority (urgent, high, medium, low)
        #[arg(short, long)]
        priority: Option<String>,

        /// Category/directory (file) or project name (Linear)
        #[arg(short, long)]
        category: Option<String>,

        /// Labels to attach (can specify multiple -l flags)
        #[arg(short, long)]
        label: Vec<String>,

        /// Issue body/description (use "-" to read from stdin)
        #[arg(short, long)]
        body: Option<String>,

        /// Parent issue ID (e.g. "JIG-19") to create this as a sub-issue
        #[arg(short = 'P', long)]
        parent: Option<String>,

        /// Initial status (triage, backlog, planned, in-progress, complete, blocked)
        #[arg(short = 's', long, default_value = "backlog")]
        status: String,
    },

    /// Update an existing issue's fields
    Update {
        /// Issue ID (e.g. "features/my-feature" or "AUT-123")
        id: String,

        /// New title
        #[arg(short, long)]
        title: Option<String>,

        /// New body/description (use "-" to read from stdin)
        #[arg(short, long)]
        body: Option<String>,

        /// Append body to existing description instead of replacing it
        #[arg(short, long)]
        append: bool,

        /// New priority (urgent, high, medium, low)
        #[arg(short, long)]
        priority: Option<String>,

        /// Labels to set — REPLACES the current label set (can specify multiple -l flags).
        /// Use --add-label / --remove-label for additive edits.
        #[arg(short, long)]
        label: Vec<String>,

        /// Labels to add to the existing set (repeatable)
        #[arg(long = "add-label")]
        add_label: Vec<String>,

        /// Labels to remove from the existing set (repeatable)
        #[arg(long = "remove-label")]
        remove_label: Vec<String>,

        /// Category/directory (file) or project name (Linear)
        #[arg(short, long)]
        category: Option<String>,

        /// Assignee — "me" for the authenticated user, or a Linear user ID
        /// (Linear provider only; ignored by the file provider)
        #[arg(short = 'A', long)]
        assignee: Option<String>,

        /// Add blocking dependencies (issue IDs that block this issue)
        #[arg(long, value_delimiter = ',')]
        blocked_by: Vec<String>,

        /// Remove blocking dependencies
        #[arg(long, value_delimiter = ',')]
        remove_blocked_by: Vec<String>,

        /// Parent issue ID (e.g. "JIG-19") to set as parent
        #[arg(short = 'P', long)]
        parent: Option<String>,

        /// Remove the parent issue relation
        #[arg(long)]
        remove_parent: bool,
    },

    /// Update issue status
    Status {
        /// Issue ID (e.g. "features/my-feature")
        id: String,

        /// New status (triage, backlog, planned, in-progress, complete, blocked)
        #[arg(short, long)]
        status: String,
    },

    /// Mark an issue as complete
    Complete {
        /// Issue ID (e.g. "features/my-feature")
        id: String,

        /// Delete the issue file after marking complete
        #[arg(long)]
        delete: bool,
    },

    /// Show issue statistics
    Stats,
}

#[derive(Debug, thiserror::Error)]
pub enum IssuesError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
    #[error("{0}")]
    Usage(String),
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug)]
pub enum IssuesOutput {
    Table(Vec<CoreIssue>, Option<Vec<String>>),
    Detail(Box<CoreIssue>),
    Interactive,
    Ids(Vec<String>),
    Created(String),
    Updated(String),
    StatusUpdated(String, String),
    Completed(String, bool),
    Stats(StatsData),
}

#[derive(Debug)]
pub struct StatsData {
    pub by_status: Vec<(String, usize)>,
    pub by_priority: Vec<(String, usize)>,
}

impl Issues {
    fn filter(&self) -> IssueFilter {
        IssueFilter {
            status: self.status.as_deref().and_then(|s| s.parse().ok()),
            priority: self.priority.as_deref().and_then(|s| s.parse().ok()),
            labels: self.label.clone(),
        }
    }

    /// Filter out completed issues unless --all or --status was specified.
    fn exclude_completed(&self, issues: Vec<CoreIssue>) -> Vec<CoreIssue> {
        if self.all || self.status.is_some() {
            return issues;
        }
        issues
            .into_iter()
            .filter(|i| *i.status() != IssueStatus::Complete)
            .collect()
    }

    fn apply_dep_filter(
        &self,
        issues: Vec<CoreIssue>,
        provider: &issues::IssueProvider,
    ) -> Vec<CoreIssue> {
        if self.blocked {
            issues
                .into_iter()
                .filter(|i| !provider.is_spawnable_with_deps(i))
                .collect()
        } else if self.unblocked {
            issues
                .into_iter()
                .filter(|i| provider.is_spawnable_with_deps(i))
                .collect()
        } else {
            issues
        }
    }

    fn finish(
        &self,
        all_issues: Vec<CoreIssue>,
        auto_spawn_labels: Option<Vec<String>>,
    ) -> Result<IssuesOutput, IssuesError> {
        if self.ids {
            let ids: Vec<String> = all_issues.into_iter().map(|i| i.into_id().into()).collect();
            return Ok(IssuesOutput::Ids(ids));
        }

        if self.interactive {
            run_interactive(&all_issues, auto_spawn_labels.as_deref())?;
            return Ok(IssuesOutput::Interactive);
        }

        Ok(IssuesOutput::Table(all_issues, auto_spawn_labels))
    }

    fn run_list(&self, ctx: &RepoCtx) -> Result<IssuesOutput, IssuesError> {
        let cfg = ctx.config()?;
        let filter = self.filter();
        let provider = cfg.issue_provider()?;

        if let Some(ref id) = self.id {
            let issue = provider
                .get(id)?
                .ok_or_else(|| IssuesError::Usage(format!("issue not found: {}", id)))?;
            return Ok(IssuesOutput::Detail(Box::new(issue)));
        }

        let spawn_labels = cfg.repo.issues.auto_spawn_labels.clone();
        let all_issues = if self.auto {
            let labels = spawn_labels.as_deref().unwrap_or(&[]);
            let spawnable = provider.list_spawnable(labels)?;
            // Apply additional filters on top of spawnable results
            filter.apply(spawnable)
        } else {
            provider.list(&filter)?
        };
        let all_issues = self.exclude_completed(all_issues);
        let all_issues = self.apply_dep_filter(all_issues, &provider);
        self.finish(all_issues, spawn_labels)
    }

    fn run_list_global(&self, ctx: &GlobalCtx) -> Result<IssuesOutput, IssuesError> {
        let filter = self.filter();

        let mut all_issues = Vec::new();
        for cfg in &ctx.configs {
            let provider = cfg.issue_provider()?;

            if let Some(ref id) = self.id {
                if let Some(issue) = provider.get(id)? {
                    return Ok(IssuesOutput::Detail(Box::new(issue)));
                }
                continue;
            }

            let spawn_labels = cfg.repo.issues.auto_spawn_labels.clone();
            let repo_issues = if self.auto {
                let labels = spawn_labels.as_deref().unwrap_or(&[]);
                let spawnable = provider.list_spawnable(labels)?;
                filter.apply(spawnable)
            } else {
                provider.list(&filter)?
            };
            let repo_issues = self.apply_dep_filter(repo_issues, &provider);
            all_issues.extend(repo_issues);
        }

        if let Some(id) = &self.id {
            return Err(IssuesError::Usage(format!("issue not found: {}", id)));
        }

        let all_issues = self.exclude_completed(all_issues);
        self.finish(all_issues, None)
    }
}

#[allow(clippy::too_many_arguments)]
fn run_create(
    ctx: &RepoCtx,
    title: &str,
    priority: Option<&str>,
    category: Option<&str>,
    labels: &[String],
    body: Option<&str>,
    parent: Option<&str>,
    status: &str,
) -> Result<IssuesOutput, IssuesError> {
    let cfg = ctx.config()?;
    let pri = priority.and_then(|s| s.parse().ok());

    let initial_status: IssueStatus = status
        .parse()
        .map_err(|_| IssuesError::Usage(format!("unknown status: {}", status)))?;

    // Read body from stdin if "-" was passed
    let body_text = match body {
        Some("-") => {
            let mut buf = String::new();
            io::Read::read_to_string(&mut io::stdin(), &mut buf)?;
            Some(buf)
        }
        Some(text) => Some(text.to_string()),
        None => None,
    };

    let id = match cfg.repo.issues.provider {
        ProviderKind::Linear => {
            let linear_provider = cfg.linear_provider()?;
            linear_provider.create_issue(
                title,
                body_text.as_deref(),
                pri.as_ref(),
                labels,
                category,
                parent,
                Some(&initial_status),
            )?
        }
    };

    Ok(IssuesOutput::Created(id))
}

#[allow(clippy::too_many_arguments)]
fn run_update(
    ctx: &RepoCtx,
    id: &str,
    title: Option<&str>,
    body: Option<&str>,
    append: bool,
    priority: Option<&str>,
    labels: &[String],
    add_labels: &[String],
    remove_labels: &[String],
    category: Option<&str>,
    assignee: Option<&str>,
    blocked_by: &[String],
    remove_blocked_by: &[String],
    parent: Option<&str>,
    remove_parent: bool,
) -> Result<IssuesOutput, IssuesError> {
    let cfg = ctx.config()?;
    let pri = priority.and_then(|s| s.parse().ok());

    let body_text = match body {
        Some("-") => {
            let mut buf = String::new();
            io::Read::read_to_string(&mut io::stdin(), &mut buf)?;
            Some(buf)
        }
        Some(text) => Some(text.to_string()),
        None => None,
    };

    // --label (replace) is mutually exclusive with --add-label / --remove-label
    if !labels.is_empty() && (!add_labels.is_empty() || !remove_labels.is_empty()) {
        return Err(IssuesError::Usage(
            "--label (replace) cannot be combined with --add-label / --remove-label".to_string(),
        ));
    }

    // Require at least one field to update
    if title.is_none()
        && body_text.is_none()
        && pri.is_none()
        && labels.is_empty()
        && add_labels.is_empty()
        && remove_labels.is_empty()
        && category.is_none()
        && assignee.is_none()
        && blocked_by.is_empty()
        && remove_blocked_by.is_empty()
        && parent.is_none()
        && !remove_parent
    {
        return Err(IssuesError::Usage(
            "at least one field to update is required (--title, --body, --priority, --label, --add-label, --remove-label, --category, --assignee, --blocked-by, --remove-blocked-by, --parent, --remove-parent)".to_string(),
        ));
    }

    // If --append is set and body is provided, fetch existing description and prepend it
    let effective_body = match (append, body_text) {
        (true, Some(new_body)) => {
            let provider = cfg.issue_provider()?;
            let existing = provider
                .get(id)?
                .ok_or_else(|| IssuesError::Usage(format!("issue not found: {}", id)))?;
            // The issue body includes `# Title\n\n` prefix from Linear conversion;
            // extract only the description portion (after the first heading).
            let existing_desc = existing
                .body()
                .strip_prefix(&format!("# {}\n\n", existing.title()))
                .unwrap_or(existing.body());
            if existing_desc.is_empty() {
                Some(new_body)
            } else {
                Some(format!("{}\n\n{}", existing_desc, new_body))
            }
        }
        (_, body) => body,
    };

    // Compute the effective label set for this update.
    //
    // - If the user passed --label, use it as a replacement set.
    // - Else if they passed --add-label / --remove-label, fetch the current
    //   labels and apply the delta.
    // - Else, no label mutation.
    let mutate_labels = !add_labels.is_empty() || !remove_labels.is_empty();
    let computed_labels: Vec<String> = if !labels.is_empty() {
        labels.to_vec()
    } else if mutate_labels {
        let provider = cfg.issue_provider()?;
        let existing = provider
            .get(id)?
            .ok_or_else(|| IssuesError::Usage(format!("issue not found: {}", id)))?;
        let mut set: Vec<String> = existing.labels().to_vec();
        for add in add_labels {
            if !set.iter().any(|l| l.eq_ignore_ascii_case(add)) {
                set.push(add.clone());
            }
        }
        set.retain(|l| !remove_labels.iter().any(|r| r.eq_ignore_ascii_case(l)));
        set
    } else {
        Vec::new()
    };

    let labels_changed = !labels.is_empty() || mutate_labels;

    let has_field_updates = title.is_some()
        || effective_body.is_some()
        || pri.is_some()
        || labels_changed
        || category.is_some()
        || assignee.is_some();

    match cfg.repo.issues.provider {
        ProviderKind::Linear => {
            let linear_provider = cfg.linear_provider()?;
            if has_field_updates || parent.is_some() || remove_parent {
                linear_provider.update_issue(
                    id,
                    title,
                    effective_body.as_deref(),
                    pri.as_ref(),
                    &computed_labels,
                    category,
                    assignee,
                    parent,
                    remove_parent,
                )?;
            }
            for blocker in blocked_by {
                linear_provider.add_blocked_by(id, blocker)?;
            }
            for blocker in remove_blocked_by {
                linear_provider.remove_blocked_by(id, blocker)?;
            }
        }
    }

    Ok(IssuesOutput::Updated(id.to_string()))
}

fn run_status_update(
    ctx: &RepoCtx,
    id: &str,
    new_status: &str,
) -> Result<IssuesOutput, IssuesError> {
    let cfg = ctx.config()?;

    let status: IssueStatus = new_status
        .parse()
        .map_err(|_| IssuesError::Usage(format!("unknown status: {}", new_status)))?;

    match cfg.repo.issues.provider {
        ProviderKind::Linear => {
            let linear_provider = cfg.linear_provider()?;
            linear_provider.update_status(id, &status)?;
        }
    }

    Ok(IssuesOutput::StatusUpdated(
        id.to_string(),
        status.to_string(),
    ))
}

fn run_complete(ctx: &RepoCtx, id: &str, delete: bool) -> Result<IssuesOutput, IssuesError> {
    let cfg = ctx.config()?;

    match cfg.repo.issues.provider {
        ProviderKind::Linear => {
            let linear_provider = cfg.linear_provider()?;
            linear_provider.update_status(id, &IssueStatus::Complete)?;
        }
    }

    Ok(IssuesOutput::Completed(id.to_string(), delete))
}

fn run_stats(ctx: &RepoCtx) -> Result<IssuesOutput, IssuesError> {
    let cfg = ctx.config()?;
    let provider = cfg.issue_provider()?;

    let all_issues = provider.list(&IssueFilter::default())?;
    Ok(IssuesOutput::Stats(compute_stats(&all_issues)))
}

fn run_stats_global(ctx: &GlobalCtx) -> Result<IssuesOutput, IssuesError> {
    let mut all_issues = Vec::new();

    for cfg in &ctx.configs {
        let provider = cfg.issue_provider()?;
        all_issues.extend(provider.list(&IssueFilter::default())?);
    }

    Ok(IssuesOutput::Stats(compute_stats(&all_issues)))
}

fn compute_stats(issues: &[CoreIssue]) -> StatsData {
    let mut triage = 0usize;
    let mut backlog = 0usize;
    let mut planned = 0usize;
    let mut in_progress = 0usize;
    let mut complete = 0usize;
    let mut blocked = 0usize;

    let mut urgent = 0usize;
    let mut high = 0usize;
    let mut medium = 0usize;
    let mut low = 0usize;
    for issue in issues {
        match issue.status() {
            IssueStatus::Triage => triage += 1,
            IssueStatus::Backlog => backlog += 1,
            IssueStatus::Planned => planned += 1,
            IssueStatus::InProgress => in_progress += 1,
            IssueStatus::Complete => complete += 1,
            IssueStatus::Blocked => blocked += 1,
        }
        match &issue.priority() {
            IssuePriority::Urgent => urgent += 1,
            IssuePriority::High => high += 1,
            IssuePriority::Medium => medium += 1,
            IssuePriority::Low => low += 1,
        }
    }

    let mut by_status = vec![
        ("Triage".to_string(), triage),
        ("Backlog".to_string(), backlog),
        ("Planned".to_string(), planned),
        ("In Progress".to_string(), in_progress),
        ("Complete".to_string(), complete),
        ("Blocked".to_string(), blocked),
    ];
    by_status.retain(|(_, count)| *count > 0);

    let mut by_priority = vec![
        ("Urgent".to_string(), urgent),
        ("High".to_string(), high),
        ("Medium".to_string(), medium),
        ("Low".to_string(), low),
    ];
    by_priority.retain(|(_, count)| *count > 0);

    StatsData {
        by_status,
        by_priority,
    }
}

impl Op for Issues {
    type Error = IssuesError;
    type Output = IssuesOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        match &self.command {
            Some(IssuesCommand::Create {
                title,
                template: _,
                priority,
                category,
                label,
                body,
                parent,
                status,
            }) => run_create(
                ctx,
                title,
                priority.as_deref(),
                category.as_deref(),
                label,
                body.as_deref(),
                parent.as_deref(),
                status,
            ),
            Some(IssuesCommand::Update {
                id,
                title,
                body,
                append,
                priority,
                label,
                add_label,
                remove_label,
                category,
                assignee,
                blocked_by,
                remove_blocked_by,
                parent,
                remove_parent,
            }) => run_update(
                ctx,
                id,
                title.as_deref(),
                body.as_deref(),
                *append,
                priority.as_deref(),
                label,
                add_label,
                remove_label,
                category.as_deref(),
                assignee.as_deref(),
                blocked_by,
                remove_blocked_by,
                parent.as_deref(),
                *remove_parent,
            ),
            Some(IssuesCommand::Status { id, status }) => run_status_update(ctx, id, status),
            Some(IssuesCommand::Complete { id, delete }) => run_complete(ctx, id, *delete),
            Some(IssuesCommand::Stats) => run_stats(ctx),
            None => self.run_list(ctx),
        }
    }

    fn run_global(&self, ctx: &GlobalCtx) -> Result<Self::Output, Self::Error> {
        match &self.command {
            Some(IssuesCommand::Stats) => run_stats_global(ctx),
            Some(_) => {
                eprintln!("error: this subcommand does not support -g/--global");
                std::process::exit(1);
            }
            None => self.run_list_global(ctx),
        }
    }
}

impl fmt::Display for IssuesOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Table(issues, auto_spawn_labels) => {
                if issues.is_empty() {
                    return write!(f, "No issues found");
                }
                if ui::is_plain() {
                    for issue in issues {
                        writeln!(
                            f,
                            "{}\t{}\t{}",
                            issue.status(),
                            issue.priority(),
                            issue.title()
                        )?;
                    }
                    return Ok(());
                }
                let table = render_table(issues, auto_spawn_labels.as_deref());
                write!(f, "{table}")
            }
            Self::Detail(issue) => {
                if let Some(parent) = &issue.parent() {
                    writeln!(f, "Parent: {}", parent)?;
                    writeln!(f)?;
                }
                write!(f, "{}", issue.body())?;
                if !issue.labels().is_empty() {
                    write!(f, "\n\nLabels: {}", issue.labels().join(", "))?;
                }
                if !issue.depends_on().is_empty() {
                    let deps: Vec<&str> = issue.depends_on().iter().map(|d| d.as_ref()).collect();
                    write!(f, "\n\nBlocked by: {}", deps.join(", "))?;
                }
                Ok(())
            }
            Self::Interactive => Ok(()),
            Self::Ids(ids) => {
                for id in ids {
                    writeln!(f, "{}", id)?;
                }
                Ok(())
            }
            Self::Created(id) => {
                write!(f, "Created issue: {}", id)
            }
            Self::Updated(id) => {
                write!(f, "Updated issue: {}", id)
            }
            Self::StatusUpdated(id, status) => {
                write!(f, "Updated {} -> {}", id, status)
            }
            Self::Completed(id, deleted) => {
                if *deleted {
                    write!(f, "Completed and deleted: {}", id)
                } else {
                    write!(f, "Completed: {}", id)
                }
            }
            Self::Stats(data) => {
                write!(f, "By Status:  ")?;
                for (i, (name, count)) in data.by_status.iter().enumerate() {
                    if i > 0 {
                        write!(f, "  ")?;
                    }
                    write!(f, "{}: {}", name, count)?;
                }
                writeln!(f)?;
                write!(f, "By Priority:")?;
                for (name, count) in &data.by_priority {
                    write!(f, "  {}: {}", name, count)?;
                }
                Ok(())
            }
        }
    }
}

fn render_table(issues: &[CoreIssue], auto_spawn_labels: Option<&[String]>) -> comfy_table::Table {
    let mut table = ui::new_table(&["STATUS", "AUTO", "PRI", "ISSUE"]);

    for issue in issues {
        let (status_sym, status_color) = match issue.status() {
            IssueStatus::Triage => ("[?]", Color::Magenta),
            IssueStatus::Backlog => ("[.]", Color::DarkGrey),
            IssueStatus::Planned => ("[ ]", Color::White),
            IssueStatus::InProgress => ("[~]", Color::Yellow),
            IssueStatus::Complete => ("[x]", Color::Green),
            IssueStatus::Blocked => ("[!]", Color::Red),
        };

        let (pri_text, pri_color) = match &issue.priority() {
            IssuePriority::Urgent => ("Urgent", Color::Red),
            IssuePriority::High => ("High", Color::Yellow),
            IssuePriority::Medium => ("Med", Color::White),
            IssuePriority::Low => ("Low", Color::DarkGrey),
        };

        let auto_indicator = match auto_spawn_labels {
            Some(labels) if issue.auto(labels) => "✓",
            _ => "",
        };

        let title = if issue.children().is_empty() {
            issue.title().to_string()
        } else {
            format!("{} ({} tickets)", issue.title(), issue.children().len())
        };

        table.add_row(vec![
            Cell::new(status_sym).fg(status_color),
            Cell::new(auto_indicator).fg(Color::Green),
            Cell::new(pri_text).fg(pri_color),
            Cell::new(&title).fg(Color::Cyan),
        ]);
    }

    table
}

/// Interactive browse mode using alternate screen.
fn run_interactive(
    issues: &[CoreIssue],
    auto_spawn_labels: Option<&[String]>,
) -> Result<(), IssuesError> {
    if issues.is_empty() {
        eprintln!("No issues found");
        return Ok(());
    }

    ui::with_alternate_screen(|w| interactive_loop(w, issues, auto_spawn_labels))
}

fn interactive_loop(
    w: &mut io::Stderr,
    issues: &[CoreIssue],
    auto_spawn_labels: Option<&[String]>,
) -> Result<(), IssuesError> {
    let mut cursor = 0usize;
    let mut scroll = 0usize;

    loop {
        let (cols, rows) = terminal::size().unwrap_or((80, 24));
        let visible = (rows as usize).saturating_sub(3); // header + footer + padding
        let max_title = (cols as usize).saturating_sub(30);

        // Keep cursor in view
        if cursor < scroll {
            scroll = cursor;
        } else if cursor >= scroll + visible {
            scroll = cursor - visible + 1;
        }

        write!(w, "\x1B[2J\x1B[H")?;
        write!(
            w,
            "\x1B[1mjig issues\x1B[0m — {} issues  \x1B[2m(j/k navigate, enter view, q quit)\x1B[0m\r\n\r\n",
            issues.len()
        )?;

        for (i, issue) in issues.iter().skip(scroll).take(visible).enumerate() {
            let idx = scroll + i;
            let marker = if idx == cursor { ">" } else { " " };

            let status_sym = match issue.status() {
                IssueStatus::Triage => "[?]",
                IssueStatus::Backlog => "[.]",
                IssueStatus::Planned => "[ ]",
                IssueStatus::InProgress => "[~]",
                IssueStatus::Complete => "[x]",
                IssueStatus::Blocked => "[!]",
            };

            let pri = issue.priority().to_string();

            let auto = match auto_spawn_labels {
                Some(labels) if issue.auto(labels) => " ✓",
                _ => "  ",
            };

            let title = ui::truncate(issue.title(), max_title);

            let highlight = if idx == cursor { "\x1B[1;36m" } else { "" };
            let reset = if idx == cursor { "\x1B[0m" } else { "" };

            write!(
                w,
                "{}{} {} {:6}{} {}{}\r\n",
                highlight, marker, status_sym, pri, auto, title, reset
            )?;
        }

        w.flush()?;

        if let Ok(Event::Key(key)) = event::read() {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                break;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char('j') | KeyCode::Down => {
                    if cursor + 1 < issues.len() {
                        cursor += 1;
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    cursor = cursor.saturating_sub(1);
                }
                KeyCode::Char('G') | KeyCode::End => {
                    cursor = issues.len().saturating_sub(1);
                }
                KeyCode::Char('g') | KeyCode::Home => {
                    cursor = 0;
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    view_issue(&issues[cursor], w)?;
                }
                _ => {}
            }
        }
    }

    Ok(())
}

/// Full-screen pager view for a single issue. Scroll with j/k, q/Esc to return.
fn view_issue(issue: &CoreIssue, w: &mut impl Write) -> Result<(), IssuesError> {
    let lines: Vec<&str> = issue.body().lines().collect();
    let mut scroll = 0usize;

    loop {
        let (_, rows) = terminal::size().unwrap_or((80, 24));
        let visible = (rows as usize).saturating_sub(2); // reserve header + footer

        write!(w, "\x1B[2J\x1B[H")?;
        write!(
            w,
            "\x1B[1m{}\x1B[0m  \x1B[2m{} | {}\x1B[0m\r\n",
            issue.title(),
            issue.status(),
            issue.priority(),
        )?;
        if let Some(parent) = &issue.parent() {
            write!(w, "\x1B[2mParent: {}\x1B[0m\r\n", parent)?;
        }

        for line in lines.iter().skip(scroll).take(visible) {
            write!(w, "{}\r\n", line)?;
        }

        // Footer
        let total = lines.len();
        let pct = if total == 0 {
            100
        } else {
            ((scroll + visible).min(total) * 100) / total
        };
        write!(w, "\x1B[2m— {}% (j/k scroll, q back) —\x1B[0m", pct)?;
        w.flush()?;

        if let Ok(Event::Key(key)) = event::read() {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                break;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char('j') | KeyCode::Down => {
                    if scroll + visible < lines.len() {
                        scroll += 1;
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    scroll = scroll.saturating_sub(1);
                }
                KeyCode::Char('d') => {
                    scroll = (scroll + visible / 2).min(lines.len().saturating_sub(visible));
                }
                KeyCode::Char('u') => {
                    scroll = scroll.saturating_sub(visible / 2);
                }
                KeyCode::Char(' ') | KeyCode::PageDown => {
                    scroll = (scroll + visible).min(lines.len().saturating_sub(visible));
                }
                KeyCode::PageUp => {
                    scroll = scroll.saturating_sub(visible);
                }
                KeyCode::Home | KeyCode::Char('g') => {
                    scroll = 0;
                }
                KeyCode::End => {
                    scroll = lines.len().saturating_sub(visible);
                }
                _ => {}
            }
        }
    }

    Ok(())
}
