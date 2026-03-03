//! Shared rendering utilities for CLI output.

use comfy_table::{presets, Attribute, Cell, CellAlignment, Color, ContentArrangement, Table};

use jig_core::daemon::{WorkerDisplayInfo, WorkerTickInfo};
use jig_core::spawn::TaskStatus;
use jig_core::worker::WorkerStatus;

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

/// Single source of truth: WorkerStatus → comfy_table color.
pub fn worker_state_color(status: &WorkerStatus) -> Color {
    match status {
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

/// Format WorkerStatus as a colored string (for `colored` crate / eprintln contexts).
pub fn format_worker_status_colored(status: &WorkerStatus) -> String {
    use colored::Colorize;
    match status {
        WorkerStatus::Spawned => "spawned".blue().to_string(),
        WorkerStatus::Running => "running".green().to_string(),
        WorkerStatus::Idle => "idle".yellow().to_string(),
        WorkerStatus::WaitingInput => "waiting input".magenta().to_string(),
        WorkerStatus::Stalled => "stalled".red().to_string(),
        WorkerStatus::WaitingReview => "waiting review".cyan().to_string(),
        WorkerStatus::Approved => "approved".green().to_string(),
        WorkerStatus::Merged => "merged".green().bold().to_string(),
        WorkerStatus::Failed => "failed".red().to_string(),
        WorkerStatus::Archived => "archived".dimmed().to_string(),
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
        .set_header(vec![
            Cell::new("WORKER").add_attribute(Attribute::Bold),
            Cell::new("STATE").add_attribute(Attribute::Bold),
            Cell::new("COMMITS").add_attribute(Attribute::Bold),
            Cell::new("PR").add_attribute(Attribute::Bold),
            Cell::new("HEALTH").add_attribute(Attribute::Bold),
            Cell::new("ISSUE").add_attribute(Attribute::Bold),
        ]);

    for w in workers {
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

        table.add_row(vec![
            Cell::new(&name).fg(tmux_color),
            Cell::new(state_text).fg(state_color),
            Cell::new(&commits)
                .fg(commit_color)
                .set_alignment(CellAlignment::Right),
            Cell::new(&pr).fg(pr_color),
            Cell::new(&health_text).fg(health_color),
            Cell::new(&issue).fg(issue_color),
        ]);
    }

    table
}
