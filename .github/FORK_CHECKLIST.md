## HT Fork Setup Checklist

Reusable checklist for initializing any new HT fork. Run through this after forking an upstream repo.

### Prerequisites

- [ ] Fork created under [heiervang-technologies](https://github.com/heiervang-technologies) with `ht-` prefix (e.g. `ht-vllm-omni`) — verify with `gh api repos/heiervang-technologies/<name> --jq .name` (404 means the fork does not exist yet; do not proceed with the rest of the checklist)
- [ ] `origin` points to the HT org fork, not upstream: `git remote get-url origin` should return `...heiervang-technologies/ht-<name>...`. If it returns the upstream URL, the directory is a reference clone, not a fork — either re-clone from the HT fork or `git remote set-url origin <ht-fork-url>` before continuing
- [ ] Upstream remote added: `git remote add upstream <upstream-url>`

### Branch setup

- [ ] `main` branch is a clean fast-forward mirror of upstream — **never commit directly**
- [ ] Create `ht` branch from `main`: `git checkout -b ht main`
- [ ] Set `ht` as the **default branch** in GitHub repo settings
- [ ] Push `ht` to origin: `git push -u origin ht`

### First commit on `ht` (docs — single commit)

All of the following changes go into **one commit** with message `docs: add ht-fork documentation and discussion links`:

- [ ] **README.md** — Prefix title with `ht-` (e.g. `ht-ACE-Step 1.5`)
- [ ] **README.md** — Add HT fork subtitle: _"Heiervang Technologies fork of [Upstream](url)"_
- [ ] **README.md** — Add link bar with:
  - [HT Discussions](https://github.com/orgs/heiervang-technologies/discussions)
  - [Fork Management Guide](https://github.com/orgs/heiervang-technologies/discussions/3)
  - Upstream project link
- [ ] **README.md** — Add `## HT Fork Changes` section with:
  - Description of what this fork is
  - Table of changes vs upstream (columns: Change, Description, Contributed back?)
  - Branch strategy explanation (`main` = upstream mirror, `ht` = HT changes)
  - Link to [HT Discussions](https://github.com/orgs/heiervang-technologies/discussions) for questions/inquiries
  - Link to [Fork Management Guide](https://github.com/orgs/heiervang-technologies/discussions/3)
- [ ] **CONTRIBUTING.md** — Add `## HT Fork Management` section **on top** of existing content, with:
  - Link to [Fork Management Guide](https://github.com/orgs/heiervang-technologies/discussions/3)
  - Branch conventions (`main`, `ht`, feature branches from `ht`)
  - Sync workflow (fast-forward main, rebase ht, force-push)
  - Commit standards (conventional commits, linear history)
  - Link to [HT Discussions](https://github.com/orgs/heiervang-technologies/discussions) for all inquiries
- [ ] If no CONTRIBUTING.md exists, create one with the HT section + a brief note to follow upstream contribution guidelines for non-fork-specific changes

### Workflow management

- [ ] Disable all upstream workflows: `gh workflow list -R heiervang-technologies/<repo> --all` then `gh workflow disable <id> -R ...` for each non-HT workflow
- [ ] Enable pre-commit/lint CI if upstream has one: `gh workflow enable <id> -R ...`
- [ ] If the enabled CI workflow only triggers on `main`/`master`, add `ht` to `on.push.branches` and/or `on.pull_request.branches` in the workflow YAML, commit to `ht`

### Post-setup
- [ ] Enable issues in GitHub repo settings (`gh repo edit --enable-issues`)

- [ ] Verify `ht` branch has clean linear history on top of `main`
- [ ] Force-push `ht` if history was rewritten: `git push --force-with-lease origin ht`
- [ ] Confirm default branch is `ht` in GitHub settings

### Ongoing maintenance

- [ ] Keep `main` synced with upstream via fast-forward
- [ ] Rebase `ht` onto `main` after sync — notify teammates before force-push
- [ ] Update the HT Fork Changes table in README when adding/removing HT-specific changes
- [ ] Feature branches: create from `ht`, squash-merge back via PR

---

See the [Fork Management Guide](https://github.com/orgs/heiervang-technologies/discussions/3) for full details on branch conventions, sync workflow, and contribution process.

For questions or discussion about any HT fork, use the [HT Discussions page](https://github.com/orgs/heiervang-technologies/discussions).
