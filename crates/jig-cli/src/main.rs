//! jig CLI - Git worktree manager for parallel Claude Code sessions

#[macro_use]
mod op;

mod cli;
mod commands;

use clap::Parser;
use colored::Colorize;

use cli::{Cli, Command};
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

    // Best-effort global directory setup
    let _ = jig_core::ensure_global_dirs();

    // Build context once (derives RepoContext from cwd)
    let ctx = OpContext::new(false);

    // Best-effort auto-registration and pruning of current repo
    if let Some(repo) = &ctx.repo {
        if let Ok(mut registry) = jig_core::RepoRegistry::load() {
            let _ = registry.register(repo.repo_root.clone());
            let pruned = registry.prune();
            if cli.verbose && !pruned.is_empty() {
                for p in &pruned {
                    eprintln!("{} {}", "pruned:".dimmed(), p.display());
                }
            }
            let _ = registry.save();
        }
    }

    match cli.command {
        None => {
            print_help();
            Ok(())
        }
        Some(ref command) => {
            if cli.global {
                run_global(command)
            } else {
                let output = command.execute(&ctx)?;
                let output_str = output.to_string();
                if !output_str.is_empty() {
                    println!("{}", output_str);
                }
                Ok(())
            }
        }
    }
}

/// Check if a command is compatible with --global
fn is_global_compatible(command: &Command) -> bool {
    matches!(
        command,
        Command::List(_) | Command::Ps(_) | Command::Status(_) | Command::Issues(_)
    )
}

fn run_global(command: &Command) -> Result<(), Box<dyn std::error::Error>> {
    if !is_global_compatible(command) {
        eprintln!(
            "{} this command does not support --global",
            "warning:".yellow().bold()
        );
        return Ok(());
    }

    let registry = jig_core::RepoRegistry::load()?;
    let repos = registry.repos();

    if repos.is_empty() {
        eprintln!("No repos registered. Run jig in a repo first.");
        return Ok(());
    }

    for entry in repos {
        if !entry.path.exists() {
            continue;
        }
        std::env::set_current_dir(&entry.path)?;
        let repo_name = entry.path.file_name().unwrap_or_default().to_string_lossy();
        eprintln!("{}", format!("[{}]", repo_name).bold());
        // Re-derive context for each repo directory
        let ctx = OpContext::new(true);
        match command.execute(&ctx) {
            Ok(output) => {
                let s = output.to_string();
                if !s.is_empty() {
                    println!("{}", s);
                }
            }
            Err(e) => eprintln!("  {} {}", "error:".red(), e),
        }
    }

    Ok(())
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
    eprintln!(
        "  {}        Nuke all workers and state (keeps config)",
        "nuke".cyan()
    );
    eprintln!();
    eprintln!("{}", "ISSUES:".bold());
    eprintln!("  {}      Browse and filter issues", "issues".cyan());
    eprintln!();
    eprintln!("{}", "REPOSITORY TRACKING:".bold());
    eprintln!("  {}       List tracked repositories", "repos".cyan());
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
        "  {}  Run command across all tracked repos",
        "-g, --global".cyan()
    );
    eprintln!("  {} Show verbose output", "-v, --verbose".cyan());
    eprintln!();
    eprintln!(
        "Use '{}' for more information about a command.",
        "jig <command> --help".cyan()
    );
}
