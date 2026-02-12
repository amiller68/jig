//! Init command - initialize repository for jig

use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::path::Path;

use jig_core::{config, git, Error};

// Embed templates at compile time from the templates/ directory
const JIG_TOML_CONTENT: &str = r#"[spawn]
auto = true
"#;

const CLAUDE_MD_TEMPLATE: &str = include_str!("../../../../templates/CLAUDE.md");
const DOCS_INDEX_TEMPLATE: &str = include_str!("../../../../templates/docs/index.md");
const ISSUE_TRACKING_TEMPLATE: &str = include_str!("../../../../templates/docs/issue-tracking.md");

// Skills
const SKILL_CHECK: &str = include_str!("../../../../templates/skills/check/SKILL.md");
const SKILL_DRAFT: &str = include_str!("../../../../templates/skills/draft/SKILL.md");
const SKILL_ISSUES: &str = include_str!("../../../../templates/skills/issues/SKILL.md");
const SKILL_REVIEW: &str = include_str!("../../../../templates/skills/review/SKILL.md");
const SKILL_SPAWN: &str = include_str!("../../../../templates/skills/jig/SKILL.md");

const SETTINGS_JSON: &str = r#"{
  "permissions": {
    "allow": [
      "Bash(git *)",
      "Bash(gh *)",
      "Bash(cargo *)",
      "Bash(npm *)",
      "Bash(pnpm *)",
      "Bash(yarn *)",
      "Bash(make *)",
      "Bash(go *)",
      "Bash(python *)",
      "Bash(pytest *)",
      "Bash(uv *)",
      "Bash(bundle *)",
      "Bash(rake *)",
      "Bash(jig *)",
      "Bash(tmux *)",
      "Bash(ls *)",
      "Bash(pwd)",
      "Bash(which *)",
      "Bash(cat *)",
      "Bash(head *)",
      "Bash(tail *)",
      "Bash(wc *)",
      "Bash(find *)",
      "Bash(grep *)",
      "Bash(./test.sh *)"
    ],
    "deny": [
      "Bash(rm -rf /)",
      "Bash(rm -rf ~)",
      "Bash(sudo *)",
      "Bash(git push --force *)"
    ]
  }
}
"#;

pub fn run(force: bool, backup: bool, audit: bool) -> Result<()> {
    let repo = git::get_base_repo()?;

    // Check if already initialized
    if config::has_jig_toml()? && !force {
        return Err(Error::AlreadyInitialized.into());
    }

    eprintln!("{} Initializing jig in {}", "→".cyan(), repo.display());

    // Create directories
    let dirs = [
        "docs",
        "issues",
        ".claude/skills/check",
        ".claude/skills/draft",
        ".claude/skills/issues",
        ".claude/skills/review",
        ".claude/skills/spawn",
    ];
    for dir in dirs {
        let path = repo.join(dir);
        if !path.exists() {
            fs::create_dir_all(&path)?;
            eprintln!("  {} Created {}/", "✓".green(), dir);
        }
    }

    // Write jig.toml
    write_file(&repo.join("jig.toml"), JIG_TOML_CONTENT, force, backup)?;

    // Write CLAUDE.md
    write_file(&repo.join("CLAUDE.md"), CLAUDE_MD_TEMPLATE, force, backup)?;

    // Write docs files
    write_file(
        &repo.join("docs/index.md"),
        DOCS_INDEX_TEMPLATE,
        force,
        backup,
    )?;
    write_file(
        &repo.join("docs/issue-tracking.md"),
        ISSUE_TRACKING_TEMPLATE,
        force,
        backup,
    )?;

    // Write .claude/settings.json
    write_file(
        &repo.join(".claude/settings.json"),
        SETTINGS_JSON,
        force,
        backup,
    )?;

    // Write skills
    write_file(
        &repo.join(".claude/skills/check/SKILL.md"),
        SKILL_CHECK,
        force,
        backup,
    )?;
    write_file(
        &repo.join(".claude/skills/draft/SKILL.md"),
        SKILL_DRAFT,
        force,
        backup,
    )?;
    write_file(
        &repo.join(".claude/skills/issues/SKILL.md"),
        SKILL_ISSUES,
        force,
        backup,
    )?;
    write_file(
        &repo.join(".claude/skills/review/SKILL.md"),
        SKILL_REVIEW,
        force,
        backup,
    )?;
    write_file(
        &repo.join(".claude/skills/spawn/SKILL.md"),
        SKILL_SPAWN,
        force,
        backup,
    )?;

    eprintln!();
    eprintln!("{} Initialization complete", "✓".green().bold());

    if audit {
        eprintln!();
        eprintln!("{} Run this to audit and populate documentation:", "→".cyan());
        eprintln!();
        eprintln!("  claude \"Audit this codebase and populate CLAUDE.md and docs/index.md with project-specific content, then commit.\"");
    }

    Ok(())
}

fn write_file(path: &Path, content: &str, force: bool, backup: bool) -> Result<()> {
    let name = path.file_name().unwrap().to_string_lossy();

    if path.exists() {
        if backup {
            let backup_path = path.with_extension("bak");
            fs::copy(path, &backup_path)?;
            eprintln!("  {} Backed up {}", "→".dimmed(), name);
        }

        if !force {
            eprintln!("  {} Skipped {} (exists)", "→".dimmed(), name);
            return Ok(());
        }
    }

    fs::write(path, content)?;
    eprintln!("  {} Created {}", "✓".green(), name);
    Ok(())
}
