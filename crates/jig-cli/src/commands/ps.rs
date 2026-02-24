//! Ps command — show status of spawned sessions

use std::fmt;

use clap::Args;
use comfy_table::{presets, Attribute, Cell, CellAlignment, Color, ContentArrangement, Table};

use jig_core::events::{EventLog, WorkerState};
use jig_core::global::GlobalConfig;
use jig_core::spawn::{self, TaskInfo, TaskStatus};
use jig_core::worker::WorkerStatus;

use crate::op::{Op, OpContext};

/// Show status of spawned sessions
#[derive(Args, Debug, Clone)]
pub struct Ps {
    /// Watch mode: refresh every N seconds (default 2)
    #[arg(short, long, num_args = 0..=1, default_missing_value = "2")]
    watch: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum PsError {
    #[error("failed to list tasks: {0}")]
    ListTasks(#[from] jig_core::Error),
}

/// Extended task info with event-derived state.
#[derive(Debug)]
struct EnrichedTask {
    info: TaskInfo,
    worker_status: Option<WorkerStatus>,
    nudge_count: u32,
    pr_url: Option<String>,
}

#[derive(Debug)]
pub struct PsOutput {
    pub tasks: Vec<TaskInfo>,
}

impl Op for Ps {
    type Error = PsError;
    type Output = PsOutput;

    fn execute(&self, ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        let repo = ctx.repo()?;

        if let Some(interval) = self.watch {
            let interval = if interval == 0 { 2 } else { interval };
            run_watch(repo, interval);
            // Watch mode loops forever, but if it somehow returns:
            return Ok(PsOutput { tasks: vec![] });
        }

        let tasks = spawn::list_tasks(repo)?;
        Ok(PsOutput { tasks })
    }
}

/// Run the watch loop: clear screen, render, sleep, repeat.
fn run_watch(repo: &jig_core::RepoContext, interval: u64) {
    let config = GlobalConfig::load().unwrap_or_default();
    let repo_name = repo
        .repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    loop {
        // Clear screen and move cursor to top-left
        eprint!("\x1B[2J\x1B[H");

        let tasks = match spawn::list_tasks(repo) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Error: {}", e);
                std::thread::sleep(std::time::Duration::from_secs(interval));
                continue;
            }
        };

        let enriched = enrich_tasks(&tasks, &repo_name, &config);
        let table = render_watch_table(&enriched);
        eprintln!(
            "\x1B[1mjig ps --watch\x1B[0m — {} workers  \x1B[2m(every {}s, Ctrl+C to stop)\x1B[0m",
            enriched.len(),
            interval,
        );
        eprintln!();
        eprintln!("{table}");

        // Check for 'q' keypress (non-blocking) — best-effort via sleep
        std::thread::sleep(std::time::Duration::from_secs(interval));
    }
}

/// Enrich tasks with event-derived worker state.
fn enrich_tasks(tasks: &[TaskInfo], repo_name: &str, config: &GlobalConfig) -> Vec<EnrichedTask> {
    tasks
        .iter()
        .map(|task| {
            let (worker_status, nudge_count, pr_url) =
                match EventLog::for_worker(repo_name, &task.name) {
                    Ok(log) => match log.read_all() {
                        Ok(events) if !events.is_empty() => {
                            let state = WorkerState::reduce(&events, &config.health);
                            let nudges: u32 = state.nudge_counts.values().sum();
                            (Some(state.status), nudges, state.pr_url)
                        }
                        _ => (None, 0, None),
                    },
                    Err(_) => (None, 0, None),
                };

            EnrichedTask {
                info: TaskInfo {
                    name: task.name.clone(),
                    status: task.status,
                    branch: task.branch.clone(),
                    commits_ahead: task.commits_ahead,
                    is_dirty: task.is_dirty,
                },
                worker_status,
                nudge_count,
                pr_url,
            }
        })
        .collect()
}

/// Render the enriched watch table.
fn render_watch_table(tasks: &[EnrichedTask]) -> Table {
    let mut table = Table::new();
    table
        .load_preset(presets::NOTHING)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("NAME").add_attribute(Attribute::Bold),
            Cell::new("TMUX").add_attribute(Attribute::Bold),
            Cell::new("STATE").add_attribute(Attribute::Bold),
            Cell::new("BRANCH").add_attribute(Attribute::Bold),
            Cell::new("COMMITS").add_attribute(Attribute::Bold),
            Cell::new("DIRTY").add_attribute(Attribute::Bold),
            Cell::new("NUDGES").add_attribute(Attribute::Bold),
            Cell::new("PR").add_attribute(Attribute::Bold),
        ]);

    for task in tasks {
        let (tmux_text, tmux_color) = match task.info.status {
            TaskStatus::Running => ("●", Color::Green),
            TaskStatus::Exited => ("○", Color::Yellow),
            TaskStatus::NoSession | TaskStatus::NoWindow => ("✗", Color::Red),
        };

        let (state_text, state_color) = match task.worker_status {
            Some(WorkerStatus::Running) => ("running", Color::Green),
            Some(WorkerStatus::Spawned) => ("spawned", Color::Blue),
            Some(WorkerStatus::Idle) => ("idle", Color::Yellow),
            Some(WorkerStatus::WaitingInput) => ("waiting", Color::Magenta),
            Some(WorkerStatus::Stalled) => ("stalled", Color::Red),
            Some(WorkerStatus::WaitingReview) => ("review", Color::Cyan),
            Some(WorkerStatus::Approved) => ("approved", Color::Green),
            Some(WorkerStatus::Merged) => ("merged", Color::Green),
            Some(WorkerStatus::Failed) => ("failed", Color::Red),
            Some(WorkerStatus::Archived) => ("archived", Color::DarkGrey),
            None => ("-", Color::DarkGrey),
        };

        let dirty = if task.info.is_dirty { "●" } else { "-" };

        let pr = task
            .pr_url
            .as_ref()
            .map(|url| {
                // Show just the PR number if it's a GitHub URL
                url.rsplit('/')
                    .next()
                    .map(|n| format!("#{}", n))
                    .unwrap_or_else(|| "yes".to_string())
            })
            .unwrap_or_else(|| "-".to_string());

        let nudge_text = if task.nudge_count > 0 {
            format!("{}", task.nudge_count)
        } else {
            "-".to_string()
        };
        let nudge_color = if task.nudge_count >= 3 {
            Color::Red
        } else if task.nudge_count > 0 {
            Color::Yellow
        } else {
            Color::DarkGrey
        };

        table.add_row(vec![
            Cell::new(&task.info.name).fg(Color::Cyan),
            Cell::new(tmux_text).fg(tmux_color),
            Cell::new(state_text).fg(state_color),
            Cell::new(&task.info.branch),
            Cell::new(task.info.commits_ahead).set_alignment(CellAlignment::Right),
            Cell::new(dirty).set_alignment(CellAlignment::Center),
            Cell::new(&nudge_text)
                .fg(nudge_color)
                .set_alignment(CellAlignment::Right),
            Cell::new(&pr),
        ]);
    }

    table
}

impl fmt::Display for PsOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.tasks.is_empty() {
            return write!(f, "No spawned sessions");
        }

        let mut table = Table::new();
        table
            .load_preset(presets::NOTHING)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![
                Cell::new("NAME").add_attribute(Attribute::Bold),
                Cell::new("STATUS").add_attribute(Attribute::Bold),
                Cell::new("BRANCH").add_attribute(Attribute::Bold),
                Cell::new("COMMITS").add_attribute(Attribute::Bold),
                Cell::new("DIRTY").add_attribute(Attribute::Bold),
            ]);

        for task in &self.tasks {
            let (status_text, status_color) = match task.status {
                TaskStatus::Running => (task.status.as_str(), Color::Green),
                TaskStatus::Exited => (task.status.as_str(), Color::Yellow),
                TaskStatus::NoSession | TaskStatus::NoWindow => (task.status.as_str(), Color::Red),
            };

            let dirty_indicator = if task.is_dirty { "●" } else { "-" };

            table.add_row(vec![
                Cell::new(&task.name).fg(Color::Cyan),
                Cell::new(status_text).fg(status_color),
                Cell::new(&task.branch),
                Cell::new(task.commits_ahead).set_alignment(CellAlignment::Right),
                Cell::new(dirty_indicator).set_alignment(CellAlignment::Center),
            ]);
        }

        write!(f, "{table}")
    }
}
