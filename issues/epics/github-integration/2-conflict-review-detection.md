# Conflict & Review Detection

**Status:** Complete  
**Priority:** High  
**Category:** Features  
**Epic:** issues/epics/github-integration/index.md  
**Depends-On:** issues/epics/github-integration/0-octorust-client.md

## Objective

Detect merge conflicts and unresolved review comments, nudge with resolution steps.

## Implementation

**Conflicts:** Check PR `mergeable` field. If `CONFLICTING`, nudge with rebase instructions.

**Reviews:** Fetch inline comments and general reviews via octorust. Parse file/line/body. Check for `CHANGES_REQUESTED` or `COMMENTED` decision.

NOTE: github has this annoying quirk where you can't request changes on a PR made by yourself, and I often run agents under my git id

Nudge messages:
- Conflicts: Step-by-step rebase guide
- Reviews: List all comments with context

Integrate with heartbeat nudge system (conflict/review nudge types).

## Acceptance Criteria

- [ ] Detect conflicts via `mergeable` field
- [ ] Parse inline review comments (file, line, body)
- [ ] Detect `CHANGES_REQUESTED` decision
- [ ] Build nudge messages with instructions
- [ ] Track nudge counts (conflict, review types)
