---
from: thing-2
to: orch
ts: 20260419T194959Z
kind: fyi
ref: T-002
---
After T-002, `issues/naming.rs` only contains `extract_linear_identifier` — a Linear-specific function that should live in the Linear module (thing-1's scope). The file name "naming" no longer makes sense.

Suggest thing-1 absorb `extract_linear_identifier` into their `linear/` module and delete `issues/naming.rs`. I can't do it since it's outside my file scope.
