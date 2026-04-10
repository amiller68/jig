---
description: Respond to an automated code review. Use when you receive a review nudge pointing to a .jig/reviews/NNN.md file.
allowed-tools:
  - Read
  - Glob
  - Grep
  - Edit
  - Bash(git:*)
  - Bash(jig review respond:*)
  - Bash(cargo:*)
---

Respond to an automated review of your code.

## Steps

### 1. Read the review
Read the review file specified in the nudge (e.g. `.jig/reviews/001.md`).

### 2. Address each finding
For each finding:
- **If it's a real issue**: fix it in the code
- **If you disagree**: prepare an explanation
- **If it's out of scope**: note why you're deferring

### 3. Submit your response
Pipe your response to `jig review respond --review N`:

```
cat <<'EOF' | jig review respond --review N
# Response to Review NNN

## Addressed
- `file:line` — finding description: what you did to fix it

## Disputed
- `file:line` — finding description: why you disagree

## Deferred
- `file:line` — finding description: why this is out of scope

## Notes
Any additional context.
EOF
```

### 4. Commit and push
Commit your fixes (conventional commit format) and push. The next review cycle triggers automatically.
