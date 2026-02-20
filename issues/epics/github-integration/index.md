# Epic: GitHub Integration

**Status:** Planned  
**Priority:** High  
**Category:** Features  
**Depends-On:** issues/features/global-commands.md, issues/epics/worker-heartbeat/index.md

## Objective

Add native GitHub integration for automatic detection and handling of PR-related issues: merge conflicts, failing CI, review comments, non-conventional commits.

## Why This Matters

Replace shell-outs to `gh` CLI with robust API:
- Detect CI failures and nudge with error logs
- Detect merge conflicts and nudge with rebase instructions
- Parse review comments and nudge workers
- Auto-cleanup merged PRs
- Validate conventional commits

## Tickets

| # | Ticket | Status | Priority |
|---|--------|--------|----------|
| 0 | Octorust client setup | Planned | High |
| 1 | CI status detection | Planned | High |
| 2 | Conflict & review detection | Planned | High |
| 3 | Commit validation | Planned | High |
| 4 | Auto-cleanup lifecycle | Planned | Medium |

## Design Principles

1. **Octorust for API** - type-safe, async, built-in rate limiting
2. **Cached state** - avoid redundant API calls
3. **Nudge integration** - use heartbeat system for delivery
4. **Adapter-agnostic** - PR operations work regardless of agent
5. **Configurable** - per-repo settings in `jig.toml`

## Dependencies

- `issues/features/global-commands.md` - multi-repo operations
- `issues/epics/worker-heartbeat/index.md` - nudge delivery system

## Acceptance Criteria

- [ ] Octorust client with auth and caching
- [ ] Detect CI failures with error logs
- [ ] Detect merge conflicts via GitHub API
- [ ] Parse inline and general review comments
- [ ] Validate conventional commits
- [ ] Auto-cleanup merged/closed PRs
- [ ] All operations work with `-g` flag

## References

- Commit validation: `issues/improvements/conventional-commits-validation.md`
- Original spec: docs/GRINDER-ANALYSIS.md (GitHub section)
