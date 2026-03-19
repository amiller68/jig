//! Config command

use clap::{Args, Subcommand};

use jig_core::config::{self, ConfigDisplay};
use jig_core::Error as CoreError;

use crate::op::{GlobalCtx, Op, RepoCtx};
use crate::ui;

/// Manage configuration
#[derive(Args, Debug, Clone)]
pub struct Config {
    #[command(subcommand)]
    pub subcommand: Option<ConfigCommands>,

    /// List all configuration
    #[arg(long)]
    pub list: bool,
}

#[derive(Subcommand, Debug, Clone)]
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

/// Output for config commands (may output to stdout for get operations)
#[derive(Debug)]
pub struct ConfigOutput(Option<String>);

impl std::fmt::Display for ConfigOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref value) = self.0 {
            write!(f, "{}", value)?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error(transparent)]
    Core(#[from] CoreError),
}

impl Op for Config {
    type Error = ConfigError;
    type Output = ConfigOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        if self.list {
            return show_list();
        }

        match &self.subcommand {
            None | Some(ConfigCommands::Show) => {
                if ctx.repo.is_some() {
                    show_config(ctx)
                } else {
                    show_global_config()
                }
            }
            Some(ConfigCommands::Base {
                branch,
                global,
                unset,
            }) => handle_base(ctx, branch.as_deref(), *global, *unset),
            Some(ConfigCommands::OnCreate { command, unset }) => {
                handle_on_create(ctx, command.as_deref(), *unset)
            }
        }
    }

    fn run_global(&self, _ctx: &GlobalCtx) -> Result<Self::Output, Self::Error> {
        if self.list {
            return show_list();
        }

        // In global mode, create a repo-less context for subcommands that need it
        let repo_ctx = RepoCtx { repo: None };

        match &self.subcommand {
            None | Some(ConfigCommands::Show) => show_global_config(),
            Some(ConfigCommands::Base {
                branch,
                unset,
                ..
            }) => {
                // Force global=true in -g mode
                handle_base(&repo_ctx, branch.as_deref(), true, *unset)
            }
            Some(ConfigCommands::OnCreate { .. }) => {
                eprintln!("error: on-create hook is repo-specific, cannot use with -g/--global");
                std::process::exit(1);
            }
        }
    }
}

fn show_global_config() -> Result<ConfigOutput, ConfigError> {
    let global = jig_core::global::config::GlobalConfig::load()?;

    /// Format a source attribution tag.
    fn src(s: &str) -> String {
        ui::source(&format!("({})", s))
    }

    // -- Base branch --
    ui::header("Configuration");
    eprintln!();
    match config::get_global_base_branch()? {
        Some(branch) => {
            eprintln!(
                "  {} {}",
                ui::bold("Base branch:"),
                ui::highlight(&branch)
            );
        }
        None => {
            eprintln!(
                "  {} {}",
                ui::bold("Base branch:"),
                ui::dim("(not set)")
            );
        }
    }

    // -- Spawn --
    eprintln!();
    ui::header("Spawn");
    eprintln!();
    eprintln!(
        "  {} {} {}",
        ui::dim("Max workers:"),
        ui::highlight(&global.spawn.max_concurrent_workers.to_string()),
        src("global config")
    );
    eprintln!(
        "  {} {} {}",
        ui::dim("Poll interval:"),
        ui::highlight(&format!("{}s", global.spawn.auto_spawn_interval)),
        src("global config")
    );

    // -- Health --
    eprintln!();
    ui::header("Health");
    eprintln!();
    eprintln!(
        "  {} {} {}",
        ui::dim("Silence threshold:"),
        ui::highlight(&format!("{}s", global.health.silence_threshold_seconds)),
        src("global config")
    );
    eprintln!(
        "  {} {} {}",
        ui::dim("Max nudges:"),
        ui::highlight(&global.health.max_nudges.to_string()),
        src("global config")
    );

    // -- Notify --
    if global.notify.exec.is_some() || global.notify.webhook.is_some() {
        eprintln!();
        ui::header("Notify");
        eprintln!();
        if let Some(ref exec) = global.notify.exec {
            eprintln!("  {} {}", ui::dim("Exec:"), ui::highlight(exec));
        }
        if let Some(ref webhook) = global.notify.webhook {
            eprintln!("  {} {}", ui::dim("Webhook:"), ui::highlight(webhook));
        }
        if !global.notify.events.is_empty() {
            eprintln!(
                "  {} {}",
                ui::dim("Events:"),
                ui::highlight(&global.notify.events.join(", "))
            );
        }
    }

    // -- GitHub --
    eprintln!();
    ui::header("GitHub");
    eprintln!();
    eprintln!(
        "  {} {}",
        ui::dim("Auto-cleanup merged:"),
        ui::highlight(&global.github.auto_cleanup_merged.to_string())
    );
    eprintln!(
        "  {} {}",
        ui::dim("Auto-cleanup closed:"),
        ui::highlight(&global.github.auto_cleanup_closed.to_string())
    );

    // -- Daemon --
    eprintln!();
    ui::header("Daemon");
    eprintln!();
    eprintln!(
        "  {} {}",
        ui::dim("Auto-recover:"),
        ui::highlight(&global.daemon.auto_recover.to_string())
    );
    eprintln!(
        "  {} {}",
        ui::dim("Tick interval:"),
        ui::highlight(&format!("{}s", global.daemon.interval_seconds))
    );
    eprintln!(
        "  {} {}",
        ui::dim("Session prefix:"),
        ui::highlight(&global.daemon.session_prefix)
    );

    Ok(ConfigOutput(None))
}

fn show_config(ctx: &RepoCtx) -> Result<ConfigOutput, ConfigError> {
    let repo = ctx.repo()?;
    let display = ConfigDisplay::load(&repo.repo_root)?;

    /// Format a source attribution tag.
    fn src(s: &str) -> String {
        ui::source(&format!("({})", s))
    }

    // -- Configuration --
    ui::header("Configuration");
    eprintln!();
    eprintln!(
        "  {} {}",
        ui::bold("Base branch:"),
        ui::highlight(&display.effective_base)
    );
    if let Some(ref toml) = display.toml_base {
        eprintln!("    {} {}", src(&display.worktree_source), toml);
    }
    if let Some(ref repo_base) = display.repo_base {
        eprintln!("    {} {}", src("global config"), repo_base);
    }
    if let Some(ref global) = display.global_base {
        eprintln!("    {} {}", src("global default"), global);
    }

    if display.has_local_overlay {
        eprintln!(
            "  {} {}",
            ui::bold("Local overlay:"),
            ui::highlight(jig_core::config::JIG_LOCAL_TOML)
        );
    }

    if let Some(ref hook) = display.effective_on_create {
        eprintln!(
            "  {} {} {}",
            ui::bold("On-create hook:"),
            ui::highlight(hook),
            if display.toml_on_create.is_some() {
                src(&display.worktree_source)
            } else {
                src("global config")
            }
        );
    }

    // -- Agent --
    eprintln!();
    ui::header("Agent");
    eprintln!();
    eprintln!(
        "  {} {} {}",
        ui::dim("Type:"),
        ui::highlight(&display.agent_type),
        src(&display.agent_source)
    );

    // -- Issues --
    eprintln!();
    ui::header("Issues");
    eprintln!();
    eprintln!(
        "  {} {} {}",
        ui::dim("Provider:"),
        ui::highlight(&display.issues_provider),
        src(&display.issues_source)
    );
    if display.issues_provider == "linear" {
        if let Some(ref linear) = display.linear {
            eprintln!(
                "  {} {} {}",
                ui::dim("Profile:"),
                ui::highlight(&linear.profile),
                src(&display.issues_source)
            );
            if let Some(ref team) = linear.team {
                eprintln!(
                    "  {} {} {}",
                    ui::dim("Team:"),
                    ui::highlight(team),
                    src(&display.issues_source)
                );
            }
            if !linear.projects.is_empty() {
                eprintln!(
                    "  {} {} {}",
                    ui::dim("Projects:"),
                    ui::highlight(&linear.projects.join(", ")),
                    src(&display.issues_source)
                );
            }
            if let Some(ref assignee) = linear.assignee {
                eprintln!(
                    "  {} {} {}",
                    ui::dim("Assignee:"),
                    ui::highlight(assignee),
                    src(&display.issues_source)
                );
            }
            if !linear.labels.is_empty() {
                eprintln!(
                    "  {} {} {}",
                    ui::dim("Labels:"),
                    ui::highlight(&linear.labels.join(", ")),
                    src(&display.issues_source)
                );
            }
        }
    } else {
        eprintln!(
            "  {} {} {}",
            ui::dim("Directory:"),
            ui::highlight(&display.issues_directory),
            src(&display.issues_source)
        );
    }
    match &display.auto_spawn_labels {
        None => {
            eprintln!("  {} {}", ui::dim("Auto-spawn:"), ui::warn_text("disabled"));
        }
        Some(labels) if labels.is_empty() => {
            eprintln!(
                "  {} {} {}",
                ui::dim("Auto-spawn:"),
                ui::highlight("all issues"),
                src(&display.issues_source)
            );
        }
        Some(labels) => {
            eprintln!(
                "  {} {} {}",
                ui::dim("Auto-spawn:"),
                ui::highlight(&labels.join(", ")),
                src(&display.issues_source)
            );
        }
    }

    // -- Spawn --
    eprintln!();
    ui::header("Spawn");
    eprintln!();
    eprintln!(
        "  {} {} {}",
        ui::dim("Max workers:"),
        ui::highlight(&display.max_concurrent_workers.to_string()),
        src(&display.max_concurrent_workers_source)
    );
    eprintln!(
        "  {} {} {}",
        ui::dim("Poll interval:"),
        ui::highlight(&format!("{}s", display.auto_spawn_interval)),
        src(&display.auto_spawn_interval_source)
    );

    // -- Health --
    eprintln!();
    ui::header("Health");
    eprintln!();
    eprintln!(
        "  {} {} {}",
        ui::dim("Silence threshold:"),
        ui::highlight(&format!("{}s", display.silence_threshold_seconds)),
        src(&display.silence_threshold_source)
    );
    eprintln!(
        "  {} {} {}",
        ui::dim("Max nudges:"),
        ui::highlight(&display.max_nudges.to_string()),
        src(&display.max_nudges_source)
    );
    eprintln!();
    eprintln!("  {}", ui::dim("Per-type:"));
    for (name, resolved, source) in &display.nudge_type_configs {
        eprintln!(
            "    {} max={} cooldown={}s {}",
            ui::highlight(name),
            ui::bold(&resolved.max.to_string()),
            ui::bold(&resolved.cooldown_seconds.to_string()),
            src(source)
        );
    }

    Ok(ConfigOutput(None))
}

fn show_list() -> Result<ConfigOutput, ConfigError> {
    let entries = config::list_all_config()?;

    if entries.is_empty() {
        eprintln!("No configuration set");
        return Ok(ConfigOutput(None));
    }

    for (category, key, value) in entries {
        eprintln!("{} {} = {}", ui::dim(&category), ui::highlight(&key), value);
    }

    Ok(ConfigOutput(None))
}

fn handle_base(
    ctx: &RepoCtx,
    branch: Option<&str>,
    global: bool,
    unset: bool,
) -> Result<ConfigOutput, ConfigError> {
    if unset {
        if global {
            config::unset_global_base_branch()?;
            ui::success("Unset global base branch");
        } else {
            let repo = ctx.repo()?;
            config::unset_repo_base_branch(&repo.repo_root)?;
            ui::success("Unset repo base branch");
        }
        return Ok(ConfigOutput(None));
    }

    match branch {
        Some(b) => {
            if global {
                config::set_global_base_branch(b)?;
                ui::success(&format!("Set global base branch to '{}'", ui::highlight(b)));
            } else {
                let repo = ctx.repo()?;
                config::set_repo_base_branch(&repo.repo_root, b)?;
                ui::success(&format!("Set repo base branch to '{}'", ui::highlight(b)));
            }
            Ok(ConfigOutput(None))
        }
        None => {
            // Get/show current value
            if global {
                match config::get_global_base_branch()? {
                    Some(b) => Ok(ConfigOutput(Some(b))),
                    None => {
                        eprintln!("No global default set");
                        Ok(ConfigOutput(None))
                    }
                }
            } else {
                let repo = ctx.repo()?;
                match config::get_repo_base_branch(&repo.repo_root)? {
                    Some(b) => Ok(ConfigOutput(Some(b))),
                    None => {
                        eprintln!("No config set for this repo");
                        Ok(ConfigOutput(None))
                    }
                }
            }
        }
    }
}

fn handle_on_create(
    ctx: &RepoCtx,
    command: Option<&str>,
    unset: bool,
) -> Result<ConfigOutput, ConfigError> {
    let repo = ctx.repo()?;

    if unset {
        config::unset_on_create_hook(&repo.repo_root)?;
        ui::success("Unset on-create hook");
        return Ok(ConfigOutput(None));
    }

    match command {
        Some(cmd) => {
            config::set_on_create_hook(&repo.repo_root, cmd)?;
            ui::success(&format!("Set on-create hook to '{}'", ui::highlight(cmd)));
            Ok(ConfigOutput(None))
        }
        None => match config::get_on_create_hook(&repo.repo_root)? {
            Some(cmd) => Ok(ConfigOutput(Some(cmd))),
            None => {
                eprintln!("No on-create hook set");
                Ok(ConfigOutput(None))
            }
        },
    }
}
