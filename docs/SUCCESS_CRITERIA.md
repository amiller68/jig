# Success Criteria

Checks that must pass before code can be merged. This is the CI gate.

## Quick Check

Run all checks in sequence:
```bash
cargo build && cargo test && cargo clippy -- -D warnings && cargo fmt --check
```

## Individual Checks

### Build

```bash
cargo build
```

Build must complete without errors.

### Tests

```bash
cargo test
```

All tests must pass. Integration tests create isolated git repos using tempfile.

### Linting

```bash
cargo clippy -- -D warnings
```

No clippy warnings allowed. Common issues:
- Unused variables or imports
- Unnecessary clones
- Missing error handling

### Formatting

Check formatting:
```bash
cargo fmt --check
```

Fix formatting:
```bash
cargo fmt
```

## Fixing Common Issues

### Formatting Failures

Run the formatter and commit:
```bash
cargo fmt
git add -p  # Review changes
git commit -m "style: format code"
```

### Lint Warnings

Fix the warning in the code, or if it's a false positive, add an allow attribute:
```rust
#[allow(clippy::lint_name)]
```

Only suppress lints with good reason.

### Test Failures

Run a specific test with output:
```bash
cargo test test_name -- --nocapture
```

Debug integration tests by examining the temp directory state.

## Pre-commit

No pre-commit hooks are configured. Run checks manually before pushing.
