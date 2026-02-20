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

### Core GitHub Client
- [ ] GitHub API client in jig-core (use `octocrab` or similar)
- [ ] Auth via `gh` CLI token or `GITHUB_TOKEN` env var
- [ ] Caching layer to avoid rate limits
- [ ] Graceful degradation when GitHub unavailable

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

**Phase 1: Core API**
1. Add `octocrab` or `github-rs` dependency
2. Auth and basic PR fetching
3. `jig pr status` command

**Phase 2: Health Checks**
1. Integrate with heartbeat system (#TBD)
2. PR status checks on each heartbeat
3. Auto-nudge for conflicts, CI, reviews

**Phase 3: Lifecycle**
1. Auto-cleanup merged PRs
2. Handle closed PRs gracefully
3. Alert on PR state changes

**Phase 4: Advanced**
1. Pre-commit hooks for commit validation
2. Real-time webhooks (optional)
3. Multi-repo support

## Commands

```bash
# Check PR status
jig pr status <worker>

# List open PRs for repo
jig pr list

# Cleanup merged PRs
jig pr cleanup --merged

# Force-sync PR state
jig pr sync
```

## Configuration

```toml
[github]
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
