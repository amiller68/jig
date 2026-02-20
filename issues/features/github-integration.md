# Native GitHub Integration

**Status:** Planned  
**Priority:** High  
**Category:** Features

## Objective

Add native GitHub integration to `jig` for automatic detection and handling of PR-related issues: merge conflicts, failing CI, review comments, non-conventional commits.

## Background

The issue-grinder currently shells out to `gh` CLI for all GitHub checks:
- PR merge status (conflicts)
- CI status (failing checks)
- Review comments (unresolved inline/review comments)
- Commit message validation (conventional commits)
- PR lifecycle (merged, closed)

This should be built into jig with proper error handling and caching.

## Acceptance Criteria

### Repo Registry Integration
- [ ] Depends on: `issues/features/global-commands.md` (repo registry)
- [ ] GitHub org/repo auto-detected from git remote
- [ ] Store GitHub config in repo's `jig.toml`:
  ```toml
  [github]
  owner = "amiller68"
  repo = "jig"
  requireConventionalCommits = true
  autoCleanupMerged = true
  ```
- [ ] Support per-repo GitHub settings
- [ ] Use `GlobalContext` for efficient multi-repo operations
- [ ] Support `-g` flag to operate on all registered repos

### Core GitHub Client
- [ ] GitHub API client in jig-core (use `octocrab` or similar)
- [ ] Auth via `gh` CLI token or `GITHUB_TOKEN` env var
- [ ] Caching layer to avoid rate limits
- [ ] Graceful degradation when GitHub unavailable
- [ ] Auto-detect GitHub remote from repo

### PR Status Checks
- [ ] `jig pr status <worker>` - show PR status for worker's branch
- [ ] Detect merge conflicts automatically
- [ ] Detect failing CI checks
- [ ] Detect unresolved review comments
- [ ] Detect closed/merged PRs

### Auto-Cleanup
- [ ] `jig health` detects merged PRs and cleans up workers
- [ ] `jig health` detects closed PRs and asks before cleanup
- [ ] Option: `jig pr cleanup --merged` to manually trigger

### Review Comment Detection
- [ ] Parse inline review comments from GitHub API
- [ ] Parse general review comments
- [ ] Detect "CHANGES_REQUESTED" review decision
- [ ] Format comments for nudge messages

### CI Integration
- [ ] Fetch failing check details
- [ ] Parse error logs from failed runs
- [ ] Include in nudge messages (truncated, relevant)

### Conflict Resolution
- [ ] Detect merge conflicts via GitHub mergeable status
- [ ] Nudge worker with rebase instructions
- [ ] Track conflict resolution attempts

### Commit Validation
- [ ] Validate commits follow conventional commit format
- [ ] Patterns: `feat|fix|docs|style|refactor|perf|test|chore|ci`
- [ ] Support breaking changes: `feat!: ...`
- [ ] Configurable via `jig config`
- [ ] Pre-commit hook option

## Implementation Notes

**Phase 0: Repo Registry (prerequisite)**
- Depends on: `issues/features/global-commands.md`
- GitHub owner/repo auto-detected from git remote
- Store in `jig.toml` `[github]` section
- Fallback to global token in `~/.config/jig/config`

**Phase 1: Core API**
1. Add `octocrab` or `github-rs` dependency
2. Auth and basic PR fetching (use repo's GitHub config)
3. `jig pr status` command (works on current repo or --repo)
4. Use `GlobalContext` for `-g` flag to iterate registered repos efficiently

**Phase 2: Health Checks**
1. Integrate with heartbeat system (issues/features/worker-heartbeat-system.md)
2. PR status checks on each heartbeat (uses GlobalContext for efficiency)
3. Auto-nudge for conflicts, CI, reviews (per-repo settings)

**Phase 3: Lifecycle**
1. Auto-cleanup merged PRs (per-repo `autoCleanupMerged` setting)
2. Handle closed PRs gracefully
3. Alert on PR state changes

**Phase 4: Advanced**
1. Pre-commit hooks for commit validation
2. Real-time webhooks (optional)
3. Batch operations with `jig pr cleanup -g` etc.

## Commands

```bash
# Check PR status (current repo or --repo)
jig pr status <worker> [--repo <path>]

# List open PRs for repo
jig pr list [--repo <path>]

# List PRs across all registered repos
jig pr list -g

# Cleanup merged PRs
jig pr cleanup --merged [--repo <path>]
jig pr cleanup --merged -g  # all registered repos

# Force-sync PR state
jig pr sync [--repo <path>]
jig pr sync -g  # all registered repos
```

## Configuration

**Per-repo settings in `jig.toml`:**

```toml
[github]
# GitHub owner/repo (auto-detected from git remote if not specified)
owner = "amiller68"
repo = "jig"

# Auth token (fallback to gh CLI or GITHUB_TOKEN)
token = "ghp_..."

# Enable auto-cleanup of merged PRs
autoCleanupMerged = true

# Require conventional commits
requireConventionalCommits = true

# Max age for closed PRs before auto-cleanup (hours)
closedPrCleanupAfter = 24

[conventionalCommits]
# Allowed types
types = ["feat", "fix", "docs", "style", "refactor", "perf", "test", "chore", "ci"]

# Require scope
requireScope = false

# Allowed scopes (empty = any)
scopes = []
```

**Global fallback in `~/.config/jig/config`:**
- `github.token=ghp_...` - auth token used across all repos
- Per-repo settings in `jig.toml` override global defaults

## Related Issues

- #TBD: Worker heartbeat system
- #TBD: Notification/alert system
- #TBD: Worker lifecycle management

## References

- Current implementation: `~/.openclaw/workspace/skills/issue-grinder/grind.sh`
  - PR cleanup: lines 204-237
  - CI checks: lines 599-673
  - Conflicts: lines 675-749
  - Reviews: lines 751-857
  - Commits: lines 858-920
