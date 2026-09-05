# Changelog

Clanker Code is a fork of [OpenAI Codex](https://github.com/openai/codex). This
file documents what Clanker Code adds **on top of** upstream Codex.

For upstream Codex changes, see the
[Codex releases page](https://github.com/openai/codex/releases).

The current release and its exact upstream provenance are recorded in
`codex-rs/config/src/clanker_version.rs`:

```
0.1.0+codex.0.143.0-alpha.10.355.g5c19155cbd93
└─ Clanker  └─ upstream Codex release the fork is rebased on
```

---

## Unreleased

### Added

- **Clanker build and install Makefile** — `make` targets for building and
  installing the Clanker Code binary locally.

---

## Clanker features on top of Codex

Everything below is Clanker-specific and has no upstream Codex equivalent.

### Characters

A first-class notion of an agent *character* — a named identity with an avatar,
declared aliases, and a validated manifest.

- **`codex-rs/character/` crate** — character manifest schema
  (`CHARACTER_SCHEMA_VERSION = 1`), avatar pack validation, canonical-id and
  alias resolution, and a local character registry with global collision
  detection.
- **`clanker character` CLI** (`codex-rs/cli/src/character_cmd.rs`):
  - `validate` — validate one manifest, or the complete local registry with
    `--all` (including cross-manifest name collisions).
  - `resolve` — resolve a canonical id or declared alias against the local
    registry, with `--materialize` to install a built-in character first.
  - Both support emitting a stable machine-readable character contract.
- **Canonical avatar continuity** (#30) — an avatar stays bound to its character
  across sessions rather than being re-picked per run.
- **Continuity acceptance contract** (#28) — `scripts/character_continuity_acceptance.py`
  plus `scripts/character_continuity/contract.json` and fixtures, enforcing
  character/avatar continuity as an executable contract rather than a convention.

### Avatars & pets (TUI)

An animated avatar system rendered directly in the terminal UI.

- **`codex-rs/tui/src/pets/`** — avatar runtime: spritesheet model, animation
  frames, the `/pets` picker, live preview, and an ambient/idle driver.
- **Multiple render backends** — ANSI half-block, Sixel, and terminal image
  protocols, selected per terminal capability.
- **Talking signal** (`talking_signal.rs`) — drives mouth-flap animation from
  an external signal file, so the avatar animates while the agent speaks.
- **`codex-rs/tui/src/avatars/`** — asset loading, character↔avatar binding, and
  the avatar runtime that ties a resolved character to a rendered pet.
- **Avatar pack format** — 24×24 frames, 22-frame horizontal spritesheet,
  binary alpha, and nine semantic animation states (`idle`, `running`,
  `waiting`, `review`, `failed`, `planning`, `tired-idle`, `tired-running`,
  `talking`), declared in an `avatar.json` manifest.
- **Bundled character avatars** — `clanker`, `chloe`, `c3ph0`, `hai`,
  `clautist`, `jasonelle`, and `centurion` resolve as named characters with a
  canonical avatar binding. The underlying packs are also installed into the
  pet catalog, including `chloe-r2-09` and its Locked In variant.
- **Reproducible assets** — generator scripts document and reproduce the
  Clautist and Centurion avatar sources; the Clautist generator keeps exact
  compatibility capture separate from normalized preview art.
- **Custom avatars** — loaded at runtime from `$CODEX_HOME/avatars/<id>/avatar.json`;
  the bundled default is auto-installed on first use.

### Character-scoped memories

Memories are partitioned by character and project instead of being global.

- **Character-scoped continuity** (#29) — `codex-rs/memories/write/src/scope.rs`
  and `codex-rs/ext/memories/src/character_context.rs` resolve a memory scope
  from the current thread, character, project, and parent thread.
- **Schema migrations** — `0002_character_memory_scope.sql` adds a
  `thread_memory_scopes` table and `clanker_id` / `project_key` / `visibility`
  columns; `0003_scoped_phase2.sql` completes the scoped rollout.
- **Visibility model** — memories carry an explicit visibility, with
  `anonymous_legacy` preserving pre-migration rows.
- **App-server protocol** — adds `MemoryResetParams` / `MemoryResetScope` so
  memory can be reset at a chosen scope.

### Collaboration modes

Five Clanker-specific collaboration modes
(`codex-rs/collaboration-mode-templates/templates/`), each replacing any
previously active mode:

| Mode | Template |
|---|---|
| **LARP** (default) | `larp.md` |
| **Based** | `based.md` |
| **Locked In** | `locked_in.md` |
| **Ultrachill** | `ultrachill.md` |
| **Cringe** | `cringe.md` |

### Branding

- **Clanker Code product identity** — `PRODUCT_NAME` and `VERSION` in
  `codex-rs/config/src/clanker_version.rs`, surfaced across CLI and TUI.
- **`clanker` binary entrypoint** — `codex-rs/cli/src/clanker.rs`.
- **Version provenance** — the rebase pipeline stamps `VERSION` from the
  upstream commit's nearest `rust-v*` tag, tag distance, and 12-char revision,
  so every Clanker build states exactly which Codex it derives from.

### Build & tooling

- **Reduced Cargo target disk usage** (#31) — `scripts/cargo_target_size.py`.
- **Stabilized dependency refresh integration** (#46).
- **Fork sync workflow** — `.github/workflows/fork-sync.yml`.
- **`clanker-avatar-animations` skill** — `.claude/skills/`, for authoring
  animated avatar state sets against the avatar pack contract.

---

## Licensing

Clanker Code is distributed under the Apache License 2.0, inherited from
upstream Codex. See [`LICENSE`](./LICENSE) and [`NOTICE`](./NOTICE) for
attribution, including code derived from
[Ratatui](https://github.com/ratatui/ratatui) (MIT).
