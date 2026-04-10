# Review Data Model

## File Layout

Reviews live in the worktree at `.jig/reviews/`:

```
.jig/reviews/
  001.md              # first review (written by review agent via jig review submit)
  001-response.md     # implementation agent's response (via jig review respond)
  002.md              # second review (sees 001.md + 001-response.md)
  003.md              # approved — no response needed
```

File names are zero-padded 3-digit numbers. Review files are `NNN.md`, response files are `NNN-response.md`. Paths are deterministic from the worktree path. No centralized state.

## Core Types

**Source:** `crates/jig-core/src/review.rs`

### Review

```rust
pub struct Review {
    pub reviewed_sha: String,       // commit SHA reviewed
    pub timestamp: i64,             // Unix timestamp
    pub verdict: ReviewVerdict,     // Approve or ChangesRequested
    pub sections: Vec<ReviewSection>,
    pub summary: String,
}
```

### ReviewVerdict

```rust
pub enum ReviewVerdict {
    Approve,
    ChangesRequested,
}
```

### ReviewSection

```rust
pub struct ReviewSection {
    pub category: ReviewCategory,
    pub status: ReviewStatus,       // worst severity in this section
    pub findings: Vec<Finding>,
}
```

### ReviewCategory

Six required categories, each maps to a `## Heading` in the markdown:

| Variant | Heading |
|---------|---------|
| Correctness | Correctness |
| Conventions | Conventions |
| ErrorHandling | Error Handling |
| Security | Security |
| TestCoverage | Test Coverage |
| Documentation | Documentation |

### Finding

```rust
pub struct Finding {
    pub file: Option<String>,       // e.g. "crates/jig-core/src/foo.rs"
    pub line: Option<u32>,          // line number
    pub message: String,
    pub severity: ReviewStatus,     // PASS, WARN, or FAIL
}
```

### ReviewStatus

```rust
pub enum ReviewStatus {
    Pass,   // [PASS]
    Warn,   // [WARN]
    Fail,   // [FAIL]
}
```

## Markdown Format

### Review

```markdown
# Review 001
Reviewed: abc123 | 2025-04-03T12:00:00Z

## Correctness
- [PASS] No issues found

## Conventions
- [WARN] `crates/jig-core/src/foo.rs:42` — variable name doesn't follow snake_case

## Error Handling
- [PASS] Appropriate for context

## Security
- [PASS] No issues found

## Test Coverage
- [FAIL] `crates/jig-core/src/foo.rs` — new public function `bar()` has no test

## Documentation
- [PASS] No updates needed

## Summary
VERDICT: changes_requested

Missing test coverage for `bar()`. One required change, one suggestion.
```

### Response

```markdown
# Response to Review 001

## Addressed
- `crates/jig-core/src/foo.rs` — missing test for `bar()`: Added test in commit def456

## Disputed
- `crates/jig-core/src/foo.rs:42` — snake_case: The variable follows the existing pattern in this module

## Deferred
(none)

## Notes
Also fixed an unrelated typo noticed while addressing findings.
```

## Response Types

| Action | Meaning |
|--------|---------|
| Addressed | Finding was fixed in a new commit |
| Disputed | Finding is incorrect or follows existing patterns; reviewer should not re-raise |
| Deferred | Acknowledged but out of scope; tracked for later |

## Path Helpers

**Source:** `crates/jig-core/src/review.rs`

| Function | Purpose |
|----------|---------|
| `reviews_dir(worktree)` | `.jig/reviews/` directory path |
| `next_review_path(worktree)` | Next `NNN.md` path (count + 1) |
| `review_response_path(worktree, n)` | `NNN-response.md` path |
| `review_count(worktree)` | Count of review files (excludes responses) |
| `review_history(worktree)` | All review + response files, sorted |
| `latest_verdict(worktree)` | Parse latest review file, return verdict |

## Configuration

**Source:** `crates/jig-core/src/config.rs`

```rust
pub struct ReviewConfig {
    pub enabled: bool,          // default: false
    pub model: Option<String>,  // optional model override
    pub max_rounds: u32,        // default: 5
}
```
