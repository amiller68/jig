//! Daemon command — run the orchestrator loop

use clap::Args;

use jig_core::daemon::{self, DaemonConfig};
use jig_core::global::GlobalConfig;

use crate::op::{NoOutput, Op, RepoCtx};
use crate::ui;

/// Run the daemon loop to monitor workers and dispatch actions
#[derive(Args, Debug, Clone)]
pub struct Daemon {
    /// Poll interval in seconds
    #[arg(long)]
    interval: Option<u64>,

    /// Run once and exit (no loop)
    #[arg(long)]
    once: bool,
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
            once: self.once,
            session_prefix: global.daemon.session_prefix.clone(),
            ..Default::default()
        };

        if self.once {
            eprintln!("{}", ui::dim("Running single pass..."));
        } else {
            ui::success(&format!("Daemon started (polling every {}s)", interval));
            eprintln!("{}", ui::dim("Press Ctrl+C to stop."));
        }

        daemon::run(&config)?;

        Ok(NoOutput)
    }
}
