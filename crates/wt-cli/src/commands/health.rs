//! Health command - show terminal and dependency status

use anyhow::Result;
use colored::Colorize;

use wt_core::terminal::{self, Terminal};

pub fn run() -> Result<()> {
    let term = terminal::detect_terminal();

    eprintln!("{}", "Health Check".bold());
    eprintln!();

    // Terminal detection
    eprintln!("{}", "Terminal:".bold());
    eprintln!("  Detected: {}", term.name().cyan());
    eprintln!(
        "  Tab support: {}",
        if term.supports_tabs() {
            "yes".green()
        } else {
            "no".yellow()
        }
    );

    eprintln!();

    // Dependencies
    eprintln!("{}", "Dependencies:".bold());
    let deps = terminal::check_dependencies();

    for dep in deps {
        let status = if dep.available {
            "✓".green()
        } else if dep.required {
            "✗".red()
        } else {
            "○".yellow()
        };

        let label = if dep.required {
            format!("{} (required)", dep.name)
        } else {
            format!("{} (optional)", dep.name)
        };

        eprintln!("  {} {}", status, label);
    }

    // Terminal-specific tools
    eprintln!();
    eprintln!("{}", "Terminal Tools:".bold());

    match term {
        Terminal::Kitty => {
            let has_kitten = terminal::command_exists("kitten");
            let status = if has_kitten { "✓".green() } else { "○".yellow() };
            eprintln!("  {} kitten (for tab support)", status);
        }
        Terminal::WezTerm => {
            let has_wezterm = terminal::command_exists("wezterm");
            let status = if has_wezterm { "✓".green() } else { "○".yellow() };
            eprintln!("  {} wezterm CLI", status);
        }
        _ => {
            eprintln!("  No additional tools needed");
        }
    }

    Ok(())
}
