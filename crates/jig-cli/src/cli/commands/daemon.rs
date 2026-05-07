//! Daemon command — run the orchestrator loop

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use clap::Args;

use crate::context::Context;

use crate::cli::op::{NoOutput, Op};
use crate::cli::ui;

/// Run the daemon loop to monitor and manage workers
#[derive(Args, Debug, Clone)]
pub struct Daemon {
    /// Poll interval in seconds
    #[arg(long)]
    interval: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
}

impl Op for Daemon {
    type Error = DaemonError;
    type Output = NoOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        let mut cfg = Context::from_global()?;
        if let Some(interval) = self.interval {
            cfg.config.tick_interval = interval;
        }
        let interval = cfg.config.tick_interval;

        ui::success(&format!("Daemon started (polling every {}s)", interval));
        eprintln!("{}", ui::dim("Press Ctrl+C to stop."));

        let quit = Arc::new(AtomicBool::new(false));
        let quit_flag = Arc::clone(&quit);
        ctrlc::set_handler(move || {
            quit_flag.store(true, Ordering::Relaxed);
        })
        .ok();

        let mut daemon = crate::daemon::Daemon::start(cfg)?;
        daemon.run(&quit, |_daemon| {
            std::thread::sleep(Duration::from_secs(interval));
            true
        });

        Ok(NoOutput)
    }
}
