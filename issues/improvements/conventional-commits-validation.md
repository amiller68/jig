# Conventional Commits Validation

**Status:** In Progress
**Priority:** Medium
**Category:** Improvements

## Objective

Replace regex-based commit message validation with a proper parser that provides clear error messages, configurable rules, and pre-commit hook support.

## Specification

**Conventional Commits v1.0.0:**
```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

**Breaking changes:** `type!:`, `type(scope)!:`, or `BREAKING CHANGE:` footer.

## Data Model

```rust
pub struct CommitMessage {
    pub commit_type: String,
    pub scope: Option<String>,
    pub breaking: bool,
    pub description: String,
    pub body: Option<String>,
    pub footers: Vec<Footer>,
}

pub struct Footer {
    pub token: String,
    pub value: String,
}
```

## Configuration

In `jig.toml`:

```toml
[commits]
types = ["feat", "fix", "docs", "style", "refactor", "perf", "test", "chore", "ci"]
require_scope = false
scopes = []  # empty = any scope allowed
allow_breaking = true
max_subject_length = 72
require_lowercase = true
```

## CLI Surface

```bash
jig commit validate              # validate last commit
jig commit validate HEAD~3       # validate specific commit
jig commit validate --stdin      # validate from stdin
jig commit validate --file PATH  # validate commit message file
jig commit examples              # show conventional commit examples
```

## Implementation Phases

1. **Core parser** — parse header (type, scope, breaking, description), body, and footers. Use simple string parsing (nom is overkill for this grammar). **Done.**
2. **Validator** — configurable rules for types, scopes, subject length, casing, breaking changes. User-friendly error messages with examples. **Done.**
3. **CLI integration** — `jig commit validate` and `jig commit examples` commands. **Done.**
4. **DX polish** — commit template generation, pre-commit hook enforcement, git config integration. **Not started.**

## Acceptance Criteria

- [x] Parse all conventional commit formats (basic, scoped, breaking, body, footers)
- [x] Validate against configurable rules (types, scopes, length, casing, breaking)
- [x] Clear error messages that show expected format and examples
- [x] `jig commit validate` and `jig commit examples` CLI commands
- [ ] Pre-commit hook integration
- [x] Per-repo configuration in `jig.toml`

## Related Issues

- issues/features/git-hooks-management.md (pre-commit hook)
- issues/features/github-integration.md (PR commit validation)
