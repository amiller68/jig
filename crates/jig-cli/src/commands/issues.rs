//! Issues command — discover, browse, and create issues.

use std::fmt;
use std::io::{self, Read as _, Write};

use clap::{Args, Subcommand};
use comfy_table::{Cell, Color};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal;

use jig_core::config::JigToml;
use jig_core::global::GlobalConfig;
use jig_core::issues::{
    self, CreateIssueInput, Issue as CoreIssue, IssueFilter, IssuePriority, IssueStatus,
};
use jig_core::worktree::Worktree;
use jig_core::{config, terminal as jig_terminal};

use crate::op::{GlobalCtx, Op, RepoCtx};
use crate::ui;

/// Discover, browse, and create issues
#[derive(Args, Debug, Clone)]
#[command(args_conflicts_with_subcommands = true)]
pub struct Issues {
    #[command(subcommand)]
    pub action: Option<IssuesAction>,

    #[command(flatten)]
    pub list: IssuesList,
}

#[derive(Subcommand, Debug, Clone)]
pub enum IssuesAction {
    /// Create a new issue via the configured provider
    Create(IssuesCreate),
}

/// Browse and filter issues (default)
#[derive(Args, Debug, Clone)]
pub struct IssuesList {
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

/// Create a new issue
#[derive(Args, Debug, Clone)]
pub struct IssuesCreate {
    /// Issue title
    #[arg(short, long)]
    pub title: String,

    /// Issue body/description (reads from stdin if omitted)
    #[arg(short, long)]
    pub body: Option<String>,

    /// Spawn a worker for the new issue after creation
    #[arg(long)]
    pub spawn: bool,

    /// Auto-start Claude with full prompt (used with --spawn)
    #[arg(long)]
    pub auto: bool,
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
    Table(Vec<CoreIssue>, Vec<String>),
    Detail(CoreIssue),
    Interactive,
    Ids(Vec<String>),
    Created(String),
}

impl IssuesList {
    fn filter(&self) -> IssueFilter {
        IssueFilter {
            status: self.status.as_deref().and_then(IssueStatus::from_str_loose),
            priority: self
                .priority
                .as_deref()
                .and_then(IssuePriority::from_str_loose),
            category: self.category.clone(),
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
            .filter(|i| i.status != IssueStatus::Complete)
            .collect()
    }

    fn apply_dep_filter(
        &self,
        issues: Vec<CoreIssue>,
        provider: &dyn issues::IssueProvider,
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
        spawn_labels: Vec<String>,
    ) -> Result<IssuesOutput, IssuesError> {
        if self.ids {
            let ids: Vec<String> = all_issues.into_iter().map(|i| i.id).collect();
            return Ok(IssuesOutput::Ids(ids));
        }

        if self.interactive {
            run_interactive(&all_issues, &spawn_labels)?;
            return Ok(IssuesOutput::Interactive);
        }

        Ok(IssuesOutput::Table(all_issues, spawn_labels))
    }

    fn run_list(&self, ctx: &RepoCtx) -> Result<IssuesOutput, IssuesError> {
        let repo = ctx.repo()?;
        let global_config = GlobalConfig::load().unwrap_or_default();
        let filter = self.filter();

        let jig_toml = JigToml::load(&repo.repo_root)?.unwrap_or_default();
        let provider = issues::make_provider(&repo.repo_root, &jig_toml, &global_config)?;

        if let Some(ref id) = self.id {
            let issue = provider
                .get(id)?
                .ok_or_else(|| IssuesError::Usage(format!("issue not found: {}", id)))?;
            return Ok(IssuesOutput::Detail(issue));
        }

        let all_issues = if self.auto {
            let spawnable = provider.list_spawnable(&jig_toml.issues.spawn_labels)?;
            // Apply additional filters on top of spawnable results
            filter.apply(spawnable)
        } else {
            provider.list(&filter)?
        };
        let all_issues = self.exclude_completed(all_issues);
        let all_issues = self.apply_dep_filter(all_issues, provider.as_ref());
        self.finish(all_issues, jig_toml.issues.spawn_labels.clone())
    }

    fn run_list_global(&self, ctx: &GlobalCtx) -> Result<IssuesOutput, IssuesError> {
        let global_config = GlobalConfig::load().unwrap_or_default();
        let filter = self.filter();

        let mut all_issues = Vec::new();
        for repo in &ctx.repos {
            let jig_toml = JigToml::load(&repo.repo_root)?.unwrap_or_default();
            let provider = issues::make_provider(&repo.repo_root, &jig_toml, &global_config)?;

            if let Some(ref id) = self.id {
                if let Some(issue) = provider.get(id)? {
                    return Ok(IssuesOutput::Detail(issue));
                }
                continue;
            }

            let repo_issues = if self.auto {
                let spawnable = provider.list_spawnable(&jig_toml.issues.spawn_labels)?;
                filter.apply(spawnable)
            } else {
                provider.list(&filter)?
            };
            let repo_issues = self.apply_dep_filter(repo_issues, provider.as_ref());
            all_issues.extend(repo_issues);
        }

        if let Some(id) = &self.id {
            return Err(IssuesError::Usage(format!("issue not found: {}", id)));
        }

        let all_issues = self.exclude_completed(all_issues);
        self.finish(all_issues, Vec::new())
    }
}

impl IssuesCreate {
    fn run_create(&self, ctx: &RepoCtx) -> Result<IssuesOutput, IssuesError> {
        let repo = ctx.repo()?;
        let global_config = GlobalConfig::load().unwrap_or_default();
        let jig_toml = JigToml::load(&repo.repo_root)?.unwrap_or_default();
        let provider = issues::make_provider(&repo.repo_root, &jig_toml, &global_config)?;

        // Read body from --body or stdin
        let body = match &self.body {
            Some(b) => b.clone(),
            None => {
                let mut buf = String::new();
                io::stdin().read_to_string(&mut buf)?;
                buf
            }
        };

        // Build labels: always include spawn_labels (e.g. "jig-auto")
        let labels = jig_toml.issues.spawn_labels.clone();

        let input = CreateIssueInput {
            title: self.title.clone(),
            body,
            labels,
        };

        let created = provider.create(&input)?;
        let issue_id = created.id.clone();

        ui::success(&format!(
            "Created issue {} ({})",
            ui::highlight(&created.id),
            created.url,
        ));

        // Optionally spawn a worker for the new issue
        if self.spawn {
            if !jig_terminal::command_exists("tmux") {
                return Err(jig_core::Error::MissingDependency("tmux".to_string()).into());
            }
            if !jig_terminal::command_exists("claude") {
                return Err(jig_core::Error::MissingDependency("claude".to_string()).into());
            }

            // Derive worktree name from the issue ID (e.g. "ENG-456" -> "eng-456")
            let wt_name = issue_id.to_lowercase();
            let worktree_path = repo.worktrees_dir.join(&wt_name);

            let wt = if !worktree_path.exists() {
                let base = &repo.base_branch;
                let copy_files = config::get_copy_files(&repo.repo_root)?;
                let on_create_hook = config::get_on_create_hook(&repo.repo_root)?;

                Worktree::create(
                    &repo.repo_root,
                    &repo.worktrees_dir,
                    &repo.git_common_dir,
                    &wt_name,
                    None,
                    base,
                    on_create_hook.as_deref(),
                    &copy_files,
                    false,
                )?
            } else {
                Worktree::open(&repo.repo_root, &repo.worktrees_dir, &wt_name)?
            };

            // Fetch the issue body for context
            let issue_context = provider.get(&issue_id)?.map(|i| i.body);

            let use_auto = if self.auto { true } else { jig_toml.spawn.auto };

            wt.register(issue_context.as_deref(), Some(&issue_id))?;
            wt.launch(issue_context.as_deref(), use_auto)?;

            ui::success(&format!(
                "Spawned worker '{}' for {}",
                ui::highlight(&wt_name),
                ui::highlight(&issue_id),
            ));
        }

        // Print issue ID to stdout for scripting
        Ok(IssuesOutput::Created(issue_id))
    }
}

impl Op for Issues {
    type Error = IssuesError;
    type Output = IssuesOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        match &self.action {
            Some(IssuesAction::Create(create)) => create.run_create(ctx),
            None => self.list.run_list(ctx),
        }
    }

    fn run_global(&self, ctx: &GlobalCtx) -> Result<Self::Output, Self::Error> {
        match &self.action {
            Some(IssuesAction::Create(_)) => Err(IssuesError::Usage(
                "issues create does not support --global".to_string(),
            )),
            None => self.list.run_list_global(ctx),
        }
    }
}

impl fmt::Display for IssuesOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Table(issues, spawn_labels) => {
                if issues.is_empty() {
                    return write!(f, "No issues found");
                }
                if ui::is_plain() {
                    for issue in issues {
                        let status = issue.status.as_str();
                        let pri = issue.priority.as_ref().map(|p| p.as_str()).unwrap_or("-");
                        let cat = issue.category.as_deref().unwrap_or("-");
                        writeln!(f, "{}\t{}\t{}\t{}", status, pri, cat, issue.title)?;
                    }
                    return Ok(());
                }
                let table = render_table(issues, spawn_labels);
                write!(f, "{table}")
            }
            Self::Detail(issue) => {
                write!(f, "{}", issue.body)
            }
            Self::Interactive => Ok(()),
            Self::Ids(ids) => {
                for id in ids {
                    writeln!(f, "{}", id)?;
                }
                Ok(())
            }
            Self::Created(id) => write!(f, "{}", id),
        }
    }
}

fn render_table(issues: &[CoreIssue], spawn_labels: &[String]) -> comfy_table::Table {
    let mut table = ui::new_table(&["STATUS", "AUTO", "PRI", "CATEGORY", "ISSUE"]);

    for issue in issues {
        let (status_sym, status_color) = match issue.status {
            IssueStatus::Planned => ("[ ]", Color::White),
            IssueStatus::InProgress => ("[~]", Color::Yellow),
            IssueStatus::Complete => ("[x]", Color::Green),
            IssueStatus::Blocked => ("[!]", Color::Red),
        };

        let (pri_text, pri_color) = match &issue.priority {
            Some(IssuePriority::Urgent) => ("Urgent", Color::Red),
            Some(IssuePriority::High) => ("High", Color::Yellow),
            Some(IssuePriority::Medium) => ("Med", Color::White),
            Some(IssuePriority::Low) => ("Low", Color::DarkGrey),
            None => ("-", Color::DarkGrey),
        };

        let auto_indicator = if issue.auto(spawn_labels) { "✓" } else { "" };

        let category = issue.category.as_deref().unwrap_or("-");

        let title = if issue.children.is_empty() {
            issue.title.clone()
        } else {
            format!("{} ({} tickets)", issue.title, issue.children.len())
        };

        table.add_row(vec![
            Cell::new(status_sym).fg(status_color),
            Cell::new(auto_indicator).fg(Color::Green),
            Cell::new(pri_text).fg(pri_color),
            Cell::new(category),
            Cell::new(&title).fg(Color::Cyan),
        ]);
    }

    table
}

/// Interactive browse mode using alternate screen.
fn run_interactive(issues: &[CoreIssue], spawn_labels: &[String]) -> Result<(), IssuesError> {
    if issues.is_empty() {
        eprintln!("No issues found");
        return Ok(());
    }

    ui::with_alternate_screen(|w| interactive_loop(w, issues, spawn_labels))
}

fn interactive_loop(
    w: &mut io::Stderr,
    issues: &[CoreIssue],
    spawn_labels: &[String],
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

            let status_sym = match issue.status {
                IssueStatus::Planned => "[ ]",
                IssueStatus::InProgress => "[~]",
                IssueStatus::Complete => "[x]",
                IssueStatus::Blocked => "[!]",
            };

            let pri = issue.priority.as_ref().map(|p| p.as_str()).unwrap_or("-");

            let auto = if issue.auto(spawn_labels) {
                " ✓"
            } else {
                "  "
            };

            let title = ui::truncate(&issue.title, max_title);

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
    let lines: Vec<&str> = issue.body.lines().collect();
    let mut scroll = 0usize;

    loop {
        let (_, rows) = terminal::size().unwrap_or((80, 24));
        let visible = (rows as usize).saturating_sub(2); // reserve header + footer

        write!(w, "\x1B[2J\x1B[H")?;
        write!(
            w,
            "\x1B[1m{}\x1B[0m  \x1B[2m{} | {}\x1B[0m\r\n",
            issue.title,
            issue.status.as_str(),
            issue.priority.as_ref().map(|p| p.as_str()).unwrap_or("-"),
        )?;

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
