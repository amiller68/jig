# Success Criteria

Checks that must pass before code can be merged. This is the CI gate.

## Quick Check

<!-- Single command to run all checks (if available):
```bash
make check
# or
npm run check
```
-->

## Individual Checks

### Build

<!-- How to verify the project builds:
```bash
cargo build
# or
npm run build
# or
go build ./...
```
-->

### Tests

<!-- How to run tests:
```bash
cargo test
# or
npm test
# or
pytest
```
-->

### Linting

<!-- How to run linters:
```bash
cargo clippy
# or
npm run lint
# or
ruff check .
```
-->

### Formatting

<!-- How to check/fix formatting:
```bash
cargo fmt --check    # Check
cargo fmt            # Fix
# or
npm run format:check
npm run format
```
-->

### Type Checking

<!-- If applicable:
```bash
npm run typecheck
# or
mypy .
# or
(Rust/Go: handled by compiler)
```
-->

## Fixing Common Issues

### Formatting Failures

Run the formatter and commit:
```bash
<!-- your format command -->
```

### Lint Warnings

<!-- How to address common lint issues -->

### Test Failures

<!-- How to debug failing tests -->

## Pre-commit

<!-- If using pre-commit hooks:
- Install: `pre-commit install`
- Run manually: `pre-commit run --all-files`
-->
