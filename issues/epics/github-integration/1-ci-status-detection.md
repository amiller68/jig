# CI Status Detection

**Status:** Planned  
**Priority:** High  
**Category:** Features  
**Epic:** issues/epics/github-integration/index.md  
**Depends-On:** issues/epics/github-integration/0-octorust-client.md

## Objective

Detect failing CI checks and nudge workers with error logs.

## Implementation

Fetch check runs for PR via octorust:
- Filter for FAILURE/ERROR status
- Get check names and details_url
- Fetch error logs (truncate to 50 lines)

Build nudge message:
```
CI is failing on PR #42:
• Lint & Format: clippy error at line 59
• Tests: 2 failing tests

Error log (last 50 lines):
[error output]

Fix, commit (fix: ...), push, call /review.
```

Integrate with heartbeat nudge system.

## Acceptance Criteria

- [ ] Detect CI failures via octorust
- [ ] Fetch error logs from failed runs
- [ ] Build contextual nudge message
- [ ] Nudge worker via tmux
- [ ] Track nudge count (ci_failure type)
