---
name: check
description: Run project checks (build, test, lint, format). Use when validating code quality, preparing for merge, or verifying changes pass CI.
allowed-tools:
  - Bash(make:*)
  - Bash(npm:*)
  - Bash(pnpm:*)
  - Bash(yarn:*)
  - Bash(cargo:*)
  - Bash(go:*)
  - Bash(python:*)
  - Bash(pytest:*)
  - Bash(uv:*)
  - Bash(bundle:*)
  - Bash(rake:*)
  - Bash(./test.sh:*)
  - Bash(cat:*)
  - Bash(ls:*)
  - Read
  - Glob
  - Grep
---

Run the full success criteria checks to validate code quality.

## Steps

1. Detect the project type by checking for config files:
   - `Makefile` with a `check` or `test` target
   - `package.json` (Node.js — npm/pnpm/yarn)
   - `Cargo.toml` (Rust — cargo)
   - `go.mod` (Go)
   - `pyproject.toml` or `setup.py` (Python)
   - `Gemfile` (Ruby)
   - `test.sh` (shell-based test runner)

2. Run the appropriate checks for the detected project type:
   - **Makefile**: `make check` (or `make test` if no check target)
   - **Node.js**: `npm test` / `pnpm test` (check lockfile for package manager)
   - **Rust**: `cargo build`, `cargo test`, `cargo clippy`, `cargo fmt --check`
   - **Go**: `go build ./...`, `go test ./...`, `go vet ./...`
   - **Python**: `pytest` or `python -m pytest`
   - **Ruby**: `bundle exec rake test`
   - **Shell**: `./test.sh`

3. If formatting checks fail, attempt to auto-fix:
   - Rust: `cargo fmt`
   - Go: `gofmt -w .`
   - Node.js: `npm run format` (if script exists)

4. Report a summary of pass/fail status for each check run.

5. If any checks fail that cannot be auto-fixed, report what needs manual attention.

This is the gate for all PRs — all checks must pass before merge.
