//! jig CLI - Git worktree manager for parallel Claude Code sessions

#[macro_use]
mod op;

mod cli;
mod commands;
mod ui;

use clap::Parser;

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
        ui::print_error(e.as_ref());
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Set global plain mode before any output
    ui::set_plain(cli.plain);

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
                                eprintln!("{} {}", ui::dim("pruned:"), p.display());
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
    eprintln!(
        "  {}  Run command across all tracked repos",
        ui::highlight("-g, --global")
    );
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
