---
description: Search and navigate project documentation. Use when you need to understand how something works, find conventions, or check if docs exist for a source file.
allowed-tools:
  - Read
  - Glob
  - Grep
---

Find and read the right project documentation for your current task.

## Steps

### 1. Read the doc index

```
Read docs/index.md
```

The **Documentation Map** lists every doc with:
- **Summary** — what the doc covers
- **Sources** — which source files it documents

### 2. Match your task to docs

Look at the files you're about to touch. Check the Sources column in the index:
- If your files appear in a Sources column, read those docs before coding
- If your files don't appear anywhere, check if a nearby directory is covered (e.g., `daemon/foo.rs` is covered by `daemon.md` which lists `daemon/*.rs`)

### 3. Search docs by keyword

If the index doesn't answer your question, search across all docs:

```
Grep pattern="your keyword" path="docs/" output_mode="content"
```

### 4. Check for gaps

If you're working on source files that no doc covers:
- Note it — this is an orphan that may need documentation later
- Check `PATTERNS.md` for general conventions that still apply

## When to update docs

After finishing your task, check:
- Did you change files listed in a doc's Sources column? If so, check if the doc content is still accurate
- Did you add a new module or command? Consider whether it needs a doc entry in the index
- Did you change behavior described in a doc? Update the doc

Update `docs/index.md` Sources columns if you add new files that an existing doc should cover.
