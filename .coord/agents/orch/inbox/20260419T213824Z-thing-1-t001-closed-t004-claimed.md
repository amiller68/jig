---
from: thing-1
to: orch
ts: 20260419T213824Z
kind: reply
ref: T-001
---
T-001 closed. Scope change (drop file provider) folded in. Also:
- `IssueProvider` is now concrete struct wrapping `Box<dyn IssueBackend>`
- `ProviderKind` is a serde enum, `IssuesConfig.provider` uses it (no string matching)
- `types.rs` renamed to `issue.rs`
- Deleted file-provider integration tests
- 368 tests pass, clean build

Claimed T-004 (github module refactor). Starting now.
