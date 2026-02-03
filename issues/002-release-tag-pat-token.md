# Use PAT in release-tag workflow to trigger release builds

**Status:** Planned

## Objective

Configure `release-tag.yml` to use a PAT (`secrets.PAT_TOKEN`) instead of the default `GITHUB_TOKEN` so that tags it pushes trigger the `release.yml` workflow.

## Background

GitHub Actions does not trigger downstream workflows from events created with the default `GITHUB_TOKEN` (anti-recursion safeguard). The `release-tag.yml` workflow creates and pushes a git tag, which should trigger `release.yml` (listens on `push: tags: v*`), but because the tag is pushed with `GITHUB_TOKEN`, the release build never fires.

## Implementation Steps

1. Create a GitHub PAT with `contents: write` scope and add it as `PAT_TOKEN` repo secret
2. Update `.github/workflows/release-tag.yml` checkout step to use the PAT:
   ```yaml
   - uses: actions/checkout@v4
     with:
       fetch-depth: 0
       token: ${{ secrets.PAT_TOKEN }}
   ```
3. The PAT will be used for the subsequent `git push origin "$TAG"`, which will trigger `release.yml`

## Files to Modify

- `.github/workflows/release-tag.yml` - Use `secrets.PAT_TOKEN` in checkout

## Acceptance Criteria

- [ ] `PAT_TOKEN` secret is configured on the repo
- [ ] `release-tag.yml` uses the PAT for checkout and push
- [ ] Merging a release PR creates a tag that triggers the full release build
