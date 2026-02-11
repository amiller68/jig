use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "jig",
    about = "Git worktree manager for parallel Claude Code sessions",
    version,
    after_help = "Use 'jig <command> --help' for more information about a command."
)]
pub struct Cli {
    /// Open/cd into worktree after creating
    #[arg(short = 'o', global = true)]
    pub open: bool,

    /// Skip on-create hook execution
    #[arg(long = "no-hooks", global = true)]
    pub no_hooks: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create a new worktree
    #[command(visible_alias = "c")]
    Create {
        /// Worktree name
        name: String,

        /// Branch name (defaults to worktree name)
        branch: Option<String>,
    },

    /// List worktrees
    #[command(visible_alias = "ls")]
    List {
        /// Show all git worktrees (including base repo)
        #[arg(long)]
        all: bool,
    },

    /// Open/cd into a worktree
    #[command(visible_alias = "o")]
    Open {
        /// Worktree name (or --all to open all in tabs)
        name: Option<String>,

        /// Open all worktrees in new tabs
        #[arg(long)]
        all: bool,
    },

    /// Remove worktree(s)
    #[command(visible_alias = "rm")]
    Remove {
        /// Worktree name or glob pattern
        pattern: String,

        /// Force removal even with uncommitted changes
        #[arg(long, short)]
        force: bool,
    },

    /// Exit current worktree and remove it
    Exit {
        /// Force removal even with uncommitted changes
        #[arg(long, short)]
        force: bool,
    },

    /// Manage configuration
    Config {
        #[command(subcommand)]
        subcommand: Option<ConfigCommands>,

        /// List all configuration
        #[arg(long)]
        list: bool,
    },

    /// Create worktree and launch Claude in tmux
    Spawn {
        /// Worktree name
        name: String,

        /// Task context/description
        #[arg(long, short)]
        context: Option<String>,

        /// Auto-start Claude with full prompt
        #[arg(long)]
        auto: bool,
    },

    /// Show status of spawned sessions
    Ps,

    /// Attach to tmux session
    Attach {
        /// Window name to switch to
        name: Option<String>,
    },

    /// Show diff for parent review
    Review {
        /// Worktree name
        name: String,

        /// Show full diff instead of summary
        #[arg(long)]
        full: bool,
    },

    /// Merge reviewed worktree into current branch
    Merge {
        /// Worktree name
        name: String,
    },

    /// Kill a running tmux window
    Kill {
        /// Worktree name
        name: String,
    },

    /// Initialize repository for jig
    Init {
        /// Reinitialize, overwriting existing files
        #[arg(long, short)]
        force: bool,

        /// Backup existing files before overwriting
        #[arg(long)]
        backup: bool,

        /// Run Claude audit to populate docs
        #[arg(long)]
        audit: bool,
    },

    /// Update jig to latest version
    Update {
        /// Force update, discarding local changes
        #[arg(long, short)]
        force: bool,
    },

    /// Show version information
    Version,

    /// Show path to jig executable
    Which,

    /// Show terminal and dependency status
    Health,

    /// Launch the terminal UI
    Tui,

    /// Show detailed worker status
    Status {
        /// Worker name
        name: Option<String>,
    },

    /// Print shell integration code
    #[command(name = "shell-init")]
    ShellInit {
        /// Shell type (bash, zsh, fish)
        shell: String,
    },

    /// Automatically configure shell integration
    #[command(name = "shell-setup")]
    ShellSetup {
        /// Show what would be done without making changes
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// Get or set base branch
    Base {
        /// Branch name to set
        branch: Option<String>,

        /// Use global default
        #[arg(long, short)]
        global: bool,

        /// Remove the setting
        #[arg(long)]
        unset: bool,
    },

    /// Get or set on-create hook
    OnCreate {
        /// Command to run
        command: Option<String>,

        /// Remove the hook
        #[arg(long)]
        unset: bool,
    },

    /// Show current configuration (default)
    Show,
}
