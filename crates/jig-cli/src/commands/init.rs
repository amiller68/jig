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

// Docs templates
const DOCS_INDEX: &str = include_str!("../../../../templates/docs/index.md");
const DOCS_PATTERNS: &str = include_str!("../../../../templates/docs/PATTERNS.md");
const DOCS_CONTRIBUTING: &str = include_str!("../../../../templates/docs/CONTRIBUTING.md");
const DOCS_SUCCESS_CRITERIA: &str = include_str!("../../../../templates/docs/SUCCESS_CRITERIA.md");
const DOCS_PROJECT_LAYOUT: &str = include_str!("../../../../templates/docs/PROJECT_LAYOUT.md");

// Issues templates
const ISSUES_README: &str = include_str!("../../../../templates/issues/README.md");
const ISSUES_TEMPLATE: &str = include_str!("../../../../templates/issues/_template.md");

// Skills
const SKILL_CHECK: &str = include_str!("../../../../templates/skills/check/SKILL.md");
const SKILL_DRAFT: &str = include_str!("../../../../templates/skills/draft/SKILL.md");
const SKILL_ISSUES: &str = include_str!("../../../../templates/skills/issues/SKILL.md");
const SKILL_REVIEW: &str = include_str!("../../../../templates/skills/review/SKILL.md");
const SKILL_SPAWN: &str = include_str!("../../../../templates/skills/spawn/SKILL.md");

const SETTINGS_JSON: &str = r#"{
  "$schema": "https://claude.ai/schemas/claude-settings.json",
  "permissions": {
    "allow": [
      "Bash(git status)",
      "Bash(git log:*)",
      "Bash(git diff:*)",
      "Bash(git branch:*)",
      "Bash(git add:*)",
      "Bash(git commit:*)",
      "Bash(git push:*)",
      "Bash(git pull:*)",
      "Bash(git checkout:*)",
      "Bash(git switch:*)",
      "Bash(git fetch:*)",
      "Bash(git stash:*)",
      "Bash(gh pr view:*)",
      "Bash(gh pr list:*)",
      "Bash(gh pr create:*)",
      "Bash(gh pr checkout:*)",
      "Bash(gh issue:*)",
      "Bash(gh repo view:*)",
      "Bash(cargo *)",
      "Bash(npm *)",
      "Bash(pnpm *)",
      "Bash(yarn *)",
      "Bash(bun *)",
      "Bash(make *)",
      "Bash(go *)",
      "Bash(python *)",
      "Bash(python3 *)",
      "Bash(pytest *)",
      "Bash(uv *)",
      "Bash(pip *)",
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
    "ask": [
      "Bash(git merge:*)",
      "Bash(git rebase:*)",
      "Bash(git reset:*)",
      "Bash(gh pr merge:*)"
    ],
    "deny": [
      "Bash(rm -rf /)",
      "Bash(rm -rf ~)",
      "Bash(rm -rf .)",
      "Bash(sudo *)",
      "Bash(git push --force *)",
      "Bash(git push -f *)",
      "Bash(git reset --hard *)",
      "Bash(cat .env*)",
      "Bash(cat */.env*)",
      "Bash(cat *.pem)",
      "Bash(cat *.key)",
      "Bash(cat *credentials*)"
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
    write_file(&repo.join("docs/index.md"), DOCS_INDEX, force, backup)?;
    write_file(&repo.join("docs/PATTERNS.md"), DOCS_PATTERNS, force, backup)?;
    write_file(
        &repo.join("docs/CONTRIBUTING.md"),
        DOCS_CONTRIBUTING,
        force,
        backup,
    )?;
    write_file(
        &repo.join("docs/SUCCESS_CRITERIA.md"),
        DOCS_SUCCESS_CRITERIA,
        force,
        backup,
    )?;
    write_file(
        &repo.join("docs/PROJECT_LAYOUT.md"),
        DOCS_PROJECT_LAYOUT,
        force,
        backup,
    )?;

    // Write issues files
    write_file(&repo.join("issues/README.md"), ISSUES_README, force, backup)?;
    write_file(
        &repo.join("issues/_template.md"),
        ISSUES_TEMPLATE,
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
        eprintln!(
            "{} Run this to audit and populate documentation:",
            "→".cyan()
        );
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
