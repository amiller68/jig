//! Ps command — show status of spawned sessions

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use clap::Args;
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use crossterm::terminal::{self, disable_raw_mode};

use crate::context::{Context, JigToml};

use crate::cli::op::{NoOutput, Op};
use crate::cli::ui;

/// Show status of spawned sessions
#[derive(Args, Debug, Clone)]
pub struct Ps {
    /// Watch mode: refresh every N seconds (default 2)
    #[arg(short, long, num_args = 0..=1, default_missing_value = "2")]
    pub watch: Option<u64>,

    /// Maximum number of concurrent auto-spawned workers
    #[arg(long)]
    max_workers: Option<usize>,

    /// Operate on all tracked repos
    #[arg(short = 'g', long)]
    global: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum PsError {
    #[error("failed to list tasks: {0}")]
    ListTasks(#[from] jig_core::Error),
}

impl Op for Ps {
    type Error = PsError;
    type Output = NoOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        let mut cfg = if self.global {
            Context::from_global()?
        } else {
            Context::from_cwd()?
        };
        if !self.global {
            let jig_toml = JigToml::load(&cfg.repo()?.repo_root).ok().flatten().unwrap_or_default();
            cfg.config.max_concurrent_workers = self.max_workers.unwrap_or(jig_toml.spawn.max_concurrent_workers);
        }
        self.execute_ps(cfg, self.global)
    }
}

impl Ps {
    fn execute_ps(&self, mut cfg: Context, global: bool) -> Result<NoOutput, PsError> {
        if let Some(interval) = self.watch {
            cfg.config.tick_interval = if interval == 0 { 2 } else { interval };
            run_watch(cfg, global);
            return Ok(NoOutput);
        }

        let mut workers = vec![];
        let mut triages = vec![];
        let quit = AtomicBool::new(false);
        let mut daemon = crate::daemon::Daemon::start(cfg)?;
        daemon.run(&quit, |daemon| {
            workers = daemon.monitor.actor().workers();
            triages = daemon.triage.actor().active_entries();
            false
        });

        if workers.is_empty() && triages.is_empty() {
            eprintln!("No spawned sessions");
        } else if global {
            if !workers.is_empty() {
                let output = ui::render_worker_table_grouped(&workers, false);
                eprintln!("{output}");
            }
            let triage_section = ui::render_triage_section_grouped(&triages, false);
            if !triage_section.is_empty() {
                if !workers.is_empty() {
                    eprintln!();
                }
                eprintln!("{triage_section}");
            }
        } else {
            if !workers.is_empty() {
                let table = ui::render_worker_table(&workers, false);
                eprintln!("{table}");
            }
            let triage_section = ui::render_triage_section(&triages, false);
            if !triage_section.is_empty() {
                if !workers.is_empty() {
                    eprintln!();
                }
                eprintln!("{triage_section}");
            }
        }

        Ok(NoOutput)
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

/// Run the watch loop: display + orchestrate via daemon::run_with.
fn run_watch(cfg: Context, global: bool) {
    let interval = cfg.config.tick_interval;

    let mut view_mode = ViewMode::Table;
    let mut log_buffer: VecDeque<String> = VecDeque::with_capacity(LOG_BUFFER_SIZE);
    let mut log_tailer = crate::context::log::LogTailer::new();

    terminal::enable_raw_mode().ok();
    eprint!("\x1B[2J");

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

    let result = crate::daemon::Daemon::start(cfg);
    let mut daemon = match result {
        Ok(d) => d,
        Err(e) => {
            disable_raw_mode().ok();
            eprintln!("daemon error: {}", e);
            return;
        }
    };
    daemon.run(&quit_for_thread, |daemon| {
        if toggle_flag.swap(false, Ordering::Relaxed) {
            view_mode.toggle();
        }

        for line in log_tailer.poll(LOG_BUFFER_SIZE) {
            if log_buffer.len() >= LOG_BUFFER_SIZE {
                log_buffer.pop_front();
            }
            log_buffer.push_back(line);
        }

        let workers = daemon.monitor.actor().workers();
        let triages = daemon.triage.actor().active_entries();
        let spawning = daemon.spawn.actor().spawning_workers();
        let poll_remaining = daemon.poll_remaining_secs();

        let render = |view: &ViewMode, logs: &VecDeque<String>| {
            eprint!("\x1B[H");
            match view {
                ViewMode::Table => {
                    let table_output = if global {
                        ui::render_worker_table_grouped(&workers, true)
                    } else {
                        ui::render_worker_table(&workers, true).to_string()
                    };
                    let triage_output = if global {
                        ui::render_triage_section_grouped(&triages, true)
                    } else {
                        ui::render_triage_section(&triages, true)
                    };
                    let triage_count = if triages.is_empty() {
                        String::new()
                    } else {
                        format!(", {} triages", triages.len())
                    };
                    let spawning_section = if spawning.is_empty() {
                        String::new()
                    } else {
                        let names: Vec<&str> = spawning.iter().map(|s| s.as_str()).collect();
                        format!(
                            "\n\x1B[2mspawning:\x1B[0m \x1B[33m{}\x1B[0m\n",
                            names.join(", ")
                        )
                    };
                    let timer_section = format!(
                        "  poll: {}",
                        ui::format_duration_short(poll_remaining)
                    );
                    let triage_section = if triage_output.is_empty() {
                        String::new()
                    } else {
                        format!("\n{triage_output}\n")
                    };
                    let output = format!(
                        "\x1B[1mjig ps --watch\x1B[0m — {} workers{triage_count}  \x1B[2m(every {}s)\x1B[0m\n\n{table_output}{triage_section}{spawning_section}\n\x1B[2m[l]ogs  [q]uit{timer_section}\x1B[0m",
                        workers.len(), interval,
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

        render(&view_mode, &log_buffer);

        let sleep_end = Instant::now() + std::time::Duration::from_secs(interval);
        while Instant::now() < sleep_end {
            if quit_for_thread.load(Ordering::Relaxed) {
                return false;
            }
            if toggle_flag.swap(false, Ordering::Relaxed) {
                view_mode.toggle();
                render(&view_mode, &log_buffer);
                continue;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        true
    });

    disable_raw_mode().ok();
}
