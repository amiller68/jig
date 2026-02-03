//! Version command - show version information

use anyhow::Result;
use colored::Colorize;

pub fn run() -> Result<()> {
    eprintln!("{} {}", "jig".bold(), env!("CARGO_PKG_VERSION").cyan());
    Ok(())
}
