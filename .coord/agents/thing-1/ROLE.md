---
name: thing-1
scope: "Linear GraphQL client extraction into crates/jig-core/src/linear/"
files_owned:
  - crates/jig-core/src/linear/**
  - crates/jig-core/src/issues/linear_client.rs
  - crates/jig-core/src/issues/linear_provider.rs
  - crates/jig-core/src/issues/mod.rs
  - crates/jig-core/src/lib.rs
constraints:
  - Do not change LinearProvider's public API or IssueProvider trait
  - Do not add new dependencies
---
