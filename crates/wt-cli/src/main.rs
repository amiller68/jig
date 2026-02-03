//! jig CLI - Git worktree manager for parallel Claude Code sessions

mod cli;
mod commands;

use anyhow::Result;
use clap::Parser;
use colored::Colorize;

use cli::{Cli, Commands};

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

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            print_help();
            Ok(())
        }
        Some(Commands::Create { name, branch }) => {
            commands::create(&name, branch.as_deref(), cli.open, cli.no_hooks)
        }
        Some(Commands::List { all }) => commands::list(all),
        Some(Commands::Open { name, all }) => {
            if all {
                commands::open(None, true)
            } else {
                commands::open(name.as_deref(), false)
            }
        }
        Some(Commands::Remove { pattern, force }) => commands::remove(&pattern, force),
        Some(Commands::Exit { force }) => commands::exit(force),
        Some(Commands::Config { subcommand, list }) => commands::config(subcommand, list),
        Some(Commands::Spawn {
            name,
            context,
            auto,
        }) => commands::spawn(&name, context.as_deref(), auto),
        Some(Commands::Ps) => commands::ps(),
        Some(Commands::Attach { name }) => commands::attach(name.as_deref()),
        Some(Commands::Review { name, full }) => commands::review(&name, full),
        Some(Commands::Merge { name }) => commands::merge(&name),
        Some(Commands::Kill { name }) => commands::kill(&name),
        Some(Commands::Init {
            force,
            backup,
            audit,
        }) => commands::init(force, backup, audit),
        Some(Commands::Update { force }) => commands::update(force),
        Some(Commands::Version) => commands::version(),
        Some(Commands::Which) => commands::which(),
        Some(Commands::Health) => commands::health(),
        Some(Commands::Tui) => commands::tui(),
        Some(Commands::Status { name }) => commands::status(name.as_deref()),
        Some(Commands::ShellInit { shell }) => commands::shell_init(&shell),
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
    eprintln!("{}", "TUI:".bold());
    eprintln!("  {}         Launch the terminal UI", "tui".cyan());
    eprintln!();
    eprintln!("{}", "UTILITY:".bold());
    eprintln!("  {}        Initialize repository for jig", "init".cyan());
    eprintln!("  {}      Update jig to latest version", "update".cyan());
    eprintln!("  {}     Show version information", "version".cyan());
    eprintln!("  {}       Show path to jig executable", "which".cyan());
    eprintln!("  {}      Show terminal and dependency status", "health".cyan());
    eprintln!();
    eprintln!("{}", "GLOBAL OPTIONS:".bold());
    eprintln!("  {}            Open/cd into worktree after creating", "-o".cyan());
    eprintln!(
        "  {}    Skip on-create hook execution",
        "--no-hooks".cyan()
    );
    eprintln!();
    eprintln!(
        "Use '{}' for more information about a command.",
        "jig <command> --help".cyan()
    );
}
