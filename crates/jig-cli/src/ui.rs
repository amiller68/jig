//! Shared rendering utilities for CLI output.
//!
//! Provides consistent status symbols, color helpers, truncation, table builders,
//! and a global plain-mode flag for scriptable output.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

use colored::Colorize;
use comfy_table::{presets, Attribute, Cell, CellAlignment, Color, ContentArrangement, Table};
use crossterm::terminal;

use jig_core::daemon::{WorkerDisplayInfo, WorkerTickInfo};
use jig_core::spawn::TaskStatus;
use jig_core::worker::WorkerStatus;

// ---------------------------------------------------------------------------
// Plain mode
// ---------------------------------------------------------------------------

static PLAIN_MODE: AtomicBool = AtomicBool::new(false);

/// Enable or disable plain (no-color, no-decoration) output.
pub fn set_plain(enabled: bool) {
    PLAIN_MODE.store(enabled, Ordering::Relaxed);
}

/// Returns true when `--plain` was passed.
pub fn is_plain() -> bool {
    PLAIN_MODE.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Status symbols
// ---------------------------------------------------------------------------

/// Success symbol (green check mark).
pub const SYM_OK: &str = "✓";
/// Progress / action-in-flight symbol.
pub const SYM_ARROW: &str = "→";
/// Failure symbol.
pub const SYM_FAIL: &str = "✗";
/// Warning symbol.
pub const SYM_WARN: &str = "!";

// ---------------------------------------------------------------------------
// Formatted status helpers — print to stderr
// ---------------------------------------------------------------------------

/// Print a success line to stderr: `✓ message`
pub fn success(msg: &str) {
    if is_plain() {
        eprintln!("{}", msg);
    } else {
        eprintln!("{} {}", SYM_OK.green(), msg);
    }
}

/// Print a progress line to stderr: `→ message`
pub fn progress(msg: &str) {
    if is_plain() {
        eprintln!("{}", msg);
    } else {
        eprintln!("{} {}", SYM_ARROW.cyan(), msg);
    }
}

/// Print a failure line to stderr: `✗ message`
#[allow(dead_code)]
pub fn failure(msg: &str) {
    if is_plain() {
        eprintln!("{}", msg);
    } else {
        eprintln!("{} {}", SYM_FAIL.red(), msg);
    }
}

/// Print a warning line to stderr: `! message`
pub fn warning(msg: &str) {
    if is_plain() {
        eprintln!("{}", msg);
    } else {
        eprintln!("{} {}", SYM_WARN.yellow(), msg);
    }
}

/// Print an indented detail line to stderr: `  → detail`
pub fn detail(msg: &str) {
    if is_plain() {
        eprintln!("  {}", msg);
    } else {
        eprintln!("  {} {}", SYM_ARROW.dimmed(), msg);
    }
}

/// Print a section header to stderr.
pub fn header(msg: &str) {
    if is_plain() {
        eprintln!("{}", msg);
    } else {
        eprintln!("{}", msg.bold());
    }
}

// ---------------------------------------------------------------------------
// Color helpers for Display impls (return colored strings)
// ---------------------------------------------------------------------------

/// Highlight a name/value (cyan).
pub fn highlight(s: &str) -> String {
    if is_plain() {
        s.to_string()
    } else {
        s.cyan().to_string()
    }
}

/// Bold text.
pub fn bold(s: &str) -> String {
    if is_plain() {
        s.to_string()
    } else {
        s.bold().to_string()
    }
}

/// Yellow text for warnings (inline, no prefix).
pub fn warn_text(s: &str) -> String {
    if is_plain() {
        s.to_string()
    } else {
        s.yellow().to_string()
    }
}

/// Dimmed text for secondary info.
pub fn dim(s: &str) -> String {
    if is_plain() {
        s.to_string()
    } else {
        s.dimmed().to_string()
    }
}

// ---------------------------------------------------------------------------
// Table helpers
// ---------------------------------------------------------------------------

/// Create a new table with the standard preset (no borders) and dynamic arrangement.
pub fn new_table(headers: &[&str]) -> Table {
    let mut table = Table::new();
    table
        .load_preset(presets::NOTHING)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(
            headers
                .iter()
                .map(|h| Cell::new(*h).add_attribute(Attribute::Bold))
                .collect::<Vec<_>>(),
        );
    table
}

// ---------------------------------------------------------------------------
// Truncation
// ---------------------------------------------------------------------------

/// Maximum display width for worker names.
pub const NAME_MAX: usize = 36;

/// Truncate a string to `max` characters, appending ellipsis if needed (UTF-8 safe).
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let end = s
            .char_indices()
            .nth(max - 1)
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        format!("{}…", &s[..end])
    }
}

// ---------------------------------------------------------------------------
// Error display
// ---------------------------------------------------------------------------

/// Print a formatted error chain to stderr.
pub fn print_error(e: &dyn std::error::Error) {
    if is_plain() {
        eprintln!("error: {e}");
    } else {
        eprintln!("{} {e}", "error:".red().bold());
    }
    let mut source = e.source();
    while let Some(cause) = source {
        if is_plain() {
            eprintln!("  caused by: {cause}");
        } else {
            eprintln!("  {} {cause}", "caused by:".yellow());
        }
        source = cause.source();
    }
}

/// Single source of truth: WorkerStatus → comfy_table color.
pub fn worker_state_color(status: &WorkerStatus) -> Color {
    match status {
        WorkerStatus::Initializing => Color::Blue,
        WorkerStatus::Running => Color::Green,
        WorkerStatus::Spawned => Color::Blue,
        WorkerStatus::Idle => Color::Yellow,
        WorkerStatus::WaitingInput => Color::Magenta,
        WorkerStatus::Stalled => Color::Red,
        WorkerStatus::WaitingReview => Color::Cyan,
        WorkerStatus::Approved => Color::Green,
        WorkerStatus::Merged => Color::Green,
        WorkerStatus::Failed => Color::Red,
        WorkerStatus::Archived => Color::DarkGrey,
    }
}

/// Single source of truth: WorkerStatus → display label.
pub fn worker_state_str(status: &WorkerStatus) -> &'static str {
    match status {
        WorkerStatus::Initializing => "initializing",
        WorkerStatus::Running => "running",
        WorkerStatus::Spawned => "spawned",
        WorkerStatus::Idle => "idle",
        WorkerStatus::WaitingInput => "waiting",
        WorkerStatus::Stalled => "stalled",
        WorkerStatus::WaitingReview => "review",
        WorkerStatus::Approved => "approved",
        WorkerStatus::Merged => "merged",
        WorkerStatus::Failed => "failed",
        WorkerStatus::Archived => "archived",
    }
}

/// Format PR health status for display.
pub fn format_health(info: &WorkerTickInfo) -> (String, Color) {
    if !info.has_pr {
        return ("-".to_string(), Color::DarkGrey);
    }

    if let Some(err) = &info.pr_error {
        tracing::debug!(error = %err, "PR health error");
        return ("?".to_string(), Color::Yellow);
    }

    if info.pr_checks.is_empty() {
        return ("-".to_string(), Color::DarkGrey);
    }

    let problems: Vec<&str> = info
        .pr_checks
        .iter()
        .filter(|(_, has_problem)| *has_problem)
        .map(|(name, _)| name.as_str())
        .collect();

    if problems.is_empty() {
        ("ok".to_string(), Color::Green)
    } else {
        (problems.join(" "), Color::Red)
    }
}

/// Build a row of cells for a single worker.
fn worker_row(w: &WorkerDisplayInfo) -> Vec<Cell> {
    let tmux_indicator = match w.tmux_status {
        TaskStatus::Running => "●",
        TaskStatus::Exited => "○",
        TaskStatus::NoSession | TaskStatus::NoWindow => "✗",
    };
    let tmux_color = match w.tmux_status {
        TaskStatus::Running => Color::Green,
        TaskStatus::Exited => Color::Yellow,
        TaskStatus::NoSession | TaskStatus::NoWindow => Color::DarkGrey,
    };

    let (state_text, state_color) = match w.worker_status {
        Some(ref status) if *status == WorkerStatus::WaitingReview && w.is_draft => {
            ("draft", Color::Blue)
        }
        Some(ref status) => (worker_state_str(status), worker_state_color(status)),
        None => ("-", Color::DarkGrey),
    };

    let (nudge_text, nudge_color) = if w.nudge_count == 0 {
        ("-".to_string(), Color::DarkGrey)
    } else if w.nudge_count >= w.max_nudges {
        (format!("{}/{}", w.nudge_count, w.max_nudges), Color::Red)
    } else {
        (format!("{}/{}", w.nudge_count, w.max_nudges), Color::Yellow)
    };

    let dirty_marker = if w.is_dirty { "*" } else { "" };
    let commits = if w.commits_ahead > 0 || w.is_dirty {
        format!("{}{}", w.commits_ahead, dirty_marker)
    } else {
        "-".to_string()
    };
    let commit_color = if w.is_dirty {
        Color::Yellow
    } else if w.commits_ahead > 0 {
        Color::White
    } else {
        Color::DarkGrey
    };

    let pr = w
        .pr_url
        .as_ref()
        .map(|url| {
            url.rsplit('/')
                .next()
                .map(|n| format!("#{}", n))
                .unwrap_or_else(|| "yes".to_string())
        })
        .unwrap_or_else(|| "-".to_string());
    let pr_color = if pr == "-" {
        Color::DarkGrey
    } else {
        Color::Cyan
    };

    let issue = w
        .issue_ref
        .as_deref()
        .map(|id| truncate(id.rsplit('/').next().unwrap_or(id), 16))
        .unwrap_or_else(|| "-".to_string());
    let issue_color = if issue == "-" {
        Color::DarkGrey
    } else {
        Color::White
    };

    let (health_text, health_color) = format_health(&w.pr_health);

    let name = format!("{} {}", tmux_indicator, truncate(&w.name, NAME_MAX));

    vec![
        Cell::new(&name).fg(tmux_color),
        Cell::new(state_text).fg(state_color),
        Cell::new(&nudge_text)
            .fg(nudge_color)
            .set_alignment(CellAlignment::Right),
        Cell::new(&commits)
            .fg(commit_color)
            .set_alignment(CellAlignment::Right),
        Cell::new(&pr).fg(pr_color),
        Cell::new(&health_text).fg(health_color),
        Cell::new(&issue).fg(issue_color),
    ]
}

/// Standard table header cells.
fn table_header() -> Vec<Cell> {
    vec![
        Cell::new("WORKER").add_attribute(Attribute::Bold),
        Cell::new("STATE").add_attribute(Attribute::Bold),
        Cell::new("NUDGE").add_attribute(Attribute::Bold),
        Cell::new("COMMITS").add_attribute(Attribute::Bold),
        Cell::new("PR").add_attribute(Attribute::Bold),
        Cell::new("HEALTH").add_attribute(Attribute::Bold),
        Cell::new("ISSUE").add_attribute(Attribute::Bold),
    ]
}

/// Render a worker table from display info.
///
/// `borders`: true uses UTF8_BORDERS_ONLY (watch mode), false uses NOTHING (non-watch).
pub fn render_worker_table(workers: &[WorkerDisplayInfo], borders: bool) -> Table {
    let mut table = Table::new();
    let preset = if borders {
        presets::UTF8_BORDERS_ONLY
    } else {
        presets::NOTHING
    };
    table
        .load_preset(preset)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(table_header());

    for w in workers {
        table.add_row(worker_row(w));
    }

    table
}

/// Render workers grouped by repo, with bold repo headers.
///
/// Returns a formatted string with separate tables per repo.
pub fn render_worker_table_grouped(workers: &[WorkerDisplayInfo], borders: bool) -> String {
    // Collect unique repos in order of appearance
    let mut repos: Vec<String> = Vec::new();
    for w in workers {
        if !repos.contains(&w.repo) {
            repos.push(w.repo.clone());
        }
    }

    let preset = if borders {
        presets::UTF8_BORDERS_ONLY
    } else {
        presets::NOTHING
    };

    let mut sections: Vec<String> = Vec::new();

    for repo in &repos {
        let repo_workers: Vec<&WorkerDisplayInfo> =
            workers.iter().filter(|w| &w.repo == repo).collect();

        let mut table = Table::new();
        table
            .load_preset(preset)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(table_header());

        for w in &repo_workers {
            table.add_row(worker_row(w));
        }

        // Bold repo name header, then indented table
        let table_str = table.to_string();
        let indented: String = table_str
            .lines()
            .map(|line| format!("  {}", line))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("\x1B[1m{}\x1B[0m\n{}", repo, indented));
    }

    sections.join("\n\n")
}

// ---------------------------------------------------------------------------
// Alternate screen
// ---------------------------------------------------------------------------

/// Run a closure in the alternate screen with raw mode enabled.
///
/// Enters the alternate screen buffer (like `less` or `git diff`), enables
/// raw mode for keypress handling, then runs `f`. On return (or error),
/// raw mode and the alternate screen are always restored.
pub fn with_alternate_screen<F, T, E>(f: F) -> Result<T, E>
where
    F: FnOnce(&mut io::Stderr) -> Result<T, E>,
    E: From<io::Error>,
{
    let mut w = io::stderr();
    crossterm::execute!(w, terminal::EnterAlternateScreen)?;
    terminal::enable_raw_mode()?;

    let result = f(&mut w);

    let _ = terminal::disable_raw_mode();
    let _ = crossterm::execute!(w, terminal::LeaveAlternateScreen);

    result
}
