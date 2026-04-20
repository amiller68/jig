#![allow(deprecated)] // Command::cargo_bin is deprecated but used across tests
//! Integration tests for the automated review pipeline (JIG-27).
//!
//! These tests exercise the full review lifecycle: submit → respond → submit,
//! multi-round convergence, review history assembly, and file structure
//! invariants. The review actor (ephemeral Claude session) is not invoked;
//! instead, we drive the CLI commands directly with predetermined markdown.
//!
//! ## Manual test plan
//!
//! To test the full end-to-end loop with a real review agent:
//!
//! 1. Create a worktree: `jig create test-review`
//! 2. Make changes, commit, push, create draft PR
//! 3. Enable review: add `[review]\nenabled = true` to jig.toml
//! 4. Run daemon: `jig daemon --once` or check with `jig ps -w`
//! 5. Observe: review actor runs ephemerally, writes `.jig/reviews/001.md`
//! 6. Observe: AutoReview nudge delivered to worker's tmux session
//! 7. In worker session: read review, fix issues, `jig review respond --review 1`
//! 8. Push new commits — next review cycle triggers
//! 9. If approved: PR marked ready, issue status updated to "In Review"

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::process::Command as StdCommand;
use tempfile::TempDir;

const JIG_DIR: &str = ".jig";
const REVIEWS_DIR: &str = "reviews";

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

        // Add remote (pointing to self for testing)
        let path_str = dir.path().to_string_lossy().to_string();
        StdCommand::new("git")
            .args(["remote", "add", "origin", &path_str])
            .current_dir(dir.path())
            .output()
            .expect("add remote");

        StdCommand::new("git")
            .args(["fetch", "-q", "origin"])
            .current_dir(dir.path())
            .output()
            .expect("fetch");

        let config_file = config_dir.path().join("jig").join("config");
        fs::create_dir_all(config_file.parent().unwrap()).unwrap();
        fs::write(&config_file, "_default=main\n").unwrap();

        Self { dir, config_dir }
    }

    fn path(&self) -> PathBuf {
        self.dir.path().to_path_buf()
    }

    fn jig(&self) -> Command {
        let mut cmd = Command::cargo_bin("jig").unwrap();
        cmd.current_dir(self.path());
        cmd.env("XDG_CONFIG_HOME", self.config_dir.path());
        cmd
    }

    fn jig_at(&self, cwd: &std::path::Path) -> Command {
        let mut cmd = Command::cargo_bin("jig").unwrap();
        cmd.current_dir(cwd);
        cmd.env("XDG_CONFIG_HOME", self.config_dir.path());
        cmd
    }

    fn reviews_dir_at(&self, cwd: &std::path::Path) -> PathBuf {
        cwd.join(JIG_DIR).join(REVIEWS_DIR)
    }

    fn worktrees_path(&self) -> PathBuf {
        self.path().join(JIG_DIR)
    }

    /// Create a worktree and return its path.
    fn create_worktree(&self, name: &str) -> PathBuf {
        self.jig().args(["create", name]).assert().success();
        self.worktrees_path().join(name)
    }
}

// ---------------------------------------------------------------------------
// Review markdown fixtures
// ---------------------------------------------------------------------------

fn review_changes_requested(sha: &str) -> String {
    format!(
        "\
# Review 001
Reviewed: {sha} | 2026-04-04T12:00:00Z

## Correctness
- [FAIL] `src/foo.rs:42` — missing null check

## Conventions
- [WARN] `src/foo.rs:10` — variable name doesn't follow snake_case

## Error Handling
- [PASS] Appropriate for context

## Security
- [PASS] No issues found

## Test Coverage
- [WARN] `src/foo.rs` — new public function `bar()` has no test

## Documentation
- [PASS] No updates needed

## Summary
VERDICT: changes_requested

Missing null check is a blocker. Test coverage for `bar()` should be added.
"
    )
}

fn review_approve(sha: &str) -> String {
    format!(
        "\
# Review 001
Reviewed: {sha} | 2026-04-04T14:00:00Z

## Correctness
- [PASS] Previous null check issue addressed

## Conventions
- [PASS] No issues found

## Error Handling
- [PASS] Appropriate for context

## Security
- [PASS] No issues found

## Test Coverage
- [PASS] Tests added for `bar()`

## Documentation
- [PASS] No updates needed

## Summary
VERDICT: approve

All findings from Review 001 have been addressed. Looks good to merge.
"
    )
}

fn response_to_review(review_number: u32) -> String {
    format!(
        "\
# Response to Review {:03}

## Addressed
- `src/foo.rs:42` null check: Fixed in commit abc789
- `src/foo.rs` missing test: Added test_bar() in tests/

## Disputed
- `src/foo.rs:10` snake_case: Follows existing module pattern, see lines 3-8

## Deferred
(none)

## Notes
Also fixed an unrelated typo spotted during review.
",
        review_number
    )
}

// ============================================================================
// Test 1: Single review cycle — changes requested
// ============================================================================

#[test]
fn single_cycle_changes_requested() {
    let repo = TestRepo::new();
    let wt = repo.create_worktree("review-cr-1");

    // Submit a changes_requested review
    repo.jig_at(&wt)
        .args(["review", "submit"])
        .write_stdin(review_changes_requested("aaa111"))
        .assert()
        .success()
        .stdout(predicate::str::contains("Verdict: changes_requested"));

    // Verify review file
    let reviews = repo.reviews_dir_at(&wt);
    let r001 = reviews.join("001.md");
    assert!(r001.exists());
    let content = fs::read_to_string(&r001).unwrap();
    assert!(content.contains("VERDICT: changes_requested"));
    assert!(content.contains("[FAIL]"));
    assert!(content.contains("missing null check"));
}

// ============================================================================
// Test 2: Single review cycle — approved
// ============================================================================

#[test]
fn single_cycle_approved() {
    let repo = TestRepo::new();
    let wt = repo.create_worktree("review-ap-1");

    repo.jig_at(&wt)
        .args(["review", "submit"])
        .write_stdin(review_approve("bbb222"))
        .assert()
        .success()
        .stdout(predicate::str::contains("Verdict: approve"));

    let reviews = repo.reviews_dir_at(&wt);
    let content = fs::read_to_string(reviews.join("001.md")).unwrap();
    assert!(content.contains("VERDICT: approve"));
}

// ============================================================================
// Test 3: Multi-round convergence
// ============================================================================

#[test]
fn multi_round_convergence() {
    let repo = TestRepo::new();
    let wt = repo.create_worktree("review-multi");

    // Round 1: Review finds issues
    repo.jig_at(&wt)
        .args(["review", "submit"])
        .write_stdin(review_changes_requested("commit1"))
        .assert()
        .success();

    // Round 1: Worker responds
    repo.jig_at(&wt)
        .args(["review", "respond", "--review", "1"])
        .write_stdin(response_to_review(1))
        .assert()
        .success();

    // Round 2: Review approves
    repo.jig_at(&wt)
        .args(["review", "submit"])
        .write_stdin(review_approve("commit2"))
        .assert()
        .success();

    // Verify complete file structure
    let reviews = repo.reviews_dir_at(&wt);
    assert!(reviews.join("001.md").exists(), "First review");
    assert!(reviews.join("001-response.md").exists(), "First response");
    assert!(reviews.join("002.md").exists(), "Second review");
    assert!(
        !reviews.join("002-response.md").exists(),
        "No response to approval"
    );

    // Verify numbering in content
    let r2 = fs::read_to_string(reviews.join("002.md")).unwrap();
    assert!(r2.contains("# Review 002"));
    assert!(r2.contains("VERDICT: approve"));

    // Verify response references correct review
    let resp = fs::read_to_string(reviews.join("001-response.md")).unwrap();
    assert!(resp.contains("# Response to Review 001"));
}

// ============================================================================
// Test 4: Response handling — prompt assembly includes responses
// ============================================================================

#[test]
fn response_content_preserved_for_next_round() {
    let repo = TestRepo::new();
    let wt = repo.create_worktree("review-resp-check");

    // Submit review with findings
    repo.jig_at(&wt)
        .args(["review", "submit"])
        .write_stdin(review_changes_requested("sha1"))
        .assert()
        .success();

    // Submit response with disputed finding
    repo.jig_at(&wt)
        .args(["review", "respond", "--review", "1"])
        .write_stdin(response_to_review(1))
        .assert()
        .success();

    // Verify the response file includes both addressed and disputed findings
    let reviews = repo.reviews_dir_at(&wt);
    let resp_content = fs::read_to_string(reviews.join("001-response.md")).unwrap();
    assert!(
        resp_content.contains("Addressed"),
        "Response should have Addressed section"
    );
    assert!(
        resp_content.contains("Disputed"),
        "Response should have Disputed section"
    );
    assert!(
        resp_content.contains("snake_case"),
        "Disputed finding should be preserved"
    );
    assert!(
        resp_content.contains("null check"),
        "Addressed finding should be preserved"
    );

    // Verify review history ordering: files sort correctly for prompt assembly
    let mut files: Vec<String> = fs::read_dir(&reviews)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    files.sort();

    // Lexicographic sort gives: 001-response.md, 001.md
    // The review actor reads all files in sort order, so it sees:
    // 001-response.md (response) then 001.md (review)
    // Both will be included in the prompt for the next review round.
    assert_eq!(files.len(), 2);
    assert!(files.contains(&"001.md".to_string()));
    assert!(files.contains(&"001-response.md".to_string()));
}

// ============================================================================
// Test 5: Review count excludes response files
// ============================================================================

#[test]
fn review_count_excludes_responses() {
    let repo = TestRepo::new();
    let wt = repo.create_worktree("review-count");

    // Submit review
    repo.jig_at(&wt)
        .args(["review", "submit"])
        .write_stdin(review_changes_requested("sha1"))
        .assert()
        .success();

    // Submit response
    repo.jig_at(&wt)
        .args(["review", "respond", "--review", "1"])
        .write_stdin(response_to_review(1))
        .assert()
        .success();

    // Submit second review
    repo.jig_at(&wt)
        .args(["review", "submit"])
        .write_stdin(review_approve("sha2"))
        .assert()
        .success();

    // Count review files (NNN.md only, not NNN-response.md)
    let reviews = repo.reviews_dir_at(&wt);
    let review_count = fs::read_dir(&reviews)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.ends_with(".md") && !name.contains("-response") && name.len() == 6
        })
        .count();
    assert_eq!(review_count, 2, "Should count 2 reviews, not responses");

    // Total .md files should be 3
    let total_md = fs::read_dir(&reviews)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".md"))
        .count();
    assert_eq!(total_md, 3, "Should have 3 total .md files");
}

// ============================================================================
// Test 6: Max rounds file structure
// ============================================================================

#[test]
fn max_rounds_file_structure() {
    let repo = TestRepo::new();
    let wt = repo.create_worktree("review-maxround");
    let reviews = repo.reviews_dir_at(&wt);

    // Simulate max_rounds=2 by writing 2 review files directly
    fs::create_dir_all(&reviews).unwrap();
    fs::write(reviews.join("001.md"), review_changes_requested("sha1")).unwrap();
    fs::write(reviews.join("001-response.md"), response_to_review(1)).unwrap();
    fs::write(reviews.join("002.md"), review_changes_requested("sha2")).unwrap();

    // Count reviews — should be 2 (at max_rounds=2, daemon would escalate)
    let count = fs::read_dir(&reviews)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.ends_with(".md") && !name.contains("-response") && name.len() == 6
        })
        .count();
    assert_eq!(count, 2);

    // With max_rounds=2 and round=2, the daemon would skip triggering another
    // review and instead emit NeedsIntervention. The daemon logic:
    //   at_max = review_round >= max_rounds → 2 >= 2 → true
    //   needs_review && at_max → Escalate
    // This is verified by the unit tests in daemon/mod.rs; here we just
    // confirm the file structure matches the expected state.
}

// ============================================================================
// Test 7: Config opt-out — review disabled means no review files
// ============================================================================

#[test]
fn config_review_disabled_by_default() {
    let repo = TestRepo::new();

    // Write jig.toml without [review] section
    fs::write(repo.path().join("jig.toml"), "[spawn]\nauto = false\n").unwrap();

    // jig should work normally
    repo.jig().args(["list"]).assert().success();

    // The daemon would check review_config.enabled (default false) and skip.
    // No review files should exist.
    let reviews = repo.path().join(JIG_DIR).join(REVIEWS_DIR);
    assert!(
        !reviews.exists(),
        "Reviews dir should not exist when disabled"
    );
}

#[test]
fn config_review_enabled() {
    let repo = TestRepo::new();

    // Write jig.toml with review enabled
    fs::write(
        repo.path().join("jig.toml"),
        "[review]\nenabled = true\nmax_rounds = 3\n",
    )
    .unwrap();

    // jig should work normally with review config
    repo.jig().args(["list"]).assert().success();
}

// ============================================================================
// Test 8: Review submit from worktree uses cwd
// ============================================================================

#[test]
fn submit_writes_to_cwd_reviews_dir() {
    let repo = TestRepo::new();
    let wt = repo.create_worktree("review-cwd");

    repo.jig_at(&wt)
        .args(["review", "submit"])
        .write_stdin(review_approve("abc"))
        .assert()
        .success();

    // Review should be in the worktree's .jig/reviews/, not the repo root
    let wt_review = wt.join(JIG_DIR).join(REVIEWS_DIR).join("001.md");
    assert!(wt_review.exists(), "Review in worktree");

    let root_review = repo.path().join(JIG_DIR).join(REVIEWS_DIR).join("001.md");
    assert!(!root_review.exists(), "No review in repo root");
}

// ============================================================================
// Test 9: Invalid status markers rejected
// ============================================================================

#[test]
fn submit_rejects_invalid_status_marker() {
    let repo = TestRepo::new();
    let wt = repo.create_worktree("review-badstatus");

    let bad_md = "\
# Review 001
Reviewed: abc | 2026-04-04T12:00:00Z

## Correctness
- [GOOD] Looks fine

## Conventions
- [PASS] Ok

## Error Handling
- [PASS] Ok

## Security
- [PASS] Ok

## Test Coverage
- [PASS] Ok

## Documentation
- [PASS] Ok

## Summary
VERDICT: approve
";

    repo.jig_at(&wt)
        .args(["review", "submit"])
        .write_stdin(bad_md)
        .assert()
        .failure()
        .stderr(predicate::str::contains("[PASS], [WARN], or [FAIL]"));
}

// ============================================================================
// Test 10: Three-round cycle with responses
// ============================================================================

#[test]
fn three_round_review_cycle() {
    let repo = TestRepo::new();
    let wt = repo.create_worktree("review-3round");

    // Round 1: changes requested
    repo.jig_at(&wt)
        .args(["review", "submit"])
        .write_stdin(review_changes_requested("sha1"))
        .assert()
        .success();

    repo.jig_at(&wt)
        .args(["review", "respond", "--review", "1"])
        .write_stdin(response_to_review(1))
        .assert()
        .success();

    // Round 2: still changes requested
    repo.jig_at(&wt)
        .args(["review", "submit"])
        .write_stdin(review_changes_requested("sha2"))
        .assert()
        .success();

    repo.jig_at(&wt)
        .args(["review", "respond", "--review", "2"])
        .write_stdin(response_to_review(2))
        .assert()
        .success();

    // Round 3: approved
    repo.jig_at(&wt)
        .args(["review", "submit"])
        .write_stdin(review_approve("sha3"))
        .assert()
        .success();

    // Verify complete structure
    let reviews = repo.reviews_dir_at(&wt);
    let mut files: Vec<String> = fs::read_dir(&reviews)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    files.sort();

    assert_eq!(
        files,
        vec![
            "001-response.md",
            "001.md",
            "002-response.md",
            "002.md",
            "003.md",
        ]
    );

    // Verify final review
    let r3 = fs::read_to_string(reviews.join("003.md")).unwrap();
    assert!(r3.contains("# Review 003"));
    assert!(r3.contains("VERDICT: approve"));
}

// ============================================================================
// Test 11: Review respond requires --review flag
// ============================================================================

#[test]
fn respond_requires_review_number() {
    let repo = TestRepo::new();
    let wt = repo.create_worktree("review-noflag");

    // Submit a review first
    repo.jig_at(&wt)
        .args(["review", "submit"])
        .write_stdin(review_approve("abc"))
        .assert()
        .success();

    // Respond without --review flag should fail
    repo.jig_at(&wt)
        .args(["review", "respond"])
        .write_stdin(response_to_review(1))
        .assert()
        .failure()
        .stderr(predicate::str::contains("--review"));
}

// ============================================================================
// Test 12: Review submit missing header
// ============================================================================

#[test]
fn submit_rejects_missing_header() {
    let repo = TestRepo::new();
    let wt = repo.create_worktree("review-noheader");

    // Review without the "Reviewed:" line
    let bad_md = "\
# Review 001

## Correctness
- [PASS] No issues found

## Conventions
- [PASS] Ok

## Error Handling
- [PASS] Ok

## Security
- [PASS] Ok

## Test Coverage
- [PASS] Ok

## Documentation
- [PASS] Ok

## Summary
VERDICT: approve
";

    repo.jig_at(&wt)
        .args(["review", "submit"])
        .write_stdin(bad_md)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Reviewed:"));
}
