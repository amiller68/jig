//! Update command - update scribe to latest version

use anyhow::Result;
use colored::Colorize;

pub fn run(_force: bool) -> Result<()> {
    // For Rust binary, update would typically use self-update crate
    // or direct the user to their package manager

    eprintln!("{}", "Update".bold());
    eprintln!();
    eprintln!("To update scribe, reinstall from the install script:");
    eprintln!();
    eprintln!("  {} curl -fsSL https://raw.githubusercontent.com/amiller68/scribe-rs/main/install.sh | bash", "→".dimmed());
    eprintln!();
    eprintln!("Or rebuild from source:");
    eprintln!();
    eprintln!("  {} cargo install --git https://github.com/amiller68/scribe-rs", "→".dimmed());

    Ok(())
}
