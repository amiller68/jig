# Releasing

Automated release workflow using conventional commits and cargo-smart-release.

## PAT Setup

The release pipeline requires a **Personal Access Token** stored as `RELEASE_PAT` in GitHub Actions secrets. This is needed because tags pushed by the default `GITHUB_TOKEN` don't trigger downstream workflows — without a PAT, the tag push from `release-tag.yml` won't kick off the binary build in `release.yml`.

### Creating the token

1. Go to **GitHub → Settings → Developer settings → Personal access tokens → Fine-grained tokens**
2. Create a new token scoped to the `jig` repository with these permissions:
   - **Contents**: Read and write (push tags)
   - **Workflows**: Read and write (trigger workflow runs)
3. Set expiration as desired (90 days is fine, you'll get an email before it expires)

### Adding to the repository

1. Go to **jig repo → Settings → Secrets and variables → Actions**
2. Add a new repository secret named `RELEASE_PAT` with the token value

### When to rotate

GitHub sends expiration reminders. When the token expires, tag creation in `release-tag.yml` will fail silently (the tag step uses the PAT for checkout). Create a new token and update the secret.

## How It Works

1. **Push to main** triggers `.github/workflows/release-pr.yml`
2. **cargo-smart-release** analyzes conventional commits since last tag
3. **Version bumps** are determined automatically:
   - `feat:` commits → minor bump (0.x.0)
   - `fix:` commits → patch bump (0.0.x)
   - `BREAKING CHANGE:` → major bump (x.0.0)
4. **Release PR** is created/updated with version bumps and changelogs
5. **Merge PR** → tag is created automatically
6. **Tag push** triggers `.github/workflows/release.yml` → builds binaries

## Commit Message Format

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add new command for X
fix: prevent crash when Y
docs: update README
chore: update dependencies
refactor: simplify error handling
test: add integration tests for Z
```

Breaking changes:
```
feat!: change API for spawn command
```
or:
```
feat: change API for spawn command

BREAKING CHANGE: spawn now requires --context flag
```

## Release PR

When conventional commits are pushed to main, a release PR is automatically created:
- **Branch**: `release-automation`
- **Contains**: Version bumps in Cargo.toml, changelog updates
- **Review**: Verify versions and changelog entries are correct

## Manual Release

If automation fails, you can release manually:

```bash
# Install cargo-smart-release
cargo install cargo-smart-release

# Preview what would happen
cargo smart-release jig-cli --no-publish

# Execute (bumps versions, updates changelogs)
cargo smart-release jig-cli --execute --no-publish --no-tag

# Commit and push
git add -A
git commit -m "chore: release v0.X.0"
git push

# Tag and push (triggers binary builds)
git tag v0.X.0
git push --tags
```

## Troubleshooting

### Version not bumping

If `cargo-smart-release` isn't bumping versions despite feat commits:

1. Check commits since last tag:
   ```bash
   git log v0.4.0..HEAD --oneline --grep="^feat"
   ```

2. Force a specific bump:
   ```bash
   cargo smart-release jig-cli --execute --no-publish --no-tag --bump minor
   ```

### Release PR not created

Check workflow runs:
```bash
gh run list --workflow="Create Release PR"
gh run view <run-id> --log
```

### Tag already exists

If trying to release a version that's already tagged:
```bash
git tag -d v0.X.0           # Delete local tag
git push --delete origin v0.X.0  # Delete remote tag (careful!)
```
