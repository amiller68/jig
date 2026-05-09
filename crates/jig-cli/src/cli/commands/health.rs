//! Health command - validate system dependencies and repo setup

use clap::Args;

use crate::context::JigToml;
use crate::terminal::check_dep;

use crate::cli::op::{NoOutput, Op};
use crate::context::Context;
use crate::cli::ui;

/// Show terminal and dependency status
#[derive(Args, Debug, Clone)]
pub struct Health;

#[derive(Debug, thiserror::Error)]
pub enum HealthError {
    #[error("Health check failed")]
    CheckFailed,
    #[error(transparent)]
    Context(#[from] crate::context::ContextError),
}

impl Op for Health {
    type Error = HealthError;
    type Output = NoOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        let version = env!("CARGO_PKG_VERSION");
        let mut all_passed = true;

        let check_ok = |name: &str, detail: Option<&str>| {
            if let Some(d) = detail {
                eprintln!("  {} {} {}", ui::SYM_OK, name, ui::dim(d));
            } else {
                eprintln!("  {} {}", ui::SYM_OK, name);
            }
        };
        let check_fail = |name: &str, note: Option<&str>| {
            if let Some(n) = note {
                eprintln!("  {} {} {}", ui::SYM_FAIL, name, ui::dim(n));
            } else {
                eprintln!("  {} {}", ui::SYM_FAIL, name);
            }
        };

        eprintln!("jig v{}", version);

        // Section 1: System dependencies
        eprintln!();
        ui::header("System");

        let git = check_dep("git", &["--version"]);
        if git.found {
            check_ok("git", git.version.as_deref());
        } else {
            check_fail("git", None);
            all_passed = false;
        }

        let tmux = check_dep("tmux", &["-V"]);
        if tmux.found {
            check_ok("tmux", tmux.version.as_deref());
        } else {
            check_fail("tmux", None);
            all_passed = false;
        }

        // Section 2: Repository
        eprintln!();
        let cfg = Context::from_cwd().ok();
        let global = cfg.as_ref().map(|c| &c.config);

        let repo = cfg.as_ref().and_then(|c| c.repo().ok());
        match repo {
            Some(repo) => {
                let repo_name = repo
                    .repo_root
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                ui::header(&format!("Repository: {}", repo_name));

                if JigToml::exists(&repo.repo_root) {
                    check_ok("jig.toml", None);
                } else {
                    check_fail("jig.toml", Some("(not found)"));
                    all_passed = false;
                }

                let global_ref = global.cloned().unwrap_or_default();
                let branch = repo.base_branch(&global_ref);
                eprintln!("  {} Base branch: {}", ui::SYM_OK, branch);

                if repo.worktrees_path.is_dir() {
                    check_ok(&format!("{} directory", crate::context::JIG_DIR), None);
                } else {
                    check_fail(
                        &format!("{} directory", crate::context::JIG_DIR),
                        Some("(not found)"),
                    );
                    all_passed = false;
                }

                // Section 3: Agent — init from config, use its health + scaffolding
                eprintln!();
                let jig_config = JigToml::load(&repo.repo_root)
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let agent = jig_core::agents::Agent::from_config(
                    &jig_config.agent.agent_type,
                    Some(&jig_config.agent.model),
                    &jig_config.agent.disallowed_tools,
                );

                match agent {
                    Some(agent) => {
                        ui::header(&format!("Agent: {}", agent.name()));

                        match agent.health() {
                            Ok(ver) => check_ok(agent.command(), Some(&ver)),
                            Err(_) => {
                                check_fail(agent.command(), Some("(not found or broken)"));
                                all_passed = false;
                            }
                        }

                        let project_file = agent.project_file();
                        if repo.repo_root.join(project_file).is_file() {
                            check_ok(&project_file.display().to_string(), None);
                        } else {
                            check_fail(
                                &project_file.display().to_string(),
                                Some("(not found)"),
                            );
                            all_passed = false;
                        }

                        if let Some(settings) = agent.settings_file() {
                            if repo.repo_root.join(settings).is_file() {
                                check_ok(&settings.display().to_string(), None);
                            } else {
                                check_fail(
                                    &settings.display().to_string(),
                                    Some("(not found)"),
                                );
                                all_passed = false;
                            }
                        }

                        let skills_dir = repo.repo_root.join(agent.skills_dir());
                        if skills_dir.is_dir() {
                            eprintln!("  Skills ({}):", agent.skills_dir().display());
                            if let Ok(entries) = std::fs::read_dir(&skills_dir) {
                                let mut found_any = false;
                                let mut skill_names: Vec<String> = entries
                                    .filter_map(|e| e.ok())
                                    .filter(|e| e.path().is_dir())
                                    .filter(|e| {
                                        e.path().join(agent.skill_file()).is_file()
                                    })
                                    .map(|e| {
                                        e.file_name().to_string_lossy().to_string()
                                    })
                                    .collect();
                                skill_names.sort();
                                for name in &skill_names {
                                    eprintln!("    {} {}", ui::SYM_OK, name);
                                    found_any = true;
                                }
                                if !found_any {
                                    eprintln!("    {} (none found)", ui::SYM_FAIL);
                                    all_passed = false;
                                }
                            }
                        } else {
                            check_fail(
                                &format!("{} directory", agent.skills_dir().display()),
                                Some("(not found)"),
                            );
                            all_passed = false;
                        }
                    }
                    None => {
                        ui::header("Agent");
                        check_fail("agent", Some("(unknown agent type in config)"));
                        all_passed = false;
                    }
                }
            }
            None => {
                ui::header("Repository:");
                eprintln!("  {} Not in a git repository", ui::SYM_FAIL);

                eprintln!();
                ui::header("Agent");
                eprintln!("  {} Skipped (no repository)", ui::SYM_FAIL);
                all_passed = false;
            }
        }

        eprintln!();
        if all_passed {
            eprintln!("All checks passed.");
            Ok(NoOutput)
        } else {
            eprintln!(
                "Run '{}' to set up this repository.",
                ui::highlight("jig init")
            );
            Err(HealthError::CheckFailed)
        }
    }
}
