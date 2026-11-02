#!/usr/bin/env python3
"""Hand-rolled Claudian 24x24 avatar generator.

Every pixel is placed with hard-edged PIL ImageDraw primitives.
Binary alpha (0 or 255). Talking frames (20, 21) modify ONLY mouth rows relative to frame 0.
"""

from pathlib import Path
from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
SIZE = 24


def mp(d: ImageDraw.ImageDraw, pts, fill):
    """Plot symmetric points mirrored across x=11.5 (23 - x)."""
    for x, y in pts:
        d.point([(x, y), (23 - x, y)], fill=fill)


def finalize_alpha(img: Image.Image) -> Image.Image:
    alpha = img.getchannel("A").point(lambda v: 255 if v >= 128 else 0)
    img.putalpha(alpha)
    return img


def claudian_frame(
    *,
    bob: int = 0,
    eyes: str = "stoic",
    toga_sway: int = 0,
    halo: str = "none",
    mouth: str = "closed",
    extras: tuple[str, ...] = (),
) -> Image.Image:
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    INK = (28, 18, 14)
    TERRA_DARK = (158, 72, 28)
    TERRA = (214, 112, 46)
    TERRA_HI = (242, 154, 82)
    TOGA_SHAD = (198, 186, 166)
    TOGA = (238, 230, 214)
    TOGA_HI = (252, 248, 240)
    GOLD_DARK = (176, 124, 28)
    GOLD = (242, 192, 56)
    GOLD_HI = (255, 236, 140)
    EYE_WHITE = (252, 246, 232)
    RED = (238, 64, 58)
    COFFEE = (132, 84, 46)
    STEAM = (210, 215, 225)
    SWEAT = (110, 195, 245)

    def y(v: int) -> int:
        return v + bob

    # --- Optional Anthara solar rays during active/pulse states ---
    if halo == "pulse":
        d.line([(3, y(4)), (4, y(5))], fill=GOLD_HI)
        d.line([(20, y(4)), (19, y(5))], fill=GOLD_HI)
        d.point([(11, y(0)), (12, y(0))], fill=GOLD)

    # --- Shoulders & Torso: Left bare terracotta muscle, Right draped in cream Roman toga ---
    d.polygon([(3, 23), (4, y(15)), (18, y(15)), (20, 23)], fill=TERRA)
    d.polygon([(3, 23), (4, y(15)), (8, y(15)), (7, 23)], fill=TERRA_DARK)
    d.point([(6, y(17)), (7, y(17)), (6, y(18))], fill=TERRA_HI)
    # Gold arm bracer on bare left arm
    d.rectangle([3, y(20), 5, y(22)], fill=GOLD)
    d.point((4, y(21)), fill=GOLD_HI)

    # Diagonal cream Roman toga slung from viewer-right shoulder across to lower-left
    d.polygon(
        [
            (7 + toga_sway, 23),
            (13, y(14)),
            (19, y(15)),
            (21 + toga_sway, 23),
        ],
        fill=TOGA,
    )
    d.line([(9 + toga_sway, 23), (14, y(15))], fill=TOGA_HI)
    d.line([(12 + toga_sway, 23), (16, y(16))], fill=TOGA_SHAD)
    d.line([(16 + toga_sway, 23), (18, y(17))], fill=TOGA_SHAD)
    # Gold waist belt peeking at bottom
    d.line([(8, 22), (18, 22)], fill=GOLD)
    d.point([(11, 22), (14, 22)], fill=GOLD_HI)

    # Round Gold Fibula Clasp on right shoulder (x=14..16, y=14..16)
    d.rectangle([14, y(14), 16, y(16)], fill=GOLD_DARK)
    d.rectangle([14, y(14), 15, y(15)], fill=GOLD)
    d.point((15, y(14)), fill=GOLD_HI)

    # --- Neck & 3/4 Classical Bald Terracotta Head ---
    d.rectangle([9, y(13), 14, y(15)], fill=TERRA_DARK)
    d.rectangle([17, y(8), 18, y(10)], fill=TERRA_DARK)
    d.rounded_rectangle([6, y(2), 17, y(13)], radius=3, fill=TERRA)
    d.line([(6, y(5)), (6, y(11))], fill=TERRA_DARK)
    d.line([(8, y(13)), (15, y(13))], fill=TERRA_DARK)
    d.polygon([(8, y(2)), (14, y(2)), (15, y(4)), (9, y(4))], fill=TERRA_HI)

    # --- Stoic Classical Eyes (y=7..8) ---
    ey = y(7)
    d.line([(7, ey - 1), (10, ey - 1)], fill=TERRA_DARK)
    d.line([(12, ey - 1), (15, ey - 1)], fill=TERRA_DARK)
    if eyes == "stoic":
        for ex in (8, 13):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=INK)
            d.point((ex, ey), fill=EYE_WHITE)
    elif eyes == "solar":
        for ex in (8, 13):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=GOLD)
            d.point((ex, ey), fill=GOLD_HI)
    elif eyes == "blink":
        d.line([(8, ey + 1), (9, ey + 1)], fill=INK)
        d.line([(13, ey + 1), (14, ey + 1)], fill=INK)
    elif eyes == "left":
        for ex in (7, 12):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=INK)
            d.point((ex, ey), fill=EYE_WHITE)
    elif eyes == "right":
        for ex in (9, 14):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=INK)
            d.point((ex + 1, ey), fill=EYE_WHITE)
    elif eyes == "half":
        d.line([(8, ey), (9, ey)], fill=TERRA_DARK)
        d.line([(13, ey), (14, ey)], fill=TERRA_DARK)
        d.line([(8, ey + 1), (9, ey + 1)], fill=INK)
        d.line([(13, ey + 1), (14, ey + 1)], fill=INK)
    elif eyes == "x":
        for ex in (8, 13):
            d.point([(ex, ey), (ex + 1, ey + 1), (ex + 1, ey), (ex, ey + 1)], fill=RED)

    # --- Nose tip at y=9 + clean 1px gap at y=10 + Slim 2px lip at y=11 ---
    d.point((10, y(9)), fill=TERRA_DARK)
    if mouth == "closed":
        d.line([(10, y(11)), (12, y(11))], fill=TERRA_DARK)
    elif mouth == "half":
        d.rectangle([10, y(11), 12, y(12)], fill=INK)
        d.point((11, y(11)), fill=TOGA_HI)
    elif mouth == "open":
        d.rectangle([9, y(11), 13, y(12)], fill=INK)
        d.line([(10, y(11)), (12, y(11))], fill=TOGA_HI)
        d.point((11, y(12)), fill=RED)

    # --- State extras ---
    if "wait1" in extras:
        d.point([(20, y(8)), (20, y(10))], fill=GOLD)
    if "wait2" in extras:
        d.point([(20, y(6)), (20, y(8)), (20, y(10))], fill=GOLD_HI)
    if "scale" in extras:
        d.rectangle([12, y(6), 16, y(9)], outline=GOLD_HI)
    if "refusal" in extras:
        d.rectangle([0, y(11), 4, y(15)], fill=INK, outline=RED)
        d.line([(1, y(12)), (3, y(14))], fill=RED)
        d.line([(1, y(14)), (3, y(12))], fill=RED)
    if "holo0" in extras or "holo1" in extras or "holo2" in extras:
        stage = int(next(e[-1] for e in extras if e.startswith("holo")))
        d.rectangle([0, y(11), 4, y(18)], fill=TOGA, outline=GOLD_DARK)
        d.line([(1, y(13)), (3, y(13))], fill=TERRA_DARK)
        if stage >= 1:
            d.line([(1, y(15)), (3, y(15))], fill=TERRA_DARK)
        if stage >= 2:
            d.line([(1, y(17)), (2, y(17))], fill=GOLD_DARK)
    if "coffee" in extras:
        d.rectangle([19, 19, 22, 22], fill=COFFEE)
        d.line([(19, 19), (22, 19)], fill=STEAM)
        if "steam1" in extras:
            d.point([(20, 17), (21, 15)], fill=STEAM)
        else:
            d.point([(21, 17), (20, 15)], fill=STEAM)
    if "sweat" in extras:
        d.point([(19, y(5)), (19, y(6))], fill=SWEAT)

    return finalize_alpha(img)


def build_claudian() -> list[Image.Image]:
    return [
        claudian_frame(),
        claudian_frame(eyes="solar", halo="pulse"),
        claudian_frame(eyes="blink"),
        claudian_frame(toga_sway=-1, eyes="solar", halo="pulse"),
        claudian_frame(bob=1, toga_sway=0),
        claudian_frame(toga_sway=1, eyes="solar", halo="pulse"),
        claudian_frame(bob=1, toga_sway=0),
        claudian_frame(eyes="left", extras=("wait1",)),
        claudian_frame(eyes="right", extras=("wait2",)),
        claudian_frame(eyes="solar", extras=("scale",)),
        claudian_frame(eyes="stoic", halo="pulse", extras=("scale",)),
        claudian_frame(eyes="x", extras=("refusal",)),
        claudian_frame(bob=1, eyes="x", extras=("refusal",)),
        claudian_frame(eyes="left", extras=("holo0",)),
        claudian_frame(eyes="left", extras=("holo1",)),
        claudian_frame(eyes="solar", extras=("holo2",)),
        claudian_frame(bob=1, eyes="half", extras=("coffee", "steam1")),
        claudian_frame(bob=1, eyes="blink", extras=("coffee",)),
        claudian_frame(bob=1, eyes="half", toga_sway=-1, extras=("sweat",)),
        claudian_frame(bob=1, eyes="half", toga_sway=1, extras=("sweat",)),
        claudian_frame(mouth="half"),
        claudian_frame(mouth="open"),
    ]

if __name__ == "__main__":
    frames = build_claudian()
    strip = Image.new("RGBA", (SIZE * len(frames), SIZE), (0, 0, 0, 0))
    for index, source in enumerate(frames):
        strip.paste(source, (index * SIZE, 0))
    strip.save(HERE / "sheet.png")
    print(f"claudian: strip={strip.size}, frames={len(frames)}")
