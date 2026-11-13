# HT Fork Management Guide

This document describes how Heiervang Technologies maintains forks of upstream open-source projects.

For questions about this fork, use the [HT Discussions page](https://github.com/orgs/heiervang-technologies/discussions).

## Branch Structure

Each fork has two key branches:

| Branch        | Purpose                                | Default?                 |
| ------------- | -------------------------------------- | ------------------------ |
| **`clanker`** | HT-specific changes on top of upstream | **Yes** (default branch) |
| **`main`**    | Clean mirror of upstream `main`        | No                       |

### `main` — Upstream Mirror

- Always a clean fast-forward of the upstream `main` branch
- **Never** commit HT-specific changes to `main`
- Enable GitHub's "Sync fork" button or use `git fetch upstream && git merge --ff-only upstream/main`
- Used as the merge base when syncing upstream changes into `clanker`

### `clanker` — Default Branch

- Contains all HT-specific features, fixes, and configuration on top of `main`
- Set as the repo's **default branch** on GitHub so that clones, PRs, and the README all reference it
- All PRs target `clanker` unless they're upstream contributions

## Syncing with Upstream

The scheduled Fork Sync workflow fast-forwards `main` from upstream and sets
`skip_rebase: true`. Check the latest workflow result: a schedule alone does not
establish that the mirror is current. Integration into `clanker` is a separate,
reviewed merge PR.

Do not rebase or force-push the published `clanker` branch. Preserve upstream
ancestry with a merge commit, including when landing the integration PR; do not
squash or rebase that PR.

After verifying that the mirror sync succeeded, prepare an isolated worktree:

```bash
git fetch origin
git worktree add ~/worktrees/clanker-upstream-sync -b sync/upstream origin/clanker
cd ~/worktrees/clanker-upstream-sync
git merge --no-ff origin/main
# Resolve conflicts, review the downstream changes, and run the relevant checks.
git push -u origin sync/upstream
gh pr create --base clanker --title "chore: merge upstream main"
```

Before landing, verify the required CI checks on the final PR revision and
review that fork features survive the integration. For changes to authentication,
model discovery, or request handling, run the focused tests and a local ChatGPT
sign-in smoke test covering a model response and tool execution. Keep credentials
out of CI logs and repository files. Retain a known-working release for rollback.

## Feature Development

1. Create a feature branch from `clanker`:
   ```bash
   git fetch origin
   git worktree add ~/worktrees/my-feature -b feat/my-feature origin/clanker
   cd ~/worktrees/my-feature
   ```
2. Develop, commit, push
3. Open a PR targeting `clanker`
4. Squash-merge the PR
5. Delete the feature branch after merge

## Commit Standards

- Use [conventional commits](https://www.conventionalcommits.org/) (e.g., `feat:`, `fix:`, `chore:`)
- Squash ordinary feature PRs; preserve merge commits for upstream integration
- One commit per feature or logical change

## Questions & Discussion

For all inquiries about this fork, use the [HT Discussions page](https://github.com/orgs/heiervang-technologies/discussions).

---

## Upstream Contributing

For non-fork-specific changes, follow the [upstream contribution guidelines](./docs/contributing.md).
