//! Init command - initialize repository for scribe

use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::path::Path;

use wt_core::{config, git, Error};

const SCRIBE_TOML_CONTENT: &str = r#"[spawn]
auto = false
"#;

const CLAUDE_MD_TEMPLATE: &str = r#"# Project Guide

Guide for AI agents and developers working on this project.

## Project Overview

<!-- Describe your project here -->

## Key Files

<!-- List important files and their purposes -->

## Development

<!-- Add development instructions -->

## Testing

<!-- Add testing instructions -->
"#;

const DOCS_INDEX_TEMPLATE: &str = r#"# Agent Instructions

Instructions for spawned Claude Code workers (via `scribe spawn`).

## Task Guidelines

When working on tasks in this project:

1. Follow existing code style and conventions
2. Write tests for new functionality
3. Update documentation as needed
4. Commit changes with clear messages

## Project Context

<!-- Add project-specific context here -->
"#;

const ISSUE_TRACKING_TEMPLATE: &str = r#"# Issue Tracking

File-based issue tracking for this project.

## Convention

Issues are tracked in the `issues/` directory as markdown files:

- `issues/001-feature-name.md` - Feature requests
- `issues/002-bug-description.md` - Bug reports

## Issue Template

```markdown
# Issue Title

## Description

## Acceptance Criteria

- [ ] Criteria 1
- [ ] Criteria 2

## Notes
```
"#;

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
      "Bash(ls *)",
      "Bash(pwd)",
      "Bash(which *)",
      "Bash(cat *)",
      "Bash(head *)",
      "Bash(tail *)",
      "Bash(wc *)"
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
    if config::has_scribe_toml()? && !force {
        return Err(Error::AlreadyInitialized.into());
    }

    eprintln!("{} Initializing scribe in {}", "→".cyan(), repo.display());

    // Create directories
    let dirs = ["docs", "issues", ".claude/skills"];
    for dir in dirs {
        let path = repo.join(dir);
        if !path.exists() {
            fs::create_dir_all(&path)?;
            eprintln!("  {} Created {}/", "✓".green(), dir);
        }
    }

    // Write scribe.toml
    write_file(&repo.join("scribe.toml"), SCRIBE_TOML_CONTENT, force, backup)?;

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

    eprintln!();
    eprintln!("{} Initialization complete", "✓".green().bold());

    if audit {
        eprintln!();
        eprintln!(
            "{} Run 'claude' to audit and customize documentation",
            "→".dimmed()
        );
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
