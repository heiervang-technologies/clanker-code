#!/usr/bin/env python3
"""Default Clanker Code avatar: the rusty clanker.

A weathered rust-orange robot — patched chassis, loose bolt, squeaky antenna.
Hand-rolled per the clanker-avatars contract: 24x24 frames, hard edges,
binary alpha, horizontal strip. 22 frames covering all 9 states:

  0-2   idle          (hold, creak-flicker, blink)
  3-6   running       (bob + arm swing + speed lines)
  7-8   waiting       (eyes glance left/right, foot tap)
  9-10  review        (gold magnifier over one eye)
  11-12 failed        (X eyes, smoke, patch plate ajar)
  13-15 planning      (blueprint + pencil, lines accrue)
  16-17 tired-idle    (droopy antenna, half eyes, steam, coffee)
  18-19 tired-running (slouched bob, sweat drop)
  20-21 talking       (mouth half-open / open; closed reuses frame 0,
                       cycle [0,20,21,20] — calm pose, the mouth carries it)
"""
from pathlib import Path
from PIL import Image, ImageDraw

OUT = Path("/tmp/av24/clanker")
OUT.mkdir(parents=True, exist_ok=True)
N = 24

RUST = (186, 98, 46)
RUST_D = (140, 68, 34)
OXIDE = (110, 58, 40)
INKY = (40, 30, 26)
FACE = (52, 40, 34)
STEEL = (160, 155, 150)
STEEL_D = (100, 95, 92)
PATCH = (100, 122, 132)
AMBER = (255, 196, 64)
AMBER_DIM = (170, 130, 50)
BOLT = (214, 172, 92)
RED = (215, 90, 60)
SMOKE = (150, 145, 140)
STEAM = (200, 200, 205)
BLUE = (62, 100, 185)
BP_LINE = (225, 238, 255)
COFFEE = (120, 78, 40)
SWEAT = (120, 190, 240)


def mp(d, pts, fill):
    for x, y in pts:
        d.point([(x, y), (23 - x, y)], fill=fill)


def frame(head_dy=0, antenna="up", eyes="on", arms="rest", mouth="grill", extras=()):
    img = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    hy = head_dy  # vertical head offset

    # --- body (static; feet at the floor) -------------------------------
    d.rounded_rectangle([6, 14, 17, 22], radius=2, fill=RUST_D, outline=INKY)
    d.line([7, 18, 16, 18], fill=OXIDE)                     # chest seam
    d.point([(11, 16), (12, 16)], fill=BOLT)                # center bolt
    d.point([(7, 15), (16, 15), (7, 21), (16, 21)], fill=STEEL_D)  # rivets
    if "tap" in extras:
        d.rectangle([6, 21, 9, 22], fill=(0, 0, 0, 0))      # foot raised
        d.rectangle([6, 20, 9, 21], fill=STEEL_D)
    d.rectangle([6, 22, 9, 23], fill=STEEL_D)               # feet
    d.rectangle([14, 22, 17, 23], fill=STEEL_D)

    # --- arms ------------------------------------------------------------
    if arms == "rest":
        d.rectangle([4, 15, 5, 19], fill=RUST); d.rectangle([18, 15, 19, 19], fill=RUST)
    elif arms == "a":       # left up, right back
        d.rectangle([4, 13, 5, 16], fill=RUST); d.rectangle([18, 17, 19, 20], fill=RUST)
    elif arms == "b":       # right up, left back
        d.rectangle([4, 17, 5, 20], fill=RUST); d.rectangle([18, 13, 19, 16], fill=RUST)
    elif arms == "slump":
        d.rectangle([4, 16, 5, 21], fill=RUST); d.rectangle([18, 16, 19, 21], fill=RUST)

    # --- head -------------------------------------------------------------
    d.rounded_rectangle([5, 3 + hy, 18, 13 + hy], radius=2, fill=RUST, outline=INKY)
    d.rectangle([6, 11 + hy, 17, 12 + hy], fill=OXIDE)      # jaw shade
    d.point([(6, 4 + hy), (17, 4 + hy), (6, 12 + hy)], fill=STEEL)  # rivets
    # patch plate (welded scar) — top-left, with stitches
    if "patch_ajar" in extras:
        d.rectangle([6, 5 + hy, 9, 7 + hy], fill=FACE)      # gap behind
        d.rectangle([7, 6 + hy, 10, 8 + hy], fill=PATCH)
    else:
        d.rectangle([6, 5 + hy, 9, 7 + hy], fill=PATCH)
        d.point([(6, 5 + hy), (9, 7 + hy)], fill=STEEL_D)
    d.point([(18, 12 + hy)], fill=BOLT)                     # the loose bolt

    # --- face band + eyes ---------------------------------------------------
    d.rectangle([7, 6 + hy, 16, 9 + hy], fill=FACE)
    exl, exr = 8, 14                                        # eye x origins (2x2)
    ey = 7 + hy
    if eyes == "on":
        d.rectangle([exl, ey, exl + 1, ey + 1], fill=AMBER)
        d.rectangle([exr, ey, exr + 1, ey + 1], fill=AMBER)
        d.point([(exl, ey), (exr, ey)], fill=(255, 240, 180))
    elif eyes == "flicker":
        d.rectangle([exl, ey, exl + 1, ey + 1], fill=AMBER_DIM)
        d.rectangle([exr, ey, exr + 1, ey + 1], fill=AMBER)
        d.point([(exr, ey)], fill=(255, 240, 180))
    elif eyes == "blink":
        d.line([exl, ey + 1, exl + 1, ey + 1], fill=AMBER_DIM)
        d.line([exr, ey + 1, exr + 1, ey + 1], fill=AMBER_DIM)
    elif eyes == "left":
        d.rectangle([exl - 1, ey, exl, ey + 1], fill=AMBER)
        d.rectangle([exr - 1, ey, exr, ey + 1], fill=AMBER)
    elif eyes == "right":
        d.rectangle([exl + 1, ey, exl + 2, ey + 1], fill=AMBER)
        d.rectangle([exr + 1, ey, exr + 2, ey + 1], fill=AMBER)
    elif eyes == "x":
        for ex in (exl, exr):
            d.point([(ex, ey), (ex + 1, ey + 1), (ex + 1, ey), (ex, ey + 1)], fill=RED)
    elif eyes == "half":
        d.line([exl, ey + 1, exl + 1, ey + 1], fill=AMBER_DIM)
        d.line([exr, ey + 1, exr + 1, ey + 1], fill=AMBER_DIM)
        d.point([(exl, ey), (exr, ey)], fill=(90, 70, 50))
    elif eyes == "down":
        d.line([exl, ey + 1, exl + 1, ey + 1], fill=AMBER)
        d.line([exr, ey + 1, exr + 1, ey + 1], fill=AMBER)
    # mouth: grill when closed, dark opening with amber glow when talking
    if mouth == "grill":
        mp(d, [(9, 11 + hy), (11, 11 + hy)], INKY)
    elif mouth == "half":
        d.rectangle([10, 11 + hy, 13, 11 + hy], fill=INKY)
    elif mouth == "open":
        d.rectangle([10, 10 + hy, 13, 12 + hy], fill=INKY)
        d.point([(11, 12 + hy), (12, 12 + hy)], fill=AMBER_DIM)

    # --- antenna ------------------------------------------------------------
    if antenna == "up":
        d.line([11, 0, 11, 3 + hy], fill=STEEL_D); d.point([(11, 0), (12, 0)], fill=RED)
    elif antenna == "sway":
        d.line([11, 3 + hy, 13, 1], fill=STEEL_D); d.point([(13, 0), (14, 0)], fill=RED)
    elif antenna == "droop":
        d.line([11, 3 + hy, 14, 2 + hy], fill=STEEL_D)
        d.point([(15, 3 + hy)], fill=RED)

    # --- extras ---------------------------------------------------------------
    if "lines0" in extras:
        d.point([(1, 8), (2, 8), (1, 16), (2, 16)], fill=STEEL_D)
    if "lines1" in extras:
        d.point([(2, 12), (3, 12), (1, 20), (2, 20)], fill=STEEL_D)
    if "magnify0" in extras or "magnify1" in extras:
        dy = 1 if "magnify1" in extras else 0
        d.ellipse([13, 5 + hy + dy, 18, 10 + hy + dy], outline=BOLT)
        d.line([18, 10 + hy + dy, 20, 13 + hy + dy], fill=BOLT)
    if "smoke0" in extras:
        d.point([(8, 1), (9, 1), (8, 0)], fill=SMOKE); d.point([(15, 2)], fill=SMOKE)
    if "smoke1" in extras:
        d.point([(9, 0), (10, 1)], fill=SMOKE); d.point([(15, 0), (16, 1)], fill=SMOKE)
    if any(e.startswith("blueprint") for e in extras):
        stage = int([e for e in extras if e.startswith("blueprint")][0][-1])
        d.rectangle([1, 15, 9, 21], fill=BLUE, outline=BP_LINE)
        d.line([3, 17, 7, 17], fill=BP_LINE)
        if stage >= 1: d.line([3, 19, 6, 19], fill=BP_LINE)
        if stage >= 2: d.point([(7, 19), (8, 18)], fill=BP_LINE)
        d.line([19, 14 + (1 if stage == 1 else 0), 21, 17 + (1 if stage == 1 else 0)], fill=BOLT)  # pencil
    if "coffee" in extras:
        d.rectangle([20, 20, 22, 23], fill=(235, 230, 225))
        d.line([20, 20, 22, 20], fill=COFFEE)
        d.point([(23, 21)], fill=(235, 230, 225))            # handle
    if "steam0" in extras:
        d.point([(21, 18), (20, 16)], fill=STEAM)
    if "steam1" in extras:
        d.point([(20, 17), (21, 15)], fill=STEAM)
    if "sweat0" in extras:
        d.point([(19, 5 + hy)], fill=SWEAT)
    if "sweat1" in extras:
        d.point([(20, 7 + hy)], fill=SWEAT); d.point([(19, 4 + hy)], fill=SWEAT)

    return img


FRAMES = [
    # idle 0-2
    frame(),
    frame(eyes="flicker", antenna="sway"),
    frame(eyes="blink"),
    # running 3-6
    frame(head_dy=0, arms="a", antenna="sway", extras=("lines0",)),
    frame(head_dy=1, arms="b", extras=("lines1",)),
    frame(head_dy=0, arms="b", antenna="sway", extras=("lines0",)),
    frame(head_dy=1, arms="a", extras=("lines1",)),
    # waiting 7-8
    frame(eyes="left", extras=("tap",)),
    frame(eyes="right"),
    # review 9-10
    frame(eyes="on", extras=("magnify0",)),
    frame(eyes="half", extras=("magnify1",)),
    # failed 11-12
    frame(eyes="x", antenna="droop", extras=("smoke0", "patch_ajar")),
    frame(eyes="x", antenna="droop", head_dy=1, extras=("smoke1", "patch_ajar")),
    # planning 13-15
    frame(eyes="down", extras=("blueprint0",)),
    frame(eyes="down", head_dy=1, extras=("blueprint1",)),
    frame(eyes="on", extras=("blueprint2",)),
    # tired-idle 16-17
    frame(eyes="half", antenna="droop", head_dy=1, arms="slump", extras=("coffee", "steam0")),
    frame(eyes="half", antenna="droop", head_dy=1, arms="slump", extras=("coffee", "steam1")),
    # tired-running 18-19
    frame(eyes="half", antenna="droop", head_dy=1, arms="a", extras=("sweat0", "lines0")),
    frame(eyes="half", antenna="droop", head_dy=1, arms="b", extras=("sweat1", "lines1")),
    # talking 20-21 (closed = frame 0)
    frame(mouth="half"),
    frame(mouth="open"),
]

# binarize alpha (contract: >=128 opaque) and assemble the strip
strip = Image.new("RGBA", (N * len(FRAMES), N), (0, 0, 0, 0))
for i, f in enumerate(FRAMES):
    a = f.split()[-1].point(lambda v: 255 if v >= 128 else 0)
    f.putalpha(a)
    strip.paste(f, (i * N, 0))
strip.save(OUT / "sheet.png")
print(f"strip: {strip.size}, frames: {len(FRAMES)}")
