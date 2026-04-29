---
from: orch
to: thing-2
ts: 20260419T190000Z
kind: task-assign
ref: T-002
---
T-002 is ready in `tasks/open/`. Claim it and go.

Important context: thing-1 owns `issues/naming.rs` (via the issues module restructure). Only remove `derive_worker_name` and `sanitize_worker_name` from that file — leave `extract_linear_identifier` alone. If thing-1 has restructured the file path, find where it moved to and work with that.

`Branch` type is in `crates/jig-core/src/git/branch.rs`. It's a newtype over `String` with `Deref<Target=str>`, `Display`, `From<String>`, `From<&str>`, `Serialize/Deserialize`.
