//! Issues command — discover and browse file-based issues.

use std::fmt;
use std::io::{self, Write};

use clap::Args;
use comfy_table::{presets, Attribute, Cell, CellAlignment, Color, ContentArrangement, Table};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal;

use jig_core::config::JigToml;
use jig_core::issues::{
    FileProvider, Issue as CoreIssue, IssueFilter, IssuePriority, IssueProvider, IssueStatus,
};

use crate::op::{Op, OpContext};

/// Discover and browse issues
#[derive(Args, Debug, Clone)]
pub struct Issues {
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

    /// Interactive expand/collapse mode
    #[arg(short, long)]
    pub interactive: bool,

    /// Print issue IDs only (one per line, for scripting)
    #[arg(long)]
    pub ids: bool,
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
    Table(Vec<CoreIssue>),
    Detail(CoreIssue),
    Interactive,
    Ids(Vec<String>),
}

impl Op for Issues {
    type Error = IssuesError;
    type Output = IssuesOutput;

    fn execute(&self, ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        let repo = ctx.repo()?;
        let jig_toml = JigToml::load(&repo.repo_root)?.unwrap_or_default();
        let issues_dir = repo.repo_root.join(&jig_toml.issues.directory);

        let provider = FileProvider::new(&issues_dir);

        // Single issue detail
        if let Some(ref id) = self.id {
            return match provider.get(id)? {
                Some(issue) => Ok(IssuesOutput::Detail(issue)),
                None => Err(IssuesError::Usage(format!("issue not found: {}", id))),
            };
        }

        // Build filter
        let filter = IssueFilter {
            status: self.status.as_deref().and_then(IssueStatus::from_str_loose),
            priority: self
                .priority
                .as_deref()
                .and_then(IssuePriority::from_str_loose),
            category: self.category.clone(),
        };

        let issues = provider.list(&filter)?;

        if self.ids {
            let ids: Vec<String> = issues.into_iter().map(|i| i.id).collect();
            return Ok(IssuesOutput::Ids(ids));
        }

        if self.interactive {
            run_interactive(&issues)?;
            return Ok(IssuesOutput::Interactive);
        }

        Ok(IssuesOutput::Table(issues))
    }
}

impl fmt::Display for IssuesOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Table(issues) => {
                if issues.is_empty() {
                    return write!(f, "No issues found");
                }
                let table = render_table(issues);
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
        }
    }
}

fn render_table(issues: &[CoreIssue]) -> Table {
    let mut table = Table::new();
    table
        .load_preset(presets::NOTHING)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("STATUS").add_attribute(Attribute::Bold),
            Cell::new("PRI").add_attribute(Attribute::Bold),
            Cell::new("CATEGORY")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Left),
            Cell::new("ISSUE").add_attribute(Attribute::Bold),
        ]);

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

        let category = issue.category.as_deref().unwrap_or("-");

        let title = if issue.children.is_empty() {
            issue.title.clone()
        } else {
            format!("{} ({} tickets)", issue.title, issue.children.len())
        };

        table.add_row(vec![
            Cell::new(status_sym).fg(status_color),
            Cell::new(pri_text).fg(pri_color),
            Cell::new(category),
            Cell::new(&title).fg(Color::Cyan),
        ]);
    }

    table
}

/// Interactive expand/collapse mode using crossterm.
fn run_interactive(issues: &[CoreIssue]) -> Result<(), IssuesError> {
    if issues.is_empty() {
        eprintln!("No issues found");
        return Ok(());
    }

    terminal::enable_raw_mode().map_err(io::Error::other)?;
    let result = interactive_loop(issues);
    terminal::disable_raw_mode().map_err(io::Error::other)?;

    result
}

fn interactive_loop(issues: &[CoreIssue]) -> Result<(), IssuesError> {
    let mut cursor = 0usize;
    let mut stderr = io::stderr();

    loop {
        // Clear screen and move to top
        write!(stderr, "\x1B[2J\x1B[H")?;
        write!(
            stderr,
            "\x1B[1mjig issues\x1B[0m — {} issues  \x1B[2m(j/k navigate, enter view, q quit)\x1B[0m\r\n\r\n",
            issues.len()
        )?;

        for (i, issue) in issues.iter().enumerate() {
            let marker = if i == cursor { ">" } else { " " };

            let status_sym = match issue.status {
                IssueStatus::Planned => "[ ]",
                IssueStatus::InProgress => "[~]",
                IssueStatus::Complete => "[x]",
                IssueStatus::Blocked => "[!]",
            };

            let pri = issue.priority.as_ref().map(|p| p.as_str()).unwrap_or("-");

            let highlight = if i == cursor { "\x1B[1m" } else { "" };
            let reset = if i == cursor { "\x1B[0m" } else { "" };

            write!(
                stderr,
                "{}{} {} {:6} {}{}\r\n",
                highlight, marker, status_sym, pri, issue.title, reset
            )?;
        }

        stderr.flush()?;

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
                KeyCode::Enter | KeyCode::Char(' ') => {
                    view_issue(&issues[cursor], &mut stderr)?;
                }
                _ => {}
            }
        }
    }

    // Clear screen on exit
    write!(stderr, "\x1B[2J\x1B[H")?;
    stderr.flush()?;
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
