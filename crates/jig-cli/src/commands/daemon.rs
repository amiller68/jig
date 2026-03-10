//! Daemon command — run the orchestrator loop

use clap::Args;

use jig_core::daemon::{self, DaemonConfig};

use crate::op::{NoOutput, Op, RepoCtx};
use crate::ui;

/// Run the daemon loop to monitor workers and dispatch actions
#[derive(Args, Debug, Clone)]
pub struct Daemon {
    /// Poll interval in seconds
    #[arg(long, default_value = "30")]
    interval: u64,

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
        let config = DaemonConfig {
            interval_seconds: self.interval,
            once: self.once,
            ..Default::default()
        };

        if self.once {
            eprintln!("{}", ui::dim("Running single pass..."));
        } else {
            ui::success(&format!(
                "Daemon started (polling every {}s)",
                self.interval
            ));
            eprintln!("{}", ui::dim("Press Ctrl+C to stop."));
        }

        daemon::run(&config)?;

        Ok(NoOutput)
    }
}
