#![allow(deprecated)] // Command::cargo_bin is deprecated but used across tests
//! Integration tests for the manual-child path in parent-child epics.
//!
//! Verifies that a child issue created with `--parent` can be spawned,
//! and the parent relationship is correctly tracked through the lifecycle.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

/// Jig worktree directory name - must match jig_core::config::JIG_DIR
const JIG_DIR: &str = ".jig";

struct TestRepo {
    dir: TempDir,
    config_dir: TempDir,
}

impl TestRepo {
    fn new() -> Self {
        let dir = TempDir::new().expect("Failed to create temp dir");
        let config_dir = TempDir::new().expect("Failed to create config dir");

        // Initialize git repo
        StdCommand::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to init git repo");

        StdCommand::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to set git email");

        StdCommand::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to set git name");

        StdCommand::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to disable gpg signing");

        StdCommand::new("git")
            .args(["commit", "--allow-empty", "-m", "init", "-q"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to create initial commit");

        let path_str = dir.path().to_string_lossy().to_string();
        StdCommand::new("git")
            .args(["remote", "add", "origin", &path_str])
            .current_dir(dir.path())
            .output()
            .expect("Failed to add remote");

        StdCommand::new("git")
            .args(["fetch", "-q", "origin"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to fetch");

        let config_file = config_dir.path().join("jig").join("config");
        fs::create_dir_all(config_file.parent().unwrap()).unwrap();
        fs::write(&config_file, "_default=main\n").expect("Failed to write config");

        // Create issues directory with templates
        let issues_dir = dir.path().join("issues");
        let templates_dir = issues_dir.join("_templates");
        fs::create_dir_all(&templates_dir).unwrap();
        fs::write(
            templates_dir.join("standalone.md"),
            "# [Title]\n\n**Status:** Planned\n\n## Objective\n\nDescribe.\n",
        )
        .unwrap();

        // Create a parent issue
        let features_dir = issues_dir.join("features");
        fs::create_dir_all(&features_dir).unwrap();
        fs::write(
            features_dir.join("parent-epic.md"),
            "# Parent Epic\n\n**Status:** In Progress\n**Priority:** High\n\nEpic description.\n",
        )
        .unwrap();

        TestRepo { dir, config_dir }
    }

    fn worktrees_path(&self) -> std::path::PathBuf {
        self.dir.path().join(JIG_DIR)
    }

    #[allow(deprecated)]
    fn jig(&self) -> Command {
        let mut cmd = Command::cargo_bin("jig").expect("Failed to find jig binary");
        cmd.current_dir(self.dir.path());
        cmd.env("XDG_CONFIG_HOME", self.config_dir.path());
        cmd
    }
}

// ============================================================================
// Parent-Child Issue Creation
// ============================================================================

#[test]
fn create_child_with_parent_sets_parent_field() {
    let repo = TestRepo::new();

    repo.jig()
        .args([
            "issues",
            "create",
            "Child task one",
            "--parent",
            "features/parent-epic",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created issue:"));

    let content =
        fs::read_to_string(repo.dir.path().join("issues/features/child-task-one.md")).unwrap();
    assert!(
        content.contains("**Parent:** features/parent-epic"),
        "Child issue should contain parent field"
    );
}

#[test]
fn detail_of_child_shows_parent_info() {
    let repo = TestRepo::new();

    // Create child with parent
    repo.jig()
        .args([
            "issues",
            "create",
            "Child with detail",
            "--parent",
            "features/parent-epic",
        ])
        .assert()
        .success();

    // View the child issue detail — should show parent info
    repo.jig()
        .args(["issues", "features/child-with-detail"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Parent:"));
}

#[test]
fn create_multiple_children_for_same_parent() {
    let repo = TestRepo::new();

    // Create two children under the same parent
    repo.jig()
        .args([
            "issues",
            "create",
            "First child",
            "--parent",
            "features/parent-epic",
        ])
        .assert()
        .success();

    repo.jig()
        .args([
            "issues",
            "create",
            "Second child",
            "--parent",
            "features/parent-epic",
        ])
        .assert()
        .success();

    // Both should exist with parent field
    let c1 = fs::read_to_string(repo.dir.path().join("issues/features/first-child.md")).unwrap();
    let c2 = fs::read_to_string(repo.dir.path().join("issues/features/second-child.md")).unwrap();

    assert!(c1.contains("**Parent:** features/parent-epic"));
    assert!(c2.contains("**Parent:** features/parent-epic"));
}

// ============================================================================
// Manual-Child Worktree Creation
// ============================================================================

#[test]
fn create_worktree_for_child_issue() {
    let repo = TestRepo::new();

    // Create child issue
    repo.jig()
        .args([
            "issues",
            "create",
            "Manual child work",
            "--parent",
            "features/parent-epic",
        ])
        .assert()
        .success();

    // Create a worktree manually (not spawn, since that needs tmux/claude)
    repo.jig()
        .args(["create", "manual-child-work"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Created worktree"));

    assert!(repo.worktrees_path().join("manual-child-work").exists());
}

// ============================================================================
// Child Lifecycle Through Completion
// ============================================================================

#[test]
fn child_completion_updates_status() {
    let repo = TestRepo::new();

    // Create child with parent
    repo.jig()
        .args([
            "issues",
            "create",
            "Completable child",
            "--parent",
            "features/parent-epic",
        ])
        .assert()
        .success();

    // Mark child complete
    repo.jig()
        .args(["issues", "complete", "features/completable-child"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Completed:"));

    let content =
        fs::read_to_string(repo.dir.path().join("issues/features/completable-child.md")).unwrap();
    assert!(content.contains("**Status:** Complete"));
    // Parent field should still be present after completion
    assert!(content.contains("**Parent:** features/parent-epic"));
}

#[test]
fn child_status_update_preserves_parent() {
    let repo = TestRepo::new();

    // Create child with parent
    repo.jig()
        .args([
            "issues",
            "create",
            "Status child",
            "--parent",
            "features/parent-epic",
        ])
        .assert()
        .success();

    // Update status to in-progress
    repo.jig()
        .args([
            "issues",
            "status",
            "features/status-child",
            "--status",
            "in-progress",
        ])
        .assert()
        .success();

    let content =
        fs::read_to_string(repo.dir.path().join("issues/features/status-child.md")).unwrap();
    assert!(content.contains("**Status:** In Progress"));
    assert!(
        content.contains("**Parent:** features/parent-epic"),
        "Parent field should be preserved after status update"
    );
}
