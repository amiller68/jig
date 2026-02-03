//! Kill command - kill a running tmux window

use anyhow::Result;
use colored::Colorize;

use wt_core::spawn;

pub fn run(name: &str) -> Result<()> {
    // Kill tmux window
    spawn::kill_window(name)?;

    // Unregister from spawn state
    spawn::unregister(name)?;

    eprintln!("{} Killed session '{}'", "✓".green(), name.cyan());

    Ok(())
}
