#!/usr/bin/env python3
"""Clautist ANSI slab — two outputs, and they make different claims.

The shipped `sheet.png` cannot be re-synthesised. I tried, and the measurements
say why:

  * Its 22 frames carry **five different palettes for the same logical colour**
    (ink is `(32,29,27)` in frames 0-2, `(36,30,28)` in 3-14, `(34,30,28)` in
    15-17, `(36,31,29)` in 18-19, `(22,21,21)` in 20-21).
  * The drift is not a uniform dim. From frame 0 to frame 3 the rust darkens
    ~15% while the blue channel *rises* 39. A designed dim moves every channel
    one way.
  * No frame is a pure recolour of any other (no consistent colour bijection
    exists between any pair), and pixel *roles* are not stable either — only
    ~484 of 576 positions hold the same luminance rank across the base
    silhouette group.

So the sheet was exported in batches by some pipeline that is not in this repo,
and the drift is an artefact of that, not authorship. Two honest outputs follow
from that, and the whole point is that they are kept apart:

**(A) legacy-exact** — a *capture*. It replays recorded pixels and asserts the
result hashes to the shipped file. This is observed compatibility data, not a
reconstruction, and it makes no claim that any of those five palettes was
intended. Its only jobs are to pin the current bytes and to fail loudly if live
art drifts underneath us.

**(B) normalized-preview** — a real generator. Every frame is drawn
parametrically from the single authored palette (the frames 3-14 group, which
matches the authored constants exactly). This is the honest candidate. It is
**preview-only**: it requires an explicit output directory, refuses to write
anywhere near the installed assets, and has no default output path, no install
step, and no overwrite. Promoting its output to live art is a separate,
approval-gated act that this file deliberately cannot perform.

    ./generator.py verify                      # (A) capture still matches
    ./generator.py preview --out /tmp/clautist # (B) candidate + evidence

"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw

HERE = Path(__file__).resolve().parent
SHEET = HERE / "sheet.png"
MANIFEST = HERE / "avatar.json"

W = FRAME = 24
FRAME_COUNT = 22

# The shipped sheet as of 2026-07-26. (A) asserts against this.
LEGACY_SHA256 = "7b6853e07cd3c7d9c987a771fb13ac99d4176e7f6968c3db7cff1b6f71c696ea"

# ---------------------------------------------------------------- (B) palette
# The single authored palette. These are the frames 3-14 values, which match the
# hand-rolled source constants exactly; the other four groups are export drift.
INK = (36, 30, 28, 255)
SCLERA = (254, 252, 241, 255)
GOLD = (240, 180, 70, 255)
YELLOW = (255, 211, 49, 255)
RUST = (191, 83, 51, 255)
TEAL = (55, 195, 182, 255)
BLUE = (86, 149, 249, 255)
PURPLE = (163, 95, 225, 255)
GREEN = (119, 232, 132, 255)
CLEAR = (0, 0, 0, 0)

WEDGES = [PURPLE, BLUE, TEAL, YELLOW, RUST]

# Geometry of the eye, lifted from the shipped silhouette.
SCLERA_BOX = (1, 6, 22, 17)
IRIS_BOX = (6, 5, 17, 18)
PUPIL_BOX = (10, 10, 13, 13)


def blank() -> Image.Image:
    return Image.new("RGBA", (W, W), CLEAR)


def draw_eye(
    im: Image.Image,
    rotation: int = 0,
    sclera_box=SCLERA_BOX,
    iris_box=IRIS_BOX,
    pupil_box=PUPIL_BOX,
    glint: bool = True,
) -> None:
    """One eye, iris clipped by the sclera so the lid cuts it.

    Without the clip the pupil leaks into transparent gaps and the whole thing
    reads as a punched hole rather than an eye — that was round 1's defect.
    """
    d = ImageDraw.Draw(im)
    d.ellipse(sclera_box, fill=SCLERA)

    iris = blank()
    di = ImageDraw.Draw(iris)
    step = 360 // len(WEDGES)
    for i, colour in enumerate(WEDGES):
        a0 = -90 + rotation + i * step
        di.pieslice(iris_box, a0, a0 + step, fill=colour)

    mask = Image.new("L", (W, W), 0)
    ImageDraw.Draw(mask).ellipse(sclera_box, fill=255)
    im.paste(iris, (0, 0), ImageChops.darker(iris.getchannel("A"), mask))

    d.ellipse(pupil_box, fill=INK)
    if glint:
        d.point((pupil_box[0] + 1, pupil_box[1] + 1), fill=SCLERA)
    d.ellipse(sclera_box, outline=INK)


def lid(closed: float, rotation: int = 0, dim_factor: float | None = None) -> Image.Image:
    """The eye at `closed` (0 open, 1 shut), drawn narrowed — not cropped.

    Cropping an open eye is the obvious implementation and it is wrong: it
    slices the sclera's curve flat, so the frame reads as a letterbox band
    rather than a lid coming down. Narrowing the sclera box and re-clipping the
    iris to it keeps the almond and closes it the way an eye closes.
    """
    x0, top, x1, bottom = SCLERA_BOX
    mid = (top + bottom) / 2
    new_top = round(top + (mid - top) * closed)
    new_bottom = round(bottom - (bottom - mid) * closed)

    im = blank()
    d = ImageDraw.Draw(im)
    if new_bottom - new_top < 2:
        # Shut: the lash line alone, with the outer corners still reading.
        y = round(mid)
        d.line([(x0 + 1, y), (x1 - 1, y)], fill=INK)
        d.line([(x0 + 3, y + 1), (x1 - 3, y + 1)], fill=INK)
        d.point([(x0, y - 1), (x1, y - 1)], fill=INK)
        return im

    draw_eye(
        im,
        rotation,
        sclera_box=(x0, new_top, x1, new_bottom),
        # Keep the iris/pupil centred on the narrowed opening so the pupil is
        # occluded by the lid instead of sliding out from under it.
        iris_box=(IRIS_BOX[0], new_top - 1, IRIS_BOX[2], new_bottom + 1),
        pupil_box=(
            PUPIL_BOX[0],
            max(new_top, round(mid) - 1),
            PUPIL_BOX[2],
            min(new_bottom, round(mid) + 1),
        ),
        glint=closed < 0.5,
    )
    return dim(im, dim_factor) if dim_factor else im


def lower_lid(px: int, rotation: int = 0) -> Image.Image:
    """The eye with only the lower lid raised `px` pixels — a speech cue.

    The top lid and the whole upper silhouette stay put, so the movement is
    local and small, which is what the corpus says a talking frame is.
    """
    x0, top, x1, bottom = SCLERA_BOX
    im = blank()
    draw_eye(
        im,
        rotation,
        sclera_box=(x0, top, x1, bottom - px),
        iris_box=IRIS_BOX,
        pupil_box=(PUPIL_BOX[0], PUPIL_BOX[1], PUPIL_BOX[2], PUPIL_BOX[3] - px),
    )
    return im


def gold_ring(im: Image.Image) -> Image.Image:
    """Planning / tired-idle wear a gold frame around the lens."""
    out = im.copy()
    d = ImageDraw.Draw(out)
    d.ellipse((0, 4, 23, 19), outline=GOLD)
    d.ellipse((1, 5, 22, 18), outline=GOLD)
    for x in (0, 23):
        d.line([(x, 10), (x, 13)], fill=GOLD)
    return out


def green_arc(im: Image.Image, closed: float = 0.0) -> Image.Image:
    """Tired-running carries a green underline hugging the lower lid.

    Anchored to where the lid actually is, not to the open-eye geometry — at
    closed=0.3 the fixed arc detached and floated under the eye like a stray
    smile.
    """
    x0, top, x1, bottom = SCLERA_BOX
    mid = (top + bottom) / 2
    lid_bottom = round(bottom - (bottom - mid) * closed)
    out = im.copy()
    ImageDraw.Draw(out).arc((2, lid_bottom - 4, 21, lid_bottom + 4), 20, 160, fill=GREEN)
    return out


def dim(im: Image.Image, factor: float) -> Image.Image:
    """Uniform dim — every channel one way, which is what the shipped drift
    conspicuously is not."""
    out = im.copy()
    px = out.load()
    for y in range(W):
        for x in range(W):
            r, g, b, a = px[x, y]
            if a:
                px[x, y] = (int(r * factor), int(g * factor), int(b * factor), a)
    return out


def normalized_frames() -> list[Image.Image]:
    """All 22 frames from the one authored palette, in manifest order."""
    frames: list[Image.Image] = []

    def base(rotation: int = 0) -> Image.Image:
        im = blank()
        draw_eye(im, rotation)
        return im

    open_eye = base()
    frames.append(open_eye)                       # 0  idle, also talking rest
    frames.append(lid(0.55))                      # 1  blink mid
    frames.append(lid(1.0))                       # 2  blink shut

    # Iris rotations must all be distinct mod 360, frame 0 included — running
    # starts at 18, not 0, or frame 3 is a pixel-for-pixel duplicate of idle.
    for k in range(4):                            # 3-6   running
        frames.append(base(18 + k * 18))
    for k in range(2):                            # 7-8   waiting
        frames.append(base(90 + k * 36))
    for k in range(2):                            # 9-10  review
        frames.append(base(162 + k * 18))
    for k in range(2):                            # 11-12 failed
        frames.append(base(216 + k * 36))
    for k in range(2):                            # 13-14 planning, pre-ring
        frames.append(base(288 + k * 18))
    frames.append(gold_ring(base(324)))           # 15    planning, ring beat

    # Tired reads as a heavy lid, not just a dim — the lid is the part that
    # survives the statusline downsample.
    tired = dim(open_eye, 0.82)
    frames.append(gold_ring(tired))                        # 16 tired-idle
    frames.append(gold_ring(lid(0.45, dim_factor=0.82)))   # 17 tired-idle, lid
    frames.append(green_arc(lid(0.30, dim_factor=0.82), 0.30))       # 18 tired-run
    frames.append(green_arc(lid(0.45, 36, dim_factor=0.82), 0.45))   # 19 tired-run

    # Talking: the lower lid alone, by 1 and 2 px. This character has no mouth,
    # and my first two attempts both missed by trusting my eye over the corpus.
    # Measured across 94 talking frames in the shipped sets, the median frame
    # moves 4% of the character and the highest legitimate one is 35%; a full
    # squint moves 59-75% and reads as a pose change, which is why the shipped
    # frames (100%) strobe. A cue is small and local.
    frames.append(lower_lid(1))                   # 20    talking
    frames.append(lower_lid(2))                   # 21    talking

    assert len(frames) == FRAME_COUNT, len(frames)
    return frames


def to_strip(frames: list[Image.Image]) -> Image.Image:
    strip = Image.new("RGBA", (FRAME * FRAME_COUNT, FRAME), CLEAR)
    for i, f in enumerate(frames):
        # Binary alpha: half-block has no way to show anything in between, and
        # a value near the 128 cut renders unpredictably.
        f = f.copy()
        f.putalpha(f.getchannel("A").point(lambda v: 255 if v >= 128 else 0))
        strip.paste(f, (i * FRAME, 0))
    return strip


# --------------------------------------------------------------- (A) capture


def legacy_capture() -> Image.Image:
    """Return the shipped sheet, asserting it is the artefact we recorded.

    This is a capture and nothing more. It does not reconstruct the sheet and
    must never be described as doing so — see the module docstring for the
    measurements that rule reconstruction out.
    """
    if not SHEET.is_file():
        raise SystemExit(f"legacy capture: {SHEET} is missing")
    data = SHEET.read_bytes()
    got = hashlib.sha256(data).hexdigest()
    if got != LEGACY_SHA256:
        raise SystemExit(
            "legacy capture MISMATCH — the live sheet has changed underneath "
            f"this capture.\n  recorded {LEGACY_SHA256}\n  on disk   {got}\n"
            "Re-measure the palette groups before trusting anything here."
        )
    return Image.open(SHEET).convert("RGBA")


# ------------------------------------------------------------------ evidence


def frames_of(strip: Image.Image) -> list[Image.Image]:
    return [strip.crop((i * FRAME, 0, (i + 1) * FRAME, FRAME)) for i in range(FRAME_COUNT)]


def deltas(legacy: Image.Image, candidate: Image.Image) -> list[dict]:
    out = []
    for i, (a, b) in enumerate(zip(frames_of(legacy), frames_of(candidate))):
        changed = [
            (x, y)
            for y in range(FRAME)
            for x in range(FRAME)
            if a.getpixel((x, y)) != b.getpixel((x, y))
        ]
        opaque_a = sum(1 for y in range(FRAME) for x in range(FRAME) if a.getpixel((x, y))[3])
        opaque_b = sum(1 for y in range(FRAME) for x in range(FRAME) if b.getpixel((x, y))[3])
        out.append(
            {
                "frame": i,
                "changed_px": len(changed),
                "legacy_opaque": opaque_a,
                "candidate_opaque": opaque_b,
                "rows": [min((y for _, y in changed), default=-1),
                         max((y for _, y in changed), default=-1)],
            }
        )
    return out


def contact_sheet(legacy: Image.Image, candidate: Image.Image, out: Path) -> Path:
    """Side-by-side contact sheet: legacy row above, candidate row below."""
    scale = 4
    pad = 2
    bg = (30, 30, 34, 255)
    cols = FRAME_COUNT
    cw = FRAME * scale + pad
    sheet = Image.new("RGBA", (cols * cw + pad, 2 * (FRAME * scale + pad) + pad + 10), bg)
    for row, strip in enumerate((legacy, candidate)):
        for i, f in enumerate(frames_of(strip)):
            tile = Image.alpha_composite(Image.new("RGBA", (FRAME, FRAME), bg), f)
            sheet.paste(
                tile.resize((FRAME * scale, FRAME * scale), Image.NEAREST),
                (pad + i * cw, pad + row * (FRAME * scale + pad)),
            )
    sheet.save(out)
    return out


# ---------------------------------------------------------------------- cli


def guard_output_dir(out: Path) -> Path:
    """Preview output must not land on or near installed assets.

    There is deliberately no default. A preview generator that can write to the
    live path by omission is one flag away from being an install.
    """
    out = out.expanduser().resolve()
    forbidden = [HERE, HERE.parent, Path.home() / ".codex" / "avatars"]
    for bad in forbidden:
        if out == bad or bad in out.parents or out in bad.parents:
            raise SystemExit(
                f"refusing to write preview output to {out}: it is inside or "
                f"above the installed asset tree ({bad}). Promotion of a "
                f"normalized sheet is an approval-gated act and this tool "
                f"cannot perform it."
            )
    out.mkdir(parents=True, exist_ok=True)
    return out


def cmd_verify(_args) -> int:
    legacy_capture()
    manifest = json.loads(MANIFEST.read_text())
    fr = manifest["frame"]
    assert (fr["width"], fr["height"], fr["columns"]) == (FRAME, FRAME, FRAME_COUNT)
    print(f"legacy capture OK — sheet.png matches {LEGACY_SHA256[:16]}… and the manifest")
    return 0


def cmd_preview(args) -> int:
    out = guard_output_dir(Path(args.out))
    legacy = legacy_capture()
    candidate = to_strip(normalized_frames())

    candidate.save(out / "sheet-normalized-candidate.png")
    report = deltas(legacy, candidate)
    (out / "deltas.json").write_text(json.dumps(report, indent=2))
    contact_sheet(legacy, candidate, out / "contact-sheet.png")

    total = sum(r["changed_px"] for r in report)
    print(f"normalized candidate written to {out}  (NOT installed, NOT live)")
    print(f"  sheet-normalized-candidate.png   22 frames, one authored palette")
    print(f"  contact-sheet.png                legacy row above, candidate below")
    print(f"  deltas.json                      {total} pixels differ across the sheet")
    print()
    print("  frame  changed  legacy→candidate opaque")
    for r in report:
        print(
            f"  {r['frame']:5}  {r['changed_px']:7}  "
            f"{r['legacy_opaque']:6} → {r['candidate_opaque']}"
        )
    print()
    print("Promotion of this candidate to live art is a separate, reviewed step.")
    return 0


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)

    v = sub.add_parser("verify", help="(A) assert the legacy capture still matches")
    v.set_defaults(fn=cmd_verify)

    p = sub.add_parser("preview", help="(B) build the normalized candidate + evidence")
    p.add_argument("--out", required=True, help="output dir (required, no default)")
    p.set_defaults(fn=cmd_preview)

    args = ap.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
