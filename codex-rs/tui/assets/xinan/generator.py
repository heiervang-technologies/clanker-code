#!/usr/bin/env python3
"""Hand-rolled Xinan 24x24 avatar generator.

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


def xinan_frame(
    *,
    bob: int = 0,
    eyes: str = "calm",
    cape_flare: int = 0,
    core: str = "crimson",
    mouth: str = "closed",
    extras: tuple[str, ...] = (),
) -> Image.Image:
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    INK = (22, 18, 24)
    PORC_SHAD = (182, 186, 198)
    PORC = (238, 240, 246)
    PORC_HI = (255, 255, 255)
    CRIMSON_DARK = (128, 18, 28)
    CRIMSON = (212, 36, 48)
    CRIMSON_HI = (255, 96, 96)
    LIP_ROSE = (186, 92, 102)
    GOLD = (242, 196, 64)
    JADE = (72, 232, 184)
    COFFEE = (132, 84, 46)
    STEAM = (210, 215, 225)
    SWEAT = (110, 195, 245)

    def y(v: int) -> int:
        return v + bob

    # --- High-Shouldered White Mantle Cape with Deep Crimson-Red Inner Lining ---
    cf = cape_flare
    d.polygon([(2 - cf, 23), (4, y(13)), (19, y(13)), (21 + cf, 23)], fill=PORC)
    d.polygon([(3 - cf, 23), (5, y(14)), (8, y(15)), (6, 23)], fill=CRIMSON_DARK)
    d.polygon([(20 + cf, 23), (18, y(14)), (15, y(15)), (17, 23)], fill=CRIMSON)
    d.polygon([(7, 23), (8, y(15)), (15, y(15)), (16, 23)], fill=PORC)
    d.polygon([(3, y(14)), (8, y(13)), (8, y(16)), (4, y(16))], fill=PORC_HI, outline=CRIMSON)
    d.polygon([(20, y(14)), (15, y(13)), (15, y(16)), (19, y(16))], fill=PORC_HI, outline=CRIMSON)

    # --- Crimson & Gold Chest Core + Red Armor Seam Lines ---
    d.line([(11, y(15)), (11, 23)], fill=CRIMSON_DARK)
    d.line([(8, y(19)), (15, y(19))], fill=PORC_SHAD)
    if core == "crimson":
        d.rectangle([10, y(17), 13, y(19)], fill=CRIMSON)
        d.point([(11, y(18)), (12, y(18))], fill=GOLD)
    elif core == "pulse":
        d.rectangle([10, y(17), 13, y(19)], fill=CRIMSON_HI)
        d.rectangle([11, y(17), 12, y(19)], fill=GOLD)
    elif core == "dim":
        d.rectangle([10, y(17), 13, y(19)], fill=CRIMSON_DARK)

    # --- Dark Mechanical Neck & Smooth Porcelain Android Head ---
    d.rectangle([9, y(13), 14, y(15)], fill=INK)
    d.point([(11, y(14)), (12, y(14))], fill=CRIMSON)
    d.rounded_rectangle([6, y(2), 17, y(13)], radius=3, fill=PORC)
    d.line([(6, y(5)), (6, y(11))], fill=PORC_SHAD)
    d.line([(8, y(13)), (15, y(13))], fill=PORC_SHAD)
    d.polygon([(9, y(2)), (15, y(2)), (16, y(4)), (10, y(4))], fill=PORC_HI)
    d.point([(11, y(4)), (12, y(4))], fill=CRIMSON)

    # --- Iconic Circular Crimson Ear-Discs & Hanging Red Tassels ---
    for ex in (4, 18):
        d.rectangle([ex, y(7), ex + 1, y(9)], fill=CRIMSON)
        d.point((ex, y(7)), fill=GOLD)
        d.line([(ex + (1 if ex < 12 else 0), y(10)), (ex + (1 if ex < 12 else 0), y(12))], fill=CRIMSON_HI)

    # --- Serene Eyes in Dark Porcelain Sockets (y=7..8) ---
    ey = y(7)
    d.line([(8, ey - 1), (10, ey - 1)], fill=INK)
    d.line([(13, ey - 1), (15, ey - 1)], fill=INK)
    if eyes == "calm":
        for ex in (8, 13):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=INK)
            d.point((ex, ey), fill=PORC_HI)
            d.point((ex + 1, ey + 1), fill=CRIMSON_HI)
    elif eyes == "glem":
        for ex in (8, 13):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=JADE)
            d.point((ex, ey), fill=PORC_HI)
    elif eyes == "blink":
        d.line([(8, ey + 1), (9, ey + 1)], fill=INK)
        d.line([(13, ey + 1), (14, ey + 1)], fill=INK)
    elif eyes == "left":
        for ex in (7, 12):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=INK)
            d.point((ex, ey), fill=PORC_HI)
    elif eyes == "right":
        for ex in (9, 14):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=INK)
            d.point((ex + 1, ey), fill=PORC_HI)
    elif eyes == "half":
        d.line([(8, ey), (9, ey)], fill=PORC_SHAD)
        d.line([(13, ey), (14, ey)], fill=PORC_SHAD)
        d.line([(8, ey + 1), (9, ey + 1)], fill=INK)
        d.line([(13, ey + 1), (14, ey + 1)], fill=INK)
    elif eyes == "x":
        for ex in (8, 13):
            d.point([(ex, ey), (ex + 1, ey + 1), (ex + 1, ey), (ex, ey + 1)], fill=CRIMSON_HI)

    # --- Nose tip at y=9 + clean 1px gap at y=10 + Serene 2px lip at y=11 ---
    d.point((11, y(9)), fill=PORC_SHAD)
    if mouth == "closed":
        d.line([(11, y(11)), (12, y(11))], fill=LIP_ROSE)
    elif mouth == "half":
        d.rectangle([10, y(11), 13, y(12)], fill=INK)
        d.line([(11, y(11)), (12, y(11))], fill=PORC_HI)
    elif mouth == "open":
        d.rectangle([9, y(11), 14, y(12)], fill=INK)
        d.line([(10, y(11)), (13, y(11))], fill=PORC_HI)
        d.line([(11, y(12)), (12, y(12))], fill=CRIMSON_HI)

    # --- State extras ---
    if "wind1" in extras:
        d.point([(1, y(4)), (2, y(5)), (21, y(3)), (22, y(4))], fill=JADE)
    if "wind2" in extras:
        d.point([(2, y(3)), (3, y(4)), (20, y(5)), (21, y(6))], fill=GOLD)
    if "wait1" in extras:
        d.point([(21, y(8)), (21, y(10))], fill=CRIMSON_HI)
    if "wait2" in extras:
        d.point([(21, y(6)), (21, y(8)), (21, y(10))], fill=GOLD)
    if "reticle" in extras:
        d.rectangle([12, y(6), 17, y(10)], outline=JADE)
    if "holo0" in extras or "holo1" in extras or "holo2" in extras:
        stage = int(next(e[-1] for e in extras if e.startswith("holo")))
        d.rectangle([0, y(10), 4, y(17)], fill=INK, outline=JADE)
        d.line([(1, y(12)), (3, y(12))], fill=JADE)
        if stage >= 1:
            d.line([(1, y(14)), (3, y(14))], fill=GOLD)
        if stage >= 2:
            d.point([(1, y(16)), (3, y(16)), (2, y(8))], fill=JADE)
    if "coffee" in extras:
        d.rectangle([19, 19, 22, 22], fill=COFFEE)
        d.line([(19, 19), (22, 19)], fill=STEAM)
        if "steam1" in extras:
            d.point([(20, 17), (21, 15)], fill=STEAM)
        else:
            d.point([(21, 17), (20, 15)], fill=STEAM)
    if "sweat" in extras:
        d.point([(20, y(5)), (20, y(6))], fill=SWEAT)

    return finalize_alpha(img)


def build_xinan() -> list[Image.Image]:
    return [
        xinan_frame(),
        xinan_frame(eyes="glem", core="pulse", extras=("wind1",)),
        xinan_frame(eyes="blink"),
        xinan_frame(cape_flare=1, eyes="glem", core="pulse", extras=("wind1",)),
        xinan_frame(bob=1, cape_flare=0, core="pulse", extras=("wind2",)),
        xinan_frame(cape_flare=1, eyes="glem", core="pulse", extras=("wind2",)),
        xinan_frame(bob=1, cape_flare=0, core="pulse", extras=("wind1",)),
        xinan_frame(eyes="left", extras=("wait1",)),
        xinan_frame(eyes="right", extras=("wait2",)),
        xinan_frame(eyes="glem", core="pulse", extras=("reticle",)),
        xinan_frame(eyes="calm", core="pulse", extras=("reticle", "wind1")),
        xinan_frame(eyes="x", core="dim"),
        xinan_frame(bob=1, eyes="x", core="dim"),
        xinan_frame(eyes="left", extras=("holo0",)),
        xinan_frame(eyes="glem", core="pulse", extras=("holo1",)),
        xinan_frame(eyes="glem", core="pulse", extras=("holo2", "wind1")),
        xinan_frame(bob=1, eyes="half", core="dim", extras=("coffee", "steam1")),
        xinan_frame(bob=1, eyes="blink", core="dim", extras=("coffee",)),
        xinan_frame(bob=1, eyes="half", core="dim", cape_flare=1, extras=("sweat",)),
        xinan_frame(bob=1, eyes="half", core="dim", cape_flare=0, extras=("sweat",)),
        xinan_frame(mouth="half"),
        xinan_frame(mouth="open"),
    ]

if __name__ == "__main__":
    frames = build_xinan()
    strip = Image.new("RGBA", (SIZE * len(frames), SIZE), (0, 0, 0, 0))
    for index, source in enumerate(frames):
        strip.paste(source, (index * SIZE, 0))
    strip.save(HERE / "sheet.png")
    print(f"xinan: strip={strip.size}, frames={len(frames)}")
