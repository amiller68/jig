# Commit Validation

**Status:** Planned  
**Priority:** High  
**Category:** Features  
**Epic:** issues/epics/github-integration/index.md  
**Depends-On:** issues/epics/github-integration/0-octorust-client.md

## Objective

Validate commits on PR branches follow conventional commit format.

## Implementation

Fetch commits from PR, validate each message against pattern:
```
^(feat|fix|docs|style|refactor|perf|test|chore|ci)(\(.+\))?!?: .+
```

Configurable types and scopes in `jig.toml`:
```toml
[conventionalCommits]
types = ["feat", "fix", "docs", ...]
requireScope = false
scopes = []
```

Nudge with rebase instructions if violations found.

## Acceptance Criteria

- [ ] Fetch commits from PR
- [ ] Validate against regex pattern
- [ ] Configurable types/scopes
- [ ] Build nudge with rebase guide
- [ ] Track nudge count (bad_commits type)

## References

See `issues/improvements/conventional-commits-validation.md` for full parser.
