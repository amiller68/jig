//! Daemon command — run the orchestrator loop

use std::time::Duration;

use clap::Args;

use jig_core::config::GlobalConfig;
use crate::daemon::{self, DaemonConfig, RuntimeConfig};

use crate::op::{NoOutput, Op, RepoCtx};
use crate::ui;

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

        daemon::run_with(&config, RuntimeConfig::default(), |_tick, _quit| {
            std::thread::sleep(Duration::from_secs(interval));
            true
        })?;

        Ok(NoOutput)
    }
}
