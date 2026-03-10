//! Ps command — show status of spawned sessions

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use clap::Args;
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use crossterm::terminal::{self, disable_raw_mode};

use jig_core::config::JigToml;
use jig_core::daemon::{DaemonConfig, RuntimeConfig, TickResult};

use crate::op::{GlobalCtx, NoOutput, Op, RepoCtx};
use crate::ui;

/// Show status of spawned sessions
#[derive(Args, Debug, Clone)]
pub struct Ps {
    /// Watch mode: refresh every N seconds (default 2)
    #[arg(short, long, num_args = 0..=1, default_missing_value = "2")]
    pub watch: Option<u64>,

    /// Enable auto-spawning of workers from eligible issues
    #[arg(long)]
    auto_spawn: bool,

    /// Maximum number of concurrent auto-spawned workers
    #[arg(long)]
    max_workers: Option<usize>,
}

#[derive(Debug, thiserror::Error)]
pub enum PsError {
    #[error("failed to list tasks: {0}")]
    ListTasks(#[from] jig_core::Error),
}

impl Op for Ps {
    type Error = PsError;
    type Output = NoOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let repo = ctx.repo()?;
        let repo_filter = repo
            .repo_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string());
        let runtime_config = self.build_runtime_config(&repo.repo_root);
        self.execute_ps(repo_filter, runtime_config, false)
    }

    fn run_global(&self, _ctx: &GlobalCtx) -> Result<Self::Output, Self::Error> {
        self.execute_ps(None, RuntimeConfig::default(), true)
    }
}

impl Ps {
    fn execute_ps(
        &self,
        repo_filter: Option<String>,
        runtime_config: RuntimeConfig,
        global: bool,
    ) -> Result<NoOutput, PsError> {
        if let Some(interval) = self.watch {
            let interval = if interval == 0 { 2 } else { interval };
            run_watch(interval, runtime_config, repo_filter, global);
            return Ok(NoOutput);
        }

        let daemon_config = DaemonConfig {
            once: true,
            skip_sync: true,
            repo_filter,
            ..Default::default()
        };

        let mut display = vec![];
        jig_core::daemon::run_with(&daemon_config, runtime_config, |tick, _| {
            display.clone_from(&tick.worker_display);
            false
        })?;

        if display.is_empty() {
            eprintln!("No spawned sessions");
        } else if global {
            let output = ui::render_worker_table_grouped(&display, false);
            eprintln!("{output}");
        } else {
            let table = ui::render_worker_table(&display, false);
            eprintln!("{table}");
        }

        Ok(NoOutput)
    }

    /// Build RuntimeConfig from CLI flags + jig.toml + global config.
    fn build_runtime_config(&self, repo_root: &std::path::Path) -> RuntimeConfig {
        let jig_toml = JigToml::load(repo_root).ok().flatten().unwrap_or_default();
        let global_config = jig_core::global::GlobalConfig::load().unwrap_or_default();
        let spawn_config = &jig_toml.spawn;

        let auto_spawn = self.auto_spawn || spawn_config.resolve_auto_spawn(&global_config.spawn);
        let max_concurrent_workers = self
            .max_workers
            .unwrap_or_else(|| spawn_config.resolve_max_concurrent_workers(&global_config.spawn));

        RuntimeConfig {
            auto_spawn,
            max_concurrent_workers,
            auto_spawn_interval: spawn_config.resolve_auto_spawn_interval(&global_config.spawn),
            sync_interval: 60,
        }
    }
}

/// View mode for the watch display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Table,
    Logs,
}

impl ViewMode {
    fn toggle(&mut self) {
        *self = match self {
            ViewMode::Table => ViewMode::Logs,
            ViewMode::Logs => ViewMode::Table,
        };
    }
}

const LOG_BUFFER_SIZE: usize = 50;

/// Format structured log lines from a TickResult.
fn format_tick_log(tick: &TickResult) -> Vec<String> {
    let now = chrono::Local::now().format("%H:%M:%S");
    let mut lines = vec![];

    lines.push(format!(
        "[{}] tick: {} workers, {} actions, {} nudges, {} errors",
        now,
        tick.workers_checked,
        tick.actions_dispatched,
        tick.nudges_sent,
        tick.errors.len(),
    ));

    for (key, info) in &tick.worker_info {
        if !info.has_pr {
            continue;
        }
        if let Some(err) = &info.pr_error {
            lines.push(format!("[{}]   {} PR: {}", now, key, err));
        } else if !info.pr_checks.is_empty() {
            let problems: Vec<&str> = info
                .pr_checks
                .iter()
                .filter(|(_, bad)| *bad)
                .map(|(name, _)| name.as_str())
                .collect();
            if problems.is_empty() {
                lines.push(format!("[{}]   {} PR: ok", now, key));
            } else {
                lines.push(format!("[{}]   {} PR: {}", now, key, problems.join(", ")));
            }
        }
    }

    for spawned in &tick.auto_spawned {
        lines.push(format!("[{}]   auto-spawned: {}", now, spawned));
    }

    for pruned in &tick.pruned {
        lines.push(format!("[{}]   pruned: {}", now, pruned));
    }

    for err in &tick.errors {
        lines.push(format!("[{}]   error: {}", now, err));
    }

    lines
}

/// Run the watch loop: display + orchestrate via daemon::run_with.
fn run_watch(
    interval: u64,
    runtime_config: RuntimeConfig,
    repo_filter: Option<String>,
    global: bool,
) {
    let daemon_config = DaemonConfig {
        interval_seconds: interval,
        once: false,
        skip_sync: false,
        repo_filter,
        ..Default::default()
    };

    let auto_spawn = runtime_config.auto_spawn;

    // Shared state for the callback
    let mut view_mode = ViewMode::Table;
    let mut log_buffer: VecDeque<String> = VecDeque::with_capacity(LOG_BUFFER_SIZE);

    // Enable raw mode for keypress detection
    terminal::enable_raw_mode().ok();

    // Clear screen once on first render
    eprint!("\x1B[2J");

    // Spawn a dedicated key-polling thread. It continuously reads crossterm events
    // and sets `quit` when q/Esc/Ctrl-C is pressed. This runs DURING ticks too,
    // so 'q' pressed mid-tick is caught immediately rather than after the tick finishes.
    // Toggle keys (l/t) are stored in a separate flag for the callback to pick up.
    let quit_for_thread = Arc::new(AtomicBool::new(false));
    let toggle_flag = Arc::new(AtomicBool::new(false));
    {
        let quit_bg = Arc::clone(&quit_for_thread);
        let toggle_bg = Arc::clone(&toggle_flag);
        std::thread::spawn(move || {
            while !quit_bg.load(Ordering::Relaxed) {
                if !event::poll(std::time::Duration::from_millis(50)).unwrap_or(false) {
                    continue;
                }
                if let Ok(Event::Key(KeyEvent {
                    code, modifiers, ..
                })) = event::read()
                {
                    match code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            quit_bg.store(true, Ordering::Relaxed);
                            return;
                        }
                        KeyCode::Char('c')
                            if modifiers.contains(crossterm::event::KeyModifiers::CONTROL) =>
                        {
                            quit_bg.store(true, Ordering::Relaxed);
                            return;
                        }
                        KeyCode::Char('l') | KeyCode::Char('t') => {
                            toggle_bg.store(true, Ordering::Relaxed);
                        }
                        _ => {}
                    }
                }
            }
        });
    }

    let result = jig_core::daemon::run_with(&daemon_config, runtime_config, |tick, quit| {
        // The background thread sets quit_for_thread; propagate to the daemon's quit flag
        if quit_for_thread.load(Ordering::Relaxed) {
            quit.store(true, Ordering::Relaxed);
            return false;
        }

        // Check for toggle
        if toggle_flag.swap(false, Ordering::Relaxed) {
            view_mode.toggle();
        }

        // Append log entries
        for line in format_tick_log(tick) {
            if log_buffer.len() >= LOG_BUFFER_SIZE {
                log_buffer.pop_front();
            }
            log_buffer.push_back(line);
        }

        let render = |view: &ViewMode, tick: &TickResult, logs: &VecDeque<String>| {
            eprint!("\x1B[H");
            match view {
                ViewMode::Table => {
                    let table_output = if global {
                        ui::render_worker_table_grouped(&tick.worker_display, true)
                    } else {
                        ui::render_worker_table(&tick.worker_display, true).to_string()
                    };
                    let status_line = format_tick_status(&Some(tick));
                    let auto_label = if auto_spawn {
                        "  \x1B[33mauto\x1B[0m"
                    } else {
                        ""
                    };
                    let spawning_section = if tick.spawning.is_empty() {
                        String::new()
                    } else {
                        let names: Vec<&str> = tick.spawning.iter().map(|s| s.as_str()).collect();
                        format!(
                            "\n\x1B[2mspawning:\x1B[0m \x1B[33m{}\x1B[0m\n",
                            names.join(", ")
                        )
                    };
                    let output = format!(
                        "\x1B[1mjig ps --watch\x1B[0m — {} workers  \x1B[2m(every {}s)\x1B[0m{status_line}{auto_label}\n\n{table_output}{spawning_section}\n\x1B[2m[l]ogs  [q]uit\x1B[0m",
                        tick.worker_display.len(), interval,
                    );
                    for line in output.lines() {
                        eprint!("{}\x1B[K\r\n", line);
                    }
                }
                ViewMode::Logs => {
                    eprint!(
                        "\x1B[1mjig ps --watch\x1B[0m — logs  \x1B[2m(every {}s)\x1B[0m\x1B[K\r\n",
                        interval
                    );
                    eprint!("\x1B[K\r\n");
                    for line in logs {
                        eprint!("{}\x1B[K\r\n", line);
                    }
                    eprint!("\x1B[K\r\n");
                    eprint!("\x1B[2m[t]able  [q]uit\x1B[0m\x1B[K\r\n");
                }
            }
            eprint!("\x1B[J");
        };

        render(&view_mode, tick, &log_buffer);

        // Sleep interval — the background thread handles all key polling,
        // so we just need to wait and check for quit/toggle periodically.
        let sleep_end = Instant::now() + std::time::Duration::from_secs(interval);
        while Instant::now() < sleep_end {
            if quit_for_thread.load(Ordering::Relaxed) {
                quit.store(true, Ordering::Relaxed);
                return false;
            }
            if toggle_flag.swap(false, Ordering::Relaxed) {
                view_mode.toggle();
                render(&view_mode, tick, &log_buffer);
                continue;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        true // keep looping
    });

    disable_raw_mode().ok();

    match result {
        Ok(_) => {}
        Err(e) => eprintln!("daemon error: {}", e),
    }
}

/// Format the daemon tick result as a compact status suffix.
fn format_tick_status(tick: &Option<&TickResult>) -> String {
    let Some(tick) = tick else {
        return String::new();
    };
    let mut parts = vec![];
    if tick.nudges_sent > 0 {
        parts.push(format!(
            "{} nudge{}",
            tick.nudges_sent,
            if tick.nudges_sent == 1 { "" } else { "s" }
        ));
    }
    if tick.notifications_sent > 0 {
        parts.push(format!("{} notify", tick.notifications_sent));
    }
    if !tick.errors.is_empty() {
        parts.push(format!(
            "{} err{}",
            tick.errors.len(),
            if tick.errors.len() == 1 { "" } else { "s" }
        ));
    }
    if !tick.auto_spawned.is_empty() {
        parts.push(format!("{} spawned", tick.auto_spawned.len()));
    }
    if !tick.spawning.is_empty() {
        parts.push(format!("spawning {}", tick.spawning.len()));
    }
    if !tick.pruned.is_empty() {
        parts.push(format!("{} pruned", tick.pruned.len()));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("  \x1B[2m[{}]\x1B[0m", parts.join(", "))
    }
}
