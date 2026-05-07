//! Shared rendering utilities for CLI output.
//!
//! Provides consistent status symbols, color helpers, truncation, table builders,
//! and a global plain-mode flag for scriptable output.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

use colored::Colorize;
use comfy_table::{presets, Attribute, Cell, CellAlignment, Color, ContentArrangement, Table};
use crossterm::terminal;

use crate::daemon::TriageEntry;
use crate::worker::events::{PrHealth, WorkerState};
use crate::worker::MuxStatus;
use crate::worker::WorkerStatus;

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

/// Source attribution text (green).
pub fn source(s: &str) -> String {
    if is_plain() {
        s.to_string()
    } else {
        s.green().to_string()
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
// Duration formatting
// ---------------------------------------------------------------------------

/// Format seconds as a short human-readable duration: `45s`, `3m12s`, `1h5m`.
pub fn format_duration_short(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        if s == 0 {
            format!("{}m", m)
        } else {
            format!("{}m{}s", m, s)
        }
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if m == 0 {
            format!("{}h", h)
        } else {
            format!("{}h{}m", h, m)
        }
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
        WorkerStatus::Created => Color::DarkGrey,
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
        WorkerStatus::Created => "created",
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
pub fn format_health(info: &PrHealth) -> (String, Color) {
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

    let problems = info.pr_checks.problems();
    if problems.is_empty() {
        ("ok".to_string(), Color::Green)
    } else {
        (problems.join(" "), Color::Red)
    }
}

/// Build a row of cells for a single worker.
fn worker_row(w: &WorkerState) -> Vec<Cell> {
    let mux_indicator = match w.mux_status {
        MuxStatus::Running => "●",
        MuxStatus::Exited => "○",
        MuxStatus::NotFound => "✗",
    };
    let mux_color = match w.mux_status {
        MuxStatus::Running => Color::Green,
        MuxStatus::Exited => Color::Yellow,
        MuxStatus::NotFound => Color::DarkGrey,
    };

    let (state_text, state_color) =
        if w.status == WorkerStatus::WaitingReview && w.is_draft {
            ("draft", Color::Blue)
        } else {
            (worker_state_str(&w.status), worker_state_color(&w.status))
        };

    let nudge_count = w.nudge_count();
    let (nudge_text, nudge_color) = if nudge_count == 0 {
        if let Some(cd) = w.nudge_cooldown_remaining {
            (format!("({})", format_duration_short(cd)), Color::DarkGrey)
        } else {
            ("-".to_string(), Color::DarkGrey)
        }
    } else if nudge_count >= w.max_nudges {
        (format!("{}/{}", nudge_count, w.max_nudges), Color::Red)
    } else if let Some(cd) = w.nudge_cooldown_remaining {
        (
            format!(
                "{}/{} ({})",
                nudge_count,
                w.max_nudges,
                format_duration_short(cd)
            ),
            Color::Yellow,
        )
    } else {
        (format!("{}/{}", nudge_count, w.max_nudges), Color::Yellow)
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
        .parsed_pr_url
        .as_ref()
        .map(|url| {
            url.path_segments()
                .and_then(|s| s.last())
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

    let name = format!("{} {}", mux_indicator, truncate(&w.name, NAME_MAX));

    vec![
        Cell::new(&name).fg(mux_color),
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
pub fn render_worker_table(workers: &[WorkerState], borders: bool) -> Table {
    let mut table = Table::new();
    let preset = if borders {
        presets::UTF8_BORDERS_ONLY
    } else {
        presets::NOTHING
    };
    table
        .load_preset(preset)
        .enforce_styling()
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
pub fn render_worker_table_grouped(workers: &[WorkerState], borders: bool) -> String {
    // Collect unique repo names in order of appearance
    let mut repos: Vec<String> = Vec::new();
    for w in workers {
        let name = w.repo_name();
        if !repos.contains(&name) {
            repos.push(name);
        }
    }

    let preset = if borders {
        presets::UTF8_BORDERS_ONLY
    } else {
        presets::NOTHING
    };

    let mut sections: Vec<String> = Vec::new();

    for repo in &repos {
        let repo_workers: Vec<&WorkerState> =
            workers.iter().filter(|w| w.repo_name() == *repo).collect();

        let mut table = Table::new();
        table
            .load_preset(preset)
            .enforce_styling()
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
// Triage table
// ---------------------------------------------------------------------------

/// Standard triage table header cells.
fn triage_header() -> Vec<Cell> {
    vec![
        Cell::new("ISSUE").add_attribute(Attribute::Bold),
        Cell::new("WORKER").add_attribute(Attribute::Bold),
        Cell::new("ELAPSED").add_attribute(Attribute::Bold),
        Cell::new("REPO").add_attribute(Attribute::Bold),
    ]
}

/// Build a row of cells for a single triage entry.
fn triage_row(t: &TriageEntry) -> Vec<Cell> {
    let now = chrono::Utc::now().timestamp();
    let elapsed = (now - t.spawned_at).max(0) as u64;
    vec![
        Cell::new(&t.issue_id).fg(Color::Cyan),
        Cell::new(&t.worker_name).fg(Color::White),
        Cell::new(&format_duration_short(elapsed))
            .fg(Color::White)
            .set_alignment(CellAlignment::Right),
        Cell::new(&t.repo_name).fg(Color::DarkGrey),
    ]
}

/// Render a triage table from display info.
///
/// `borders`: true uses UTF8_BORDERS_ONLY (watch mode), false uses NOTHING.
pub fn render_triage_table(triages: &[TriageEntry], borders: bool) -> Table {
    let mut table = Table::new();
    let preset = if borders {
        presets::UTF8_BORDERS_ONLY
    } else {
        presets::NOTHING
    };
    table
        .load_preset(preset)
        .enforce_styling()
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(triage_header());

    for t in triages {
        table.add_row(triage_row(t));
    }

    table
}

/// Render the full triage section with header. Returns empty string if no triages.
pub fn render_triage_section(triages: &[TriageEntry], borders: bool) -> String {
    if triages.is_empty() {
        return String::new();
    }
    let table = render_triage_table(triages, borders);
    if is_plain() {
        format!("TRIAGES\n{}", table)
    } else {
        format!("\x1B[1mTRIAGES\x1B[0m\n{}", table)
    }
}

/// Render triage section grouped by repo, with bold repo headers.
pub fn render_triage_section_grouped(triages: &[TriageEntry], borders: bool) -> String {
    if triages.is_empty() {
        return String::new();
    }

    // Collect unique repos in order of appearance
    let mut repos: Vec<String> = Vec::new();
    for t in triages {
        if !repos.contains(&t.repo_name) {
            repos.push(t.repo_name.clone());
        }
    }

    let preset = if borders {
        presets::UTF8_BORDERS_ONLY
    } else {
        presets::NOTHING
    };

    let mut sections: Vec<String> = Vec::new();

    for repo in &repos {
        let repo_triages: Vec<&TriageEntry> =
            triages.iter().filter(|t| &t.repo_name == repo).collect();

        let mut table = Table::new();
        table
            .load_preset(preset)
            .enforce_styling()
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(triage_header());

        for t in &repo_triages {
            table.add_row(triage_row(t));
        }

        let table_str = table.to_string();
        let indented: String = table_str
            .lines()
            .map(|line| format!("  {}", line))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("\x1B[1m{}\x1B[0m\n{}", repo, indented));
    }

    if is_plain() {
        format!("TRIAGES\n{}", sections.join("\n\n"))
    } else {
        format!("\x1B[1mTRIAGES\x1B[0m\n{}", sections.join("\n\n"))
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_duration_short_seconds() {
        assert_eq!(format_duration_short(0), "0s");
        assert_eq!(format_duration_short(1), "1s");
        assert_eq!(format_duration_short(45), "45s");
        assert_eq!(format_duration_short(59), "59s");
    }

    #[test]
    fn format_duration_short_minutes() {
        assert_eq!(format_duration_short(60), "1m");
        assert_eq!(format_duration_short(61), "1m1s");
        assert_eq!(format_duration_short(192), "3m12s");
        assert_eq!(format_duration_short(300), "5m");
        assert_eq!(format_duration_short(3599), "59m59s");
    }

    #[test]
    fn format_duration_short_hours() {
        assert_eq!(format_duration_short(3600), "1h");
        assert_eq!(format_duration_short(3660), "1h1m");
        assert_eq!(format_duration_short(7260), "2h1m");
        assert_eq!(format_duration_short(7200), "2h");
    }

    #[test]
    fn render_triage_section_empty_returns_empty() {
        let section = render_triage_section(&[], false);
        assert!(section.is_empty());
    }

    fn triage_entry(issue_id: &str, worker: &str, ago_secs: i64, repo: &str) -> TriageEntry {
        TriageEntry {
            issue_id: issue_id.to_string(),
            worker_name: worker.to_string(),
            spawned_at: chrono::Utc::now().timestamp() - ago_secs,
            repo_name: repo.to_string(),
        }
    }

    #[test]
    fn render_triage_section_shows_header_and_entries() {
        set_plain(true);
        let triages = vec![
            triage_entry("JIG-77", "triage-77", 134, "my-repo"),
            triage_entry("JIG-81", "triage-81", 45, "my-repo"),
        ];
        let section = render_triage_section(&triages, false);
        assert!(section.contains("TRIAGES"));
        assert!(section.contains("JIG-77"));
        assert!(section.contains("JIG-81"));
        assert!(section.contains("triage-77"));
        set_plain(false);
    }

    #[test]
    fn render_triage_table_has_correct_columns() {
        set_plain(true);
        let triages = vec![triage_entry("JIG-99", "triage-99", 3661, "test-repo")];
        let table = render_triage_table(&triages, false).to_string();
        assert!(table.contains("ISSUE"));
        assert!(table.contains("WORKER"));
        assert!(table.contains("ELAPSED"));
        assert!(table.contains("REPO"));
        assert!(table.contains("JIG-99"));
        assert!(table.contains("triage-99"));
        assert!(table.contains("test-repo"));
        set_plain(false);
    }

    #[test]
    fn render_triage_section_grouped_empty_returns_empty() {
        let section = render_triage_section_grouped(&[], false);
        assert!(section.is_empty());
    }

    #[test]
    fn render_triage_section_grouped_shows_repo_headers() {
        set_plain(true);
        let triages = vec![
            triage_entry("JIG-1", "triage-1", 10, "repo-a"),
            triage_entry("JIG-2", "triage-2", 20, "repo-b"),
        ];
        let section = render_triage_section_grouped(&triages, false);
        assert!(section.contains("TRIAGES"));
        assert!(section.contains("repo-a"));
        assert!(section.contains("repo-b"));
        assert!(section.contains("JIG-1"));
        assert!(section.contains("JIG-2"));
        set_plain(false);
    }
}
