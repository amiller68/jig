//! jig CLI - Git worktree manager for parallel Claude Code sessions

mod cli;
pub mod context;
pub mod daemon;
pub mod hooks;
pub mod notify;
pub mod terminal;
pub mod worker;

use std::io::IsTerminal;

use clap::Parser;

use cli::op::Op;
use cli::ui;
use cli::Cli;

fn main() {
    if let Err(e) = run() {
        ui::print_error(e.as_ref());
        std::process::exit(1);
    }
}

fn init_tracing(log_file: Option<std::path::PathBuf>) {
    use tracing_subscriber::prelude::*;

    let default_level = if log_file.is_some() { "info" } else { "warn" };
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level));

    let stderr_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);

    let file_layer = log_file.and_then(|path| {
        std::fs::File::create(&path).ok().map(|file| {
            tracing_subscriber::fmt::layer()
                .with_writer(std::sync::Mutex::new(file))
                .with_ansi(false)
        })
    });

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stderr_layer)
        .with(file_layer)
        .init();
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Set global plain mode before any output
    ui::set_plain(cli.plain);

    // The `colored` crate checks stdout for TTY detection, but all jig
    // output goes to stderr. Override colorization based on stderr instead.
    if !cli.plain && std::io::stderr().is_terminal() {
        colored::control::set_override(true);
    }

    // Best-effort global directory setup
    let _ = context::ensure_global_dirs();

    // Every command gets a session log file
    let log_file = context::new_daemon_log_path().ok();
    init_tracing(log_file);

    match cli.command {
        None => {
            print_help();
            Ok(())
        }
        Some(ref command) => {
            let output = command.run()?;
            let output_str = output.to_string();
            if !output_str.is_empty() {
                println!("{}", output_str);
            }
            Ok(())
        }
    }
}

fn print_help() {
    eprintln!("{}", ui::bold("jig - Git worktree manager"));
    eprintln!();
    eprintln!("{}", ui::bold("USAGE:"));
    eprintln!("  jig <COMMAND> [OPTIONS]");
    eprintln!();
    eprintln!("{}", ui::bold("WORKTREE COMMANDS:"));
    eprintln!("  {}      Create a new worktree", ui::highlight("create"));
    eprintln!("  {}        List worktrees", ui::highlight("list"));
    eprintln!("  {}        Open/cd into a worktree", ui::highlight("open"));
    eprintln!("  {}      Remove worktree(s)", ui::highlight("remove"));
    eprintln!("  {}        Exit current worktree", ui::highlight("exit"));
    eprintln!();
    eprintln!("{}", ui::bold("CONFIGURATION:"));
    eprintln!("  {}      Manage configuration", ui::highlight("config"));
    eprintln!();
    eprintln!("{}", ui::bold("WORKER COMMANDS:"));
    eprintln!(
        "  {}       Create worktree + launch Claude in tmux",
        ui::highlight("spawn")
    );
    eprintln!(
        "  {}          Show status of spawned workers",
        ui::highlight("ps")
    );
    eprintln!("  {}      Attach to tmux session", ui::highlight("attach"));
    eprintln!(
        "  {}      Show diff for parent review",
        ui::highlight("review")
    );
    eprintln!(
        "  {}       Merge reviewed worktree into current branch",
        ui::highlight("merge")
    );
    eprintln!(
        "  {}        Kill a running tmux window",
        ui::highlight("kill")
    );
    eprintln!(
        "  {}        Nuke all workers and state (keeps config)",
        ui::highlight("nuke")
    );
    eprintln!();
    eprintln!("{}", ui::bold("ISSUES:"));
    eprintln!(
        "  {}      Browse and filter issues",
        ui::highlight("issues")
    );
    eprintln!();
    eprintln!("{}", ui::bold("REPOSITORY TRACKING:"));
    eprintln!(
        "  {}       List tracked repositories",
        ui::highlight("repos")
    );
    eprintln!();
    eprintln!("{}", ui::bold("UTILITY:"));
    eprintln!(
        "  {}        Initialize repository for jig",
        ui::highlight("init")
    );
    eprintln!(
        "  {}      Update jig to latest version",
        ui::highlight("update")
    );
    eprintln!(
        "  {}     Show version information",
        ui::highlight("version")
    );
    eprintln!(
        "  {}       Show path to jig executable",
        ui::highlight("which")
    );
    eprintln!(
        "  {}      Show terminal and dependency status",
        ui::highlight("health")
    );
    eprintln!(
        "  {} Configure shell integration",
        ui::highlight("shell-setup")
    );
    eprintln!();
    eprintln!("{}", ui::bold("GLOBAL OPTIONS:"));
    eprintln!("  {} Show verbose output", ui::highlight("-v, --verbose"));
    eprintln!(
        "  {}    Plain output for scripting",
        ui::highlight("--plain")
    );
    eprintln!();
    eprintln!(
        "Use '{}' for more information about a command.",
        ui::highlight("jig <command> --help")
    );
}
