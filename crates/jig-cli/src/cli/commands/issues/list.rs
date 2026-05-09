use std::io::{self, Write};

use clap::Args;
use comfy_table::{Cell, Color};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal;

use jig_core::issues::{self, Issue as CoreIssue, IssueFilter, IssuePriority, IssueStatus};

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::Context;

use super::{IssuesError, IssuesOutput};

/// List and browse issues
#[derive(Args, Debug, Clone)]
pub struct List {
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

    /// Operate on all tracked repos
    #[arg(short = 'g', long)]
    pub global: bool,
}

impl List {
    fn filter(&self) -> IssueFilter {
        IssueFilter {
            status: self.status.as_deref().and_then(|s| s.parse().ok()),
            priority: self.priority.as_deref().and_then(|s| s.parse().ok()),
            labels: self.label.clone(),
        }
    }

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
                .filter(|i| !provider.may_spawn(i.id()))
                .collect()
        } else if self.unblocked {
            issues
                .into_iter()
                .filter(|i| provider.may_spawn(i.id()))
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

    fn run_list(&self, cfg: &Context) -> Result<IssuesOutput, IssuesError> {
        let repo = cfg.repo()?;
        let filter = self.filter();
        let provider = repo.issue_provider(&cfg.config)?;

        if let Some(ref id) = self.id {
            let issue = provider
                .get(id)?
                .ok_or_else(|| IssuesError::Usage(format!("issue not found: {}", id)))?;
            return Ok(IssuesOutput::Detail(Box::new(issue)));
        }

        let spawn_labels = repo.repo.issues.auto_spawn_labels.clone();
        let all_issues = if self.auto {
            let labels = spawn_labels.as_deref().unwrap_or(&[]);
            let mut spawnable = provider.list(&IssueFilter {
                status: Some(IssueStatus::Planned),
                ..Default::default()
            })?;
            spawnable.retain(|i| labels.is_empty() || i.auto(labels));
            spawnable.retain(|i| provider.may_spawn(i.id()));
            filter.apply(spawnable)
        } else {
            provider.list(&filter)?
        };
        let all_issues = self.exclude_completed(all_issues);
        let all_issues = self.apply_dep_filter(all_issues, &provider);
        self.finish(all_issues, spawn_labels)
    }

    fn run_list_global(&self, cfg: &Context) -> Result<IssuesOutput, IssuesError> {
        let filter = self.filter();

        let mut all_issues = Vec::new();
        for repo in &cfg.repos {
            let provider = repo.issue_provider(&cfg.config)?;

            if let Some(ref id) = self.id {
                if let Some(issue) = provider.get(id)? {
                    return Ok(IssuesOutput::Detail(Box::new(issue)));
                }
                continue;
            }

            let spawn_labels = repo.repo.issues.auto_spawn_labels.clone();
            let repo_issues = if self.auto {
                let labels = spawn_labels.as_deref().unwrap_or(&[]);
                let mut spawnable = provider.list(&IssueFilter {
                    status: Some(IssueStatus::Planned),
                    ..Default::default()
                })?;
                spawnable.retain(|i| labels.is_empty() || i.auto(labels));
                spawnable.retain(|i| provider.may_spawn(i.id()));
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

impl Op for List {
    type Error = IssuesError;
    type Output = IssuesOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        if self.global {
            let cfg = Context::from_global()?;
            self.run_list_global(&cfg)
        } else {
            let cfg = Context::from_cwd()?;
            self.run_list(&cfg)
        }
    }
}

pub fn render_table(issues: &[CoreIssue], auto_spawn_labels: Option<&[String]>) -> comfy_table::Table {
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
        let visible = (rows as usize).saturating_sub(3);
        let max_title = (cols as usize).saturating_sub(30);

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

fn view_issue(issue: &CoreIssue, w: &mut impl Write) -> Result<(), IssuesError> {
    let lines: Vec<&str> = issue.body().lines().collect();
    let mut scroll = 0usize;

    loop {
        let (_, rows) = terminal::size().unwrap_or((80, 24));
        let visible = (rows as usize).saturating_sub(2);

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
