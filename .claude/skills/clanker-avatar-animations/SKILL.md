---
name: clanker-avatar-animations
description: Author animated avatar state sets (spritesheet + avatar.json) for the Clanker Code pets TUI — state wheel, talking mouth-flap, context-tier variants, QA checklist. Use when creating or extending a custom pet avatar's animations.
---

# Clanker Avatar Animations

How to author a full animated state set for a Clanker Code pet avatar.
Ground truth lives in `codex-rs/tui/src/pets/` (`model.rs` = manifest,
`ambient.rs` = state wheel, `talking_signal.rs` = talking flag).

## Install target

```
~/.codex/avatars/<id>/
  sheet.png       # horizontal strip: N frames of WxH (24x24 is the norm), 1 row
  avatar.json     # manifest, camelCase keys
```

`~/.codex/pets/<id>/pet.json` is also scanned. Selector is `custom:<id>`
(picker: `/pets`). The fork default is `custom:clanker`. Bundled reference
sets: `codex-rs/tui/assets/<id>/`.

## Manifest

```json
{
  "displayName": "C3PH0",
  "renderMode": "ansi-half-block",
  "spritesheetPath": "sheet.png",
  "frame": { "width": 24, "height": 24, "columns": 22, "rows": 1 },
  "animations": {
    "idle":    { "frames": [0, 0, 0, 1, 0, 0, 2, 2], "fps": 8 },
    "running": { "frames": [3, 4, 5, 6], "fps": 12 },
    "wave":    { "frames": [7, 8], "fps": 4, "loop": false, "fallback": "idle" }
  }
}
```

- Frame indices are **0-based** into the strip; repeat an index to hold it.
- The `frame` block is required for multi-frame sheets.
- `loop: false` makes a one-shot; its `fallback` must name an existing
  animation or the manifest is rejected ("animation X fallback Y does not
  exist").
- Alpha must be binary — threshold at >=128, no soft edges. Hard-edged
  pixels only; the renderer is ANSI half-block (2 px per character cell).

## The state wheel (ambient.rs)

Base states the TUI drives, in the order you should author them:

| animation | trigger |
|-----------|---------|
| `idle`    | default / rest |
| `running` | agent working |
| `waiting` | needs input |
| `review`  | ready for review |
| `failed`  | blocked / error |
| `planning`| plan mode |
| `talking` | say audio playing (highest priority) |

Missing states fall back gracefully (`talking` → `running` → `idle`), so
ship `idle` first and grow the set — but a full set covers all seven.

### Context tiers (optional overrides)

`idle` and `running` can carry tier variants keyed off context used:

| used % | tier | variants |
|--------|------|----------|
| 0–24   | fresh     | `fresh-idle`, `fresh-running` |
| 25–49  | focused   | `focused-idle`, `focused-running` |
| 50–74  | tired     | `tired-idle`, `tired-running` |
| 75–100 | exhausted | `exhausted-idle`, `exhausted-running` |

Absent variants fall back to the base animation. The shipped sets do the
tired pair (coffee, steam, sweat, dimmed glow) and skip fresh/focused.

## Talking

`talking_signal.rs` polls `~/.local/share/agent-tools/talking/<agent>.json`
(agent name from `AGENT_PERSONA`, else director session name). `say` holds
the flag while audio plays; no wiring needed on the avatar side.

Convention (locked across shipped sets): cycle `[BASE, half-open, open,
half-open]` at fps 8, where BASE is idle frame 0. **Only the jaw/mouth
pixels move** — QA diffs talking frames against BASE and flags anything
outside the mouth region.

## Authoring method

Parameterize one `frame(...)` function instead of copy-pasting frames:

```python
def frame(dy=0, eyes="open", mouth="closed", extras=()):
    img = Image.new("RGBA", (24, 24), (0, 0, 0, 0))
    ...draw with ImageDraw primitives only, whole-pixel offsets...
    return img
```

- Motion legibility at 24px: move whole pixels; 2-frame alternation for
  idle-ish states, 3–4 frames for `running` (head bob `dy`, limb swing).
- `failed` reads best static or 2-frame at low fps (X eyes, droop).
- Assemble the strip with PIL paste at `x = index * width` (or
  `magick +append`), save as `sheet.png`.
- Preview gif: `magick -dispose Background -delay 12 -loop 0 f*.png out.gif`;
  upscale with `-filter point` only.

Frame-layout convention from the shipped 22-column sets (a convention, not
a contract — indices are whatever your manifest says): idle 0–2,
running 3–6, waiting 7–8, review 9–10, failed 11–12, planning 13–15,
tired 16–19, talking mouths 20–21 (cycle uses `[0, 20, 21, 20]`).

## QA checklist before handoff

1. Strip width == `columns * frame.width`, single row.
2. Every pixel alpha is 0 or 255 (or at least unambiguous at the 128 cut).
3. Every `fallback` names an existing animation; every frame index < columns.
4. Talking frames diff against frame 0 confined to the mouth/jaw rows.
5. Load it live: drop the dir in `~/.codex/avatars/`, run `/pets`, select
   `custom:<id>`, and walk the wheel (run a turn, trigger say, burn context
   past 50% or temporarily remap `tired-idle` to eyeball it).
