//! Config command

use std::path::Path;

use clap::{Args, Subcommand};

use crate::context::{self, Config as GlobalConfig, JigToml, LinearIssuesConfig, RepoConfig, DEFAULT_BASE_BRANCH};
use crate::context::ContextError;

use crate::cli::op::Op;
use crate::cli::ui;

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
    Context(#[from] ContextError),
}

impl Op for Config {
    type Error = ConfigError;
    type Output = ConfigOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        if self.list {
            return show_list();
        }

        match &self.subcommand {
            None | Some(ConfigCommands::Show) => {
                match RepoConfig::from_cwd() {
                    Ok(repo) => show_config(&repo),
                    Err(_) => show_global_config(),
                }
            }
            Some(ConfigCommands::Base {
                branch,
                global,
                unset,
            }) => handle_base(branch.as_deref(), *global, *unset),
            Some(ConfigCommands::OnCreate { command, unset }) => {
                handle_on_create(command.as_deref(), *unset)
            }
        }
    }
}

fn show_global_config() -> Result<ConfigOutput, ConfigError> {
    let global = GlobalConfig::load()?;

    fn src(s: &str) -> String {
        ui::source(&format!("({})", s))
    }

    // -- Base branch --
    ui::header("Configuration");
    eprintln!();
    match &global.default_base_branch {
        Some(branch) => {
            eprintln!("  {} {}", ui::bold("Base branch:"), ui::highlight(branch));
        }
        None => {
            eprintln!("  {} {}", ui::bold("Base branch:"), ui::dim("(not set)"));
        }
    }

    // -- Spawn --
    eprintln!();
    ui::header("Spawn");
    eprintln!();
    eprintln!(
        "  {} {} {}",
        ui::dim("Max workers:"),
        ui::highlight(&global.max_concurrent_workers.to_string()),
        src("global config")
    );
    eprintln!(
        "  {} {} {}",
        ui::dim("Poll interval:"),
        ui::highlight(&format!("{}s", global.poll_interval)),
        src("global config")
    );

    // -- Health --
    eprintln!();
    ui::header("Health");
    eprintln!();
    eprintln!(
        "  {} {} {}",
        ui::dim("Silence threshold:"),
        ui::highlight(&format!("{}s", global.silence_threshold_seconds)),
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
        ui::highlight(&global.auto_cleanup_merged.to_string())
    );
    eprintln!(
        "  {} {}",
        ui::dim("Auto-cleanup closed:"),
        ui::highlight(&global.auto_cleanup_closed.to_string())
    );

    // -- Daemon --
    eprintln!();
    ui::header("Daemon");
    eprintln!();
    eprintln!(
        "  {} {}",
        ui::dim("Auto-recover:"),
        ui::highlight(&global.auto_recover.to_string())
    );
    eprintln!(
        "  {} {}",
        ui::dim("Tick interval:"),
        ui::highlight(&format!("{}s", global.tick_interval))
    );
    eprintln!(
        "  {} {}",
        ui::dim("Session prefix:"),
        ui::highlight(&global.session_prefix)
    );

    Ok(ConfigOutput(None))
}

fn show_config(repo: &RepoConfig) -> Result<ConfigOutput, ConfigError> {
    let display = ConfigDisplay::load(&repo.repo_root)?;

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
    if let Some(ref global) = display.global_base {
        eprintln!("    {} {}", src("global default"), global);
    }

    if display.has_local_overlay {
        eprintln!(
            "  {} {}",
            ui::bold("Local overlay:"),
            ui::highlight(crate::context::JIG_LOCAL_TOML)
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
        ui::highlight("linear"),
        src(&display.issues_source)
    );
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
        src(&display.spawn_source)
    );
    eprintln!(
        "  {} {} {}",
        ui::dim("Poll interval:"),
        ui::highlight(&format!("{}s", display.poll_interval)),
        src("global config")
    );

    // -- Health --
    eprintln!();
    ui::header("Health");
    eprintln!();
    eprintln!(
        "  {} {}",
        ui::dim("Silence threshold:"),
        ui::highlight(&format!("{}s", display.global.silence_threshold_seconds)),
    );

    Ok(ConfigOutput(None))
}

fn show_list() -> Result<ConfigOutput, ConfigError> {
    show_global_config()
}

fn handle_base(
    branch: Option<&str>,
    global: bool,
    unset: bool,
) -> Result<ConfigOutput, ConfigError> {
    if unset {
        if global {
            let mut global_cfg = GlobalConfig::load()?;
            global_cfg.default_base_branch = None;
            global_cfg.save()?;
            ui::success("Unset global base branch");
        } else {
            let repo = RepoConfig::from_cwd()?;
            context::update_local_toml(&repo.repo_root, "worktree", "base", None)?;
            ui::success("Unset repo base branch");
        }
        return Ok(ConfigOutput(None));
    }

    match branch {
        Some(b) => {
            if global {
                let mut global_cfg = GlobalConfig::load()?;
                global_cfg.default_base_branch = Some(b.to_string());
                global_cfg.save()?;
                ui::success(&format!("Set global base branch to '{}'", ui::highlight(b)));
            } else {
                let repo = RepoConfig::from_cwd()?;
                context::update_local_toml(&repo.repo_root, "worktree", "base", Some(b))?;
                ui::success(&format!("Set repo base branch to '{}'", ui::highlight(b)));
            }
            Ok(ConfigOutput(None))
        }
        None => {
            if global {
                let global_cfg = GlobalConfig::load()?;
                match global_cfg.default_base_branch {
                    Some(b) => Ok(ConfigOutput(Some(b))),
                    None => {
                        eprintln!("No global default set");
                        Ok(ConfigOutput(None))
                    }
                }
            } else {
                let cfg = crate::context::Context::from_cwd()?;
                let repo = cfg.repo()?;
                Ok(ConfigOutput(Some(repo.base_branch(&cfg.config).to_string())))
            }
        }
    }
}

fn handle_on_create(
    command: Option<&str>,
    unset: bool,
) -> Result<ConfigOutput, ConfigError> {
    let repo = RepoConfig::from_cwd()?;

    if unset {
        context::update_local_toml(&repo.repo_root, "worktree", "on_create", None)?;
        ui::success("Unset on-create hook");
        return Ok(ConfigOutput(None));
    }

    match command {
        Some(cmd) => {
            context::update_local_toml(&repo.repo_root, "worktree", "on_create", Some(cmd))?;
            ui::success(&format!("Set on-create hook to '{}'", ui::highlight(cmd)));
            Ok(ConfigOutput(None))
        }
        None => {
            let on_create = repo.repo.worktree.on_create.as_deref();
            match on_create {
                Some(cmd) => Ok(ConfigOutput(Some(cmd.to_string()))),
                None => {
                    eprintln!("No on-create hook set");
                    Ok(ConfigOutput(None))
                }
            }
        }
    }
}

struct ConfigDisplay {
    effective_base: String,
    toml_base: Option<String>,
    worktree_source: String,
    global_base: Option<String>,
    effective_on_create: Option<String>,
    toml_on_create: Option<String>,
    agent_type: String,
    agent_source: String,
    issues_source: String,
    linear: Option<LinearIssuesConfig>,
    auto_spawn_labels: Option<Vec<String>>,
    max_concurrent_workers: usize,
    spawn_source: String,
    poll_interval: u64,
    global: GlobalConfig,
    has_local_overlay: bool,
}

impl ConfigDisplay {
    fn load(repo_path: &Path) -> Result<Self, ContextError> {
        let jig_toml = JigToml::load(repo_path)?.unwrap_or_default();
        let global_config = GlobalConfig::load().unwrap_or_default();

        let worktree_source = jig_toml.source_label("worktree");
        let agent_source = jig_toml.source_label("agent");
        let issues_source = jig_toml.source_label("issues");
        let spawn_source = jig_toml.source_label("spawn");

        let effective_base = jig_toml
            .worktree
            .base
            .clone()
            .or_else(|| global_config.default_base_branch.clone())
            .unwrap_or_else(|| DEFAULT_BASE_BRANCH.to_string());

        Ok(Self {
            effective_base,
            toml_base: jig_toml.worktree.base,
            worktree_source,
            global_base: global_config.default_base_branch.clone(),
            effective_on_create: jig_toml.worktree.on_create.clone(),
            toml_on_create: jig_toml.worktree.on_create,
            agent_type: jig_toml.agent.agent_type,
            agent_source,
            issues_source,
            linear: jig_toml.issues.linear,
            auto_spawn_labels: jig_toml.issues.auto_spawn_labels,
            max_concurrent_workers: jig_toml.spawn.max_concurrent_workers,
            spawn_source,
            poll_interval: global_config.poll_interval,
            global: global_config,
            has_local_overlay: jig_toml.has_local_overlay,
        })
    }
}


