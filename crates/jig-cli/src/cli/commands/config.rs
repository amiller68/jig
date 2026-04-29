//! Config command

use std::path::Path;

use clap::{Args, Subcommand};

use crate::config::{
    GlobalConfig, JigToml, LinearIssuesConfig, ResolvedNudgeConfig, DEFAULT_BASE_BRANCH,
};
use jig_core::Error as CoreError;

use crate::cli::op::{GlobalCtx, Op, RepoCtx};
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
                if ctx.config.is_some() {
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
        let repo_ctx = RepoCtx { config: None };

        match &self.subcommand {
            None | Some(ConfigCommands::Show) => show_global_config(),
            Some(ConfigCommands::Base { branch, unset, .. }) => {
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
    let global = crate::config::GlobalConfig::load()?;

    /// Format a source attribution tag.
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
    let cfg = ctx.config()?;
    let display = ConfigDisplay::load(&cfg.repo_root)?;

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
    if let Some(ref global) = display.global_base {
        eprintln!("    {} {}", src("global default"), global);
    }

    if display.has_local_overlay {
        eprintln!(
            "  {} {}",
            ui::bold("Local overlay:"),
            ui::highlight(crate::config::JIG_LOCAL_TOML)
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
        ui::highlight(&display.issues_provider.to_string()),
        src(&display.issues_source)
    );
    if display.issues_provider == jig_core::issues::ProviderKind::Linear {
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
    show_global_config()
}

fn handle_base(
    ctx: &RepoCtx,
    branch: Option<&str>,
    global: bool,
    unset: bool,
) -> Result<ConfigOutput, ConfigError> {
    if unset {
        if global {
            let mut global_cfg = crate::config::GlobalConfig::load()?;
            global_cfg.default_base_branch = None;
            global_cfg.save()?;
            ui::success("Unset global base branch");
        } else {
            let cfg = ctx.config()?;
            update_local_toml(&cfg.repo_root, "worktree", "base", None)?;
            ui::success("Unset repo base branch");
        }
        return Ok(ConfigOutput(None));
    }

    match branch {
        Some(b) => {
            if global {
                let mut global_cfg = crate::config::GlobalConfig::load()?;
                global_cfg.default_base_branch = Some(b.to_string());
                global_cfg.save()?;
                ui::success(&format!("Set global base branch to '{}'", ui::highlight(b)));
            } else {
                let cfg = ctx.config()?;
                update_local_toml(&cfg.repo_root, "worktree", "base", Some(b))?;
                ui::success(&format!("Set repo base branch to '{}'", ui::highlight(b)));
            }
            Ok(ConfigOutput(None))
        }
        None => {
            if global {
                let global_cfg = crate::config::GlobalConfig::load()?;
                match global_cfg.default_base_branch {
                    Some(b) => Ok(ConfigOutput(Some(b))),
                    None => {
                        eprintln!("No global default set");
                        Ok(ConfigOutput(None))
                    }
                }
            } else {
                let cfg = ctx.config()?;
                Ok(ConfigOutput(Some(cfg.base_branch())))
            }
        }
    }
}

fn handle_on_create(
    ctx: &RepoCtx,
    command: Option<&str>,
    unset: bool,
) -> Result<ConfigOutput, ConfigError> {
    let cfg = ctx.config()?;

    if unset {
        update_local_toml(&cfg.repo_root, "worktree", "on_create", None)?;
        ui::success("Unset on-create hook");
        return Ok(ConfigOutput(None));
    }

    match command {
        Some(cmd) => {
            update_local_toml(&cfg.repo_root, "worktree", "on_create", Some(cmd))?;
            ui::success(&format!("Set on-create hook to '{}'", ui::highlight(cmd)));
            Ok(ConfigOutput(None))
        }
        None => {
            let on_create = cfg.repo.worktree.on_create.as_deref();
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
    issues_provider: jig_core::issues::ProviderKind,
    issues_directory: String,
    issues_source: String,
    linear: Option<LinearIssuesConfig>,
    auto_spawn_labels: Option<Vec<String>>,
    max_concurrent_workers: usize,
    max_concurrent_workers_source: String,
    auto_spawn_interval: u64,
    auto_spawn_interval_source: String,
    silence_threshold_seconds: u64,
    silence_threshold_source: String,
    max_nudges: u32,
    max_nudges_source: String,
    nudge_type_configs: Vec<(String, ResolvedNudgeConfig, String)>,
    has_local_overlay: bool,
}

impl ConfigDisplay {
    fn load(repo_path: &Path) -> jig_core::Result<Self> {
        let jig_toml = JigToml::load(repo_path)?.unwrap_or_default();
        let global_config = GlobalConfig::load().unwrap_or_default();
        let has_local_overlay = jig_toml.has_local_overlay;

        let worktree_source = jig_toml.source_label("worktree");
        let agent_source = jig_toml.source_label("agent");
        let issues_source = jig_toml.source_label("issues");

        let effective_base = jig_toml
            .worktree
            .base
            .clone()
            .or_else(|| global_config.default_base_branch.clone())
            .unwrap_or_else(|| DEFAULT_BASE_BRANCH.to_string());

        let effective_on_create = jig_toml.worktree.on_create.clone();

        let global_spawn = &global_config.spawn;
        let spawn = &jig_toml.spawn;

        let spawn_source = jig_toml.source_label("spawn");
        let (max_concurrent_workers, max_concurrent_workers_source) =
            if spawn.max_concurrent_workers.is_some() {
                (
                    spawn.resolve_max_concurrent_workers(global_spawn),
                    spawn_source.clone(),
                )
            } else {
                (
                    global_spawn.max_concurrent_workers,
                    "global config".to_string(),
                )
            };

        let (auto_spawn_interval, auto_spawn_interval_source) =
            if spawn.auto_spawn_interval.is_some() {
                (
                    spawn.resolve_auto_spawn_interval(global_spawn),
                    spawn_source,
                )
            } else {
                (
                    global_spawn.auto_spawn_interval,
                    "global config".to_string(),
                )
            };

        let health = &jig_toml.health;
        let global_health = &global_config.health;
        let health_source = jig_toml.source_label("health");

        let (silence_threshold_seconds, silence_threshold_source) =
            if health.silence_threshold_seconds.is_some() {
                (
                    health.resolve_silence_threshold(global_health),
                    health_source.clone(),
                )
            } else {
                (
                    global_health.silence_threshold_seconds,
                    "global config".to_string(),
                )
            };

        let (max_nudges, max_nudges_source) = if health.max_nudges.is_some() {
            (
                health.resolve_max_nudges(global_health),
                health_source.clone(),
            )
        } else {
            (global_health.max_nudges, "global config".to_string())
        };

        let nudge_types = ["idle", "stalled", "ci", "review", "conflict", "bad_commits"];
        let nudge_type_configs: Vec<(String, ResolvedNudgeConfig, String)> = nudge_types
            .iter()
            .map(|&nt| {
                let resolved = health.resolve_for_nudge_type(nt, global_health);
                let type_cfg = match nt {
                    "idle" => &health.nudge.idle,
                    "stalled" => &health.nudge.stalled,
                    "ci" => &health.nudge.ci,
                    "review" => &health.nudge.review,
                    "conflict" => &health.nudge.conflict,
                    "bad_commits" => &health.nudge.bad_commits,
                    _ => &None,
                };
                let source = if type_cfg.is_some() {
                    format!("{} [health.nudge]", health_source)
                } else if health.max_nudges.is_some() || health.silence_threshold_seconds.is_some()
                {
                    format!("{} [health]", health_source)
                } else {
                    "global config".to_string()
                };
                (nt.to_string(), resolved, source)
            })
            .collect();

        Ok(Self {
            effective_base,
            toml_base: jig_toml.worktree.base,
            worktree_source,
            global_base: global_config.default_base_branch,
            effective_on_create,
            toml_on_create: jig_toml.worktree.on_create,
            agent_type: jig_toml.agent.agent_type,
            agent_source,
            issues_provider: jig_toml.issues.provider,
            issues_directory: jig_toml.issues.directory,
            issues_source,
            linear: jig_toml.issues.linear,
            auto_spawn_labels: jig_toml.issues.auto_spawn_labels,
            max_concurrent_workers,
            max_concurrent_workers_source,
            auto_spawn_interval,
            auto_spawn_interval_source,
            silence_threshold_seconds,
            silence_threshold_source,
            max_nudges,
            max_nudges_source,
            nudge_type_configs,
            has_local_overlay,
        })
    }
}

/// Update a key in jig.local.toml. Pass `None` as value to remove the key.
fn update_local_toml(
    repo_root: &std::path::Path,
    section: &str,
    key: &str,
    value: Option<&str>,
) -> Result<(), ConfigError> {
    let local_path = repo_root.join(crate::config::JIG_LOCAL_TOML);
    let mut doc: toml::Value = if local_path.exists() {
        let content = std::fs::read_to_string(&local_path).map_err(CoreError::Io)?;
        toml::from_str(&content).map_err(|e| CoreError::Custom(e.to_string()))?
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    let table = doc.as_table_mut().unwrap();

    match value {
        Some(v) => {
            let section_table = table
                .entry(section)
                .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
                .as_table_mut()
                .ok_or_else(|| CoreError::Custom(format!("[{}] is not a table", section)))?;
            section_table.insert(key.to_string(), toml::Value::String(v.to_string()));
        }
        None => {
            if let Some(section_val) = table.get_mut(section) {
                if let Some(section_table) = section_val.as_table_mut() {
                    section_table.remove(key);
                    if section_table.is_empty() {
                        table.remove(section);
                    }
                }
            }
        }
    }

    let content = toml::to_string_pretty(&doc).map_err(|e| CoreError::Custom(e.to_string()))?;
    std::fs::write(&local_path, content).map_err(CoreError::Io)?;

    Ok(())
}
