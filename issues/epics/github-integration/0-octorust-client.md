# Octorust Client Setup

**Status:** Planned  
**Priority:** High  
**Category:** Features  
**Epic:** issues/epics/github-integration/index.md

## Objective

Set up octorust GitHub API client with auth, caching, and basic PR operations.

## Implementation

Use octorust crate for type-safe GitHub API access.

Auth: token from `jig.toml`, `GITHUB_TOKEN` env, or `gh auth token` CLI fallback.

Cache PR state in `.worktrees/.jig-github-cache.json`:
```json
{
  "prs": {
    "features/auth": {
      "number": 42,
      "mergeable": "MERGEABLE",
      "ci_status": "SUCCESS",
      "last_checked": 1708363200
    }
  },
  "cache_ttl": 300
}
```

Implement `get_pr_for_branch()`, `invalidate_cache()`, basic queries.

## Acceptance Criteria

- [ ] Octorust client initialized from config/env/gh CLI
- [ ] PR cache with TTL
- [ ] `get_pr_for_branch()` fetches PR by branch name
- [ ] Cache bypass with `--no-cache` flag
- [ ] Graceful degradation if GitHub unavailable
