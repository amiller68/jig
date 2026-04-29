//! Daemon command — run the orchestrator loop

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use clap::Args;

use crate::config::GlobalConfig;
use crate::daemon::DaemonConfig;

use crate::cli::op::{NoOutput, Op, RepoCtx};
use crate::cli::ui;

/// Run the daemon loop to monitor workers and dispatch actions
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

    fn run(&self, _ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let global = GlobalConfig::load().unwrap_or_default();
        let interval = self.interval.unwrap_or(global.daemon.interval_seconds);
        let config = DaemonConfig {
            interval_seconds: interval,
            session_prefix: global.daemon.session_prefix.clone(),
            ..Default::default()
        };

        ui::success(&format!("Daemon started (polling every {}s)", interval));
        eprintln!("{}", ui::dim("Press Ctrl+C to stop."));

        let quit = Arc::new(AtomicBool::new(false));
        let quit_flag = Arc::clone(&quit);
        ctrlc::set_handler(move || {
            quit_flag.store(true, Ordering::Relaxed);
        })
        .ok();

        let mut daemon = crate::daemon::Daemon::start(config)?;
        daemon.run(&quit, |_daemon| {
            std::thread::sleep(Duration::from_secs(interval));
            true
        });

        Ok(NoOutput)
    }
}
