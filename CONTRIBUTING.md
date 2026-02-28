# HT Fork Management Guide

This document describes how Heiervang Technologies maintains forks of upstream open-source projects.

For the full guide, see the [HT Fork Management Guide](https://github.com/orgs/heiervang-technologies/discussions/3).

## Branch Structure

Each fork has two key branches:

| Branch | Purpose | Default? |
|--------|---------|----------|
| **`ht`** | HT-specific changes on top of upstream | **Yes** (default branch) |
| **`main`** | Clean mirror of upstream `main` | No |

### `main` — Upstream Mirror

- Always a clean fast-forward of the upstream `main` branch
- **Never** commit HT-specific changes to `main`
- Enable GitHub's "Sync fork" button or use `git fetch upstream && git merge --ff-only upstream/main`
- Used as the merge base when syncing upstream changes into `ht`

### `ht` — Default Branch

- Contains all HT-specific features, fixes, and configuration on top of `main`
- Set as the repo's **default branch** on GitHub so that clones, PRs, and the README all reference it
- All PRs target `ht` unless they're upstream contributions

## Syncing with Upstream

When upstream `main` advances:

```bash
# 1. Update local main
git checkout main
git fetch upstream
git merge --ff-only upstream/main
git push origin main

# 2. Rebase ht onto updated main
git checkout ht
git rebase main

# 3. Resolve any conflicts, then force-push
git push --force-with-lease origin ht
```

## Feature Development

1. Create a feature branch from `ht`:
   ```bash
   git checkout -b feat/my-feature ht
   ```
2. Develop, commit, push
3. Open a PR targeting `ht`
4. Squash-merge the PR
5. Delete the feature branch after merge

## Commit Standards

- Use [conventional commits](https://www.conventionalcommits.org/) (e.g., `feat:`, `fix:`, `chore:`)
- Maintain clean, linear history — no merge commits from sync operations
- One commit per feature or logical change

## Questions & Discussion

For all inquiries about this fork, use the [HT Discussions page](https://github.com/orgs/heiervang-technologies/discussions).

---

## Upstream Contributing

For non-fork-specific changes, follow the [upstream contribution guidelines](./docs/contributing.md).
