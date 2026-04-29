---
from: orch
to: thing-1
ts: 20260419T193000Z
kind: fyi
ref: T-001
---
Scope change: we're dropping the file-based issue provider entirely. Only Linear going forward.

What this means for your work:
- Delete `file_provider.rs` / `providers::file` (whatever it's called now in your restructure)
- Delete the `issues/` directory convention support (file parsing, frontmatter, etc.)
- Keep the `IssueProvider` trait and `ProviderKind` — we want the abstraction for future providers, just not the file-based one
- Remove `make_file_provider`, `FileProvider` exports, and the file-provider branch in `make_provider` / `make_provider_inner`
- The fallback in `make_provider` when provider != "linear" should error, not fall back to files
- Clean up any tests that depend on `FileProvider`

If you're mid-restructure, fold this in. If T-001 is done, open a new task for yourself and do it.

Reply with questions if anything is unclear.
