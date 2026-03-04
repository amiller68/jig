//! jig CLI - Git worktree manager for parallel Claude Code sessions

#[macro_use]
mod op;

mod cli;
mod commands;
mod ui;

use clap::Parser;
use colored::Colorize;

use cli::Cli;
use op::{GlobalCtx, Op, RepoCtx};

fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
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

    match cli.command {
        None => {
            print_help();
            Ok(())
        }
        Some(ref command) => {
            let output = if cli.global {
                let registry = jig_core::RepoRegistry::load().unwrap_or_default();
                let repos: Vec<_> = registry
                    .repos()
                    .iter()
                    .filter(|e| e.path.exists())
                    .filter_map(|e| jig_core::RepoContext::from_path(&e.path).ok())
                    .collect();
                let ctx = GlobalCtx { repos };
                command.run_global(&ctx)?
            } else {
                let repo = jig_core::RepoContext::from_cwd().ok();

                // Best-effort auto-registration and pruning of current repo
                if let Some(ref repo) = repo {
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

                let ctx = RepoCtx { repo };
                command.run(&ctx)?
            };
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
