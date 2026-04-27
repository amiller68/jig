#![allow(deprecated)] // Command::cargo_bin is deprecated but used across tests
//! Integration tests for `jig review submit` and `jig review respond` commands.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

struct TestRepo {
    dir: TempDir,
    config_dir: TempDir,
}

impl TestRepo {
    fn new() -> Self {
        let dir = TempDir::new().expect("create temp dir");
        let config_dir = TempDir::new().expect("create config dir");

        StdCommand::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(dir.path())
            .output()
            .expect("git init");

        StdCommand::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir.path())
            .output()
            .expect("set email");

        StdCommand::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(dir.path())
            .output()
            .expect("set name");

        StdCommand::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(dir.path())
            .output()
            .expect("disable gpg");

        StdCommand::new("git")
            .args(["commit", "--allow-empty", "-m", "init", "-q"])
            .current_dir(dir.path())
            .output()
            .expect("initial commit");

        let config_file = config_dir.path().join("jig").join("config");
        fs::create_dir_all(config_file.parent().unwrap()).unwrap();
        fs::write(&config_file, "_default=main\n").unwrap();

        Self { dir, config_dir }
    }

    fn jig(&self) -> Command {
        let mut cmd = Command::cargo_bin("jig").unwrap();
        cmd.current_dir(self.dir.path());
        cmd.env("XDG_CONFIG_HOME", self.config_dir.path());
        cmd
    }
}

fn valid_review_markdown() -> &'static str {
    "\
# Review 001
Reviewed: abc123 | 2026-04-04T12:00:00Z

## Correctness
- [PASS] No issues found

## Conventions
- [PASS] No issues found

## Error Handling
- [PASS] No issues found

## Security
- [PASS] No issues found

## Test Coverage
- [PASS] No issues found

## Documentation
- [PASS] No issues found

## Summary
VERDICT: approve

Looks good.
"
}

fn valid_review_changes_requested() -> &'static str {
    "\
# Review 001
Reviewed: def456 | 2026-04-04T13:00:00Z

## Correctness
- [FAIL] `src/foo.rs:42` — missing null check

## Conventions
- [PASS] No issues found

## Error Handling
- [PASS] No issues found

## Security
- [PASS] No issues found

## Test Coverage
- [WARN] `src/foo.rs` — new function lacks test

## Documentation
- [PASS] No issues found

## Summary
VERDICT: changes_requested

Needs fixes.
"
}

fn valid_response_markdown() -> &'static str {
    "\
# Response to Review 001

## Addressed
- `src/foo.rs` — missing test: Added test in commit def456

## Disputed
- `src/foo.rs:42` — naming: Follows existing module pattern, see lines 10-15

## Deferred
(none)

## Notes
Also fixed a typo.
"
}

// ---------------------------------------------------------------------------
// jig review submit tests
// ---------------------------------------------------------------------------

#[test]
fn submit_valid_review() {
    let repo = TestRepo::new();

    repo.jig()
        .args(["review", "submit"])
        .write_stdin(valid_review_markdown())
        .assert()
        .success()
        .stdout(predicate::str::contains("Review written to"))
        .stdout(predicate::str::contains("001.md"))
        .stdout(predicate::str::contains("Verdict: approve"));

    // Verify file was created
    let review_file = repo.dir.path().join(".jig/reviews/001.md");
    assert!(review_file.exists(), "Review file should exist");

    let content = fs::read_to_string(&review_file).unwrap();
    assert!(content.contains("VERDICT: approve"));
}

#[test]
fn submit_invalid_review_missing_section() {
    let repo = TestRepo::new();

    let bad_markdown = "\
# Review 001
Reviewed: abc123 | 2026-04-04T12:00:00Z

## Correctness
- [PASS] No issues found

## Summary
VERDICT: approve
All good.
";

    repo.jig()
        .args(["review", "submit"])
        .write_stdin(bad_markdown)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Missing required section"));
}

#[test]
fn submit_invalid_review_missing_verdict() {
    let repo = TestRepo::new();

    let bad_markdown = "\
# Review 001
Reviewed: abc123 | 2026-04-04T12:00:00Z

## Correctness
- [PASS] No issues found

## Conventions
- [PASS] No issues found

## Error Handling
- [PASS] No issues found

## Security
- [PASS] No issues found

## Test Coverage
- [PASS] No issues found

## Documentation
- [PASS] No issues found

## Summary
All good but no verdict.
";

    repo.jig()
        .args(["review", "submit"])
        .write_stdin(bad_markdown)
        .assert()
        .failure()
        .stderr(predicate::str::contains("VERDICT"));
}

#[test]
fn submit_second_review_gets_002() {
    let repo = TestRepo::new();

    // Submit first review
    repo.jig()
        .args(["review", "submit"])
        .write_stdin(valid_review_markdown())
        .assert()
        .success();

    // Submit second review
    repo.jig()
        .args(["review", "submit"])
        .write_stdin(valid_review_changes_requested())
        .assert()
        .success()
        .stdout(predicate::str::contains("002.md"))
        .stdout(predicate::str::contains("Verdict: changes_requested"));

    // Verify both files exist
    assert!(repo.dir.path().join(".jig/reviews/001.md").exists());
    assert!(repo.dir.path().join(".jig/reviews/002.md").exists());
}

// ---------------------------------------------------------------------------
// jig review respond tests
// ---------------------------------------------------------------------------

#[test]
fn respond_to_existing_review() {
    let repo = TestRepo::new();

    // First submit a review
    repo.jig()
        .args(["review", "submit"])
        .write_stdin(valid_review_markdown())
        .assert()
        .success();

    // Then respond to it
    repo.jig()
        .args(["review", "respond", "--review", "1"])
        .write_stdin(valid_response_markdown())
        .assert()
        .success()
        .stdout(predicate::str::contains("Response written to"))
        .stdout(predicate::str::contains("001-response.md"));

    // Verify response file was created
    let response_file = repo.dir.path().join(".jig/reviews/001-response.md");
    assert!(response_file.exists(), "Response file should exist");

    let content = fs::read_to_string(&response_file).unwrap();
    assert!(content.contains("Addressed"));
}

#[test]
fn respond_to_nonexistent_review() {
    let repo = TestRepo::new();

    repo.jig()
        .args(["review", "respond", "--review", "99"])
        .write_stdin(valid_response_markdown())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Review 099 not found"));
}

// ---------------------------------------------------------------------------
// Subcommand parsing tests
// ---------------------------------------------------------------------------

#[test]
fn submit_and_show_no_conflict() {
    let repo = TestRepo::new();

    // "submit" is a subcommand, not a worktree name
    // This should attempt to read stdin (and succeed/fail based on stdin content),
    // not try to look up a worktree named "submit"
    repo.jig()
        .args(["review", "submit"])
        .write_stdin(valid_review_markdown())
        .assert()
        .success();

    // "show" with a worktree name should work as the show subcommand
    // (it will fail because no worktree exists, but it should parse correctly)
    repo.jig()
        .args(["review", "show", "my-worktree"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));

    // Backward compat: `jig review <name>` should work the same as `jig review show <name>`
    repo.jig()
        .args(["review", "my-worktree"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}
