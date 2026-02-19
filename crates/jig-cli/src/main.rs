//! jig CLI - Git worktree manager for parallel Claude Code sessions

#[macro_use]
mod op;

mod cli;
mod commands;

use clap::Parser;
use colored::Colorize;

use cli::Cli;
use op::{Op, OpContext};

fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();

    if let Err(e) = run() {
        eprintln!("{} {}", "error:".red().bold(), e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            print_help();
            Ok(())
        }
        Some(command) => {
            let ctx = OpContext::new(cli.open, cli.no_hooks);
            let output = command.execute(&ctx)?;
            let output_str = output.to_string();
            if !output_str.is_empty() {
                println!("{}", output_str);
            }
            Ok(())
        }
    }
}

fn print_help() {
    eprintln!("{}", "jig - Git worktree manager".bold());
    eprintln!();
    eprintln!("{}", "USAGE:".bold());
    eprintln!("  jig <COMMAND> [OPTIONS]");
    eprintln!();
    eprintln!("{}", "WORKTREE COMMANDS:".bold());
    eprintln!("  {}      Create a new worktree", "create".cyan());
    eprintln!("  {}        List worktrees", "list".cyan());
    eprintln!("  {}        Open/cd into a worktree", "open".cyan());
    eprintln!("  {}      Remove worktree(s)", "remove".cyan());
    eprintln!("  {}        Exit current worktree", "exit".cyan());
    eprintln!();
    eprintln!("{}", "CONFIGURATION:".bold());
    eprintln!("  {}      Manage configuration", "config".cyan());
    eprintln!();
    eprintln!("{}", "WORKER COMMANDS:".bold());
    eprintln!(
        "  {}       Create worktree + launch Claude in tmux",
        "spawn".cyan()
    );
    eprintln!("  {}          Show status of spawned workers", "ps".cyan());
    eprintln!("  {}      Show detailed worker status", "status".cyan());
    eprintln!("  {}      Attach to tmux session", "attach".cyan());
    eprintln!("  {}      Show diff for parent review", "review".cyan());
    eprintln!(
        "  {}       Merge reviewed worktree into current branch",
        "merge".cyan()
    );
    eprintln!("  {}        Kill a running tmux window", "kill".cyan());
    eprintln!();
    eprintln!("{}", "UTILITY:".bold());
    eprintln!("  {}        Initialize repository for jig", "init".cyan());
    eprintln!("  {}      Update jig to latest version", "update".cyan());
    eprintln!("  {}     Show version information", "version".cyan());
    eprintln!("  {}       Show path to jig executable", "which".cyan());
    eprintln!(
        "  {}      Show terminal and dependency status",
        "health".cyan()
    );
    eprintln!("  {} Configure shell integration", "shell-setup".cyan());
    eprintln!();
    eprintln!("{}", "GLOBAL OPTIONS:".bold());
    eprintln!(
        "  {}            Open/cd into worktree after creating",
        "-o".cyan()
    );
    eprintln!("  {}    Skip on-create hook execution", "--no-hooks".cyan());
    eprintln!();
    eprintln!(
        "Use '{}' for more information about a command.",
        "jig <command> --help".cyan()
    );
}
