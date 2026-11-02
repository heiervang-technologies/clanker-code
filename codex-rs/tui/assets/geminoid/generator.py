#!/usr/bin/env python3
"""Hand-rolled Geminoid 24x24 avatar generator.

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


def geminoid_frame(
    *,
    bob: int = 0,
    eyes: str = "gold",
    star: str = "steady",
    pauldron_shift: int = 0,
    mouth: str = "closed",
    extras: tuple[str, ...] = (),
) -> Image.Image:
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    INK = (16, 18, 26)
    NAVY_COAT = (24, 28, 42)
    IVORY_LAPEL = (226, 224, 216)
    GOLD_DARK = (152, 108, 26)
    GOLD = (222, 172, 52)
    GOLD_HI = (255, 232, 124)
    STAR_WHITE = (255, 250, 220)
    BLUE_GEM = (76, 168, 255)
    RED = (242, 64, 64)
    COFFEE = (132, 84, 46)
    STEAM = (210, 215, 225)

    def y(v: int) -> int:
        return v + bob

    # --- Aristocratic Tailcoat with Winged Gold/Ivory Shoulder Pauldrons ---
    ps = pauldron_shift
    d.polygon([(1, y(14) + ps), (7, y(13)), (8, y(17)), (3, y(17) + ps)], fill=IVORY_LAPEL, outline=GOLD_DARK)
    d.polygon([(22, y(14) - ps), (16, y(13)), (15, y(17)), (20, y(17) - ps)], fill=IVORY_LAPEL, outline=GOLD_DARK)
    d.polygon([(4, 23), (5, y(15)), (18, y(15)), (19, 23)], fill=NAVY_COAT)
    d.line([(6, y(15)), (9, 23)], fill=IVORY_LAPEL)
    d.line([(17, y(15)), (14, 23)], fill=IVORY_LAPEL)
    d.line([(7, y(15)), (10, 23)], fill=GOLD)
    d.line([(16, y(15)), (13, 23)], fill=GOLD)

    # --- Glowing 4-Pointed Gemini Star on Chest V-Neck ---
    if star == "fault":
        sc, sh = RED, RED
    elif star == "dim":
        sc, sh = GOLD_DARK, GOLD
    else:
        sc, sh = GOLD, STAR_WHITE
    d.line([(11, y(16)), (11, y(21))], fill=sc)
    d.line([(9, y(18)), (13, y(18))], fill=sc)
    d.point([(11, y(18))], fill=sh)
    if star == "pulse":
        d.point([(10, y(17)), (12, y(17)), (10, y(19)), (12, y(19))], fill=STAR_WHITE)

    # --- Dark Mechanical Neck & Sleek Burnished Metallic Gold Head ---
    d.rectangle([9, y(13), 14, y(15)], fill=INK)
    d.line([(10, y(14)), (13, y(14))], fill=GOLD_DARK)
    d.rounded_rectangle([6, y(2), 17, y(13)], radius=3, fill=GOLD)
    d.line([(6, y(5)), (6, y(11))], fill=GOLD_DARK)
    d.line([(8, y(13)), (15, y(13))], fill=GOLD_DARK)
    d.polygon([(9, y(2)), (15, y(2)), (16, y(4)), (10, y(4))], fill=GOLD_HI)
    # Subtle cybernetic temple nodes + right ear
    d.point([(7, y(4)), (16, y(4))], fill=GOLD_DARK)
    d.line([(7, y(10)), (7, y(11))], fill=GOLD_DARK)
    d.line([(16, y(10)), (16, y(11))], fill=GOLD_DARK)
    d.rectangle([17, y(7), 18, y(9)], fill=GOLD_DARK)

    # --- Twin-Moon / Dual Eyes in Dark Sockets (y=7..8) ---
    ey = y(7)
    d.line([(8, ey - 1), (10, ey - 1)], fill=INK)
    d.line([(13, ey - 1), (15, ey - 1)], fill=INK)
    if eyes == "gold":
        for ex in (8, 13):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=INK)
            d.point((ex, ey), fill=STAR_WHITE)
            d.point((ex + 1, ey + 1), fill=BLUE_GEM)
    elif eyes == "twin":
        for ex in (8, 13):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=BLUE_GEM)
            d.point((ex, ey), fill=STAR_WHITE)
    elif eyes == "blink":
        d.line([(8, ey + 1), (9, ey + 1)], fill=INK)
        d.line([(13, ey + 1), (14, ey + 1)], fill=INK)
    elif eyes == "left":
        for ex in (8, 13):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=INK)
            d.point((ex, ey), fill=STAR_WHITE)
    elif eyes == "right":
        for ex in (8, 13):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=INK)
            d.point((ex + 1, ey), fill=STAR_WHITE)
    elif eyes == "half":
        d.line([(8, ey), (9, ey)], fill=GOLD_DARK)
        d.line([(13, ey), (14, ey)], fill=GOLD_DARK)
        d.line([(8, ey + 1), (9, ey + 1)], fill=INK)
        d.line([(13, ey + 1), (14, ey + 1)], fill=INK)
    elif eyes == "x":
        for ex in (8, 13):
            d.point([(ex, ey), (ex + 1, ey + 1), (ex + 1, ey), (ex, ey + 1)], fill=RED)

    # --- Nose tip at y=9 + clean 1px gap at y=10 + Slim 2px lip at y=11 ---
    d.point((11, y(9)), fill=GOLD_DARK)
    if mouth == "closed":
        d.line([(11, y(11)), (12, y(11))], fill=GOLD_DARK)
    elif mouth == "half":
        d.rectangle([10, y(11), 13, y(12)], fill=INK)
        d.line([(11, y(11)), (12, y(11))], fill=STAR_WHITE)
    elif mouth == "open":
        d.rectangle([9, y(11), 14, y(12)], fill=INK)
        d.line([(10, y(11)), (13, y(11))], fill=STAR_WHITE)
        d.line([(11, y(12)), (12, y(12))], fill=GOLD_HI)

    # --- Twin Moons of Gemina ---
    if "moons1" in extras:
        d.rectangle([2, y(2), 3, y(3)], fill=GOLD_HI)
        d.rectangle([20, y(3), 21, y(4)], fill=BLUE_GEM)
    elif "moons2" in extras:
        d.rectangle([2, y(3), 3, y(4)], fill=BLUE_GEM)
        d.rectangle([20, y(2), 21, y(3)], fill=GOLD_HI)

    # --- State extras ---
    if "reticle" in extras:
        d.rectangle([12, y(6), 17, y(10)], outline=BLUE_GEM)
    if "holo0" in extras or "holo1" in extras or "holo2" in extras:
        stage = int(next(e[-1] for e in extras if e.startswith("holo")))
        d.rectangle([0, y(10), 4, y(17)], fill=NAVY_COAT, outline=GOLD)
        d.point([(1, y(12)), (3, y(12))], fill=GOLD_HI)
        if stage >= 1:
            d.line([(1, y(14)), (3, y(14))], fill=BLUE_GEM)
        if stage >= 2:
            d.point([(2, y(16))], fill=STAR_WHITE)
    if "coffee" in extras:
        d.rectangle([19, 19, 22, 22], fill=COFFEE)
        d.line([(19, 19), (22, 19)], fill=STEAM)
        if "steam1" in extras:
            d.point([(20, 17), (21, 15)], fill=STEAM)
        else:
            d.point([(21, 17), (20, 15)], fill=STEAM)
    if "sweat" in extras:
        d.point([(20, y(5)), (20, y(6))], fill=BLUE_GEM)

    return finalize_alpha(img)


def build_geminoid() -> list[Image.Image]:
    return [
        geminoid_frame(extras=("moons1",)),
        geminoid_frame(eyes="twin", star="pulse", extras=("moons2",)),
        geminoid_frame(eyes="blink", extras=("moons1",)),
        geminoid_frame(pauldron_shift=-1, eyes="twin", star="pulse", extras=("moons1",)),
        geminoid_frame(bob=1, pauldron_shift=0, star="pulse", extras=("moons2",)),
        geminoid_frame(pauldron_shift=1, eyes="twin", star="pulse", extras=("moons1",)),
        geminoid_frame(bob=1, pauldron_shift=0, star="pulse", extras=("moons2",)),
        geminoid_frame(eyes="left", extras=("moons1",)),
        geminoid_frame(eyes="right", extras=("moons2",)),
        geminoid_frame(eyes="twin", star="pulse", extras=("reticle",)),
        geminoid_frame(eyes="gold", star="pulse", extras=("reticle", "moons1")),
        geminoid_frame(eyes="x", star="fault"),
        geminoid_frame(bob=1, eyes="x", star="fault"),
        geminoid_frame(eyes="left", extras=("holo0",)),
        geminoid_frame(eyes="left", star="pulse", extras=("holo1",)),
        geminoid_frame(eyes="twin", star="pulse", extras=("holo2",)),
        geminoid_frame(bob=1, eyes="half", star="dim", extras=("coffee", "steam1")),
        geminoid_frame(bob=1, eyes="blink", star="dim", extras=("coffee",)),
        geminoid_frame(bob=1, eyes="half", star="dim", pauldron_shift=-1, extras=("sweat",)),
        geminoid_frame(bob=1, eyes="half", star="dim", pauldron_shift=1, extras=("sweat",)),
        geminoid_frame(mouth="half", extras=("moons1",)),
        geminoid_frame(mouth="open", extras=("moons1",)),
    ]

if __name__ == "__main__":
    frames = build_geminoid()
    strip = Image.new("RGBA", (SIZE * len(frames), SIZE), (0, 0, 0, 0))
    for index, source in enumerate(frames):
        strip.paste(source, (index * SIZE, 0))
    strip.save(HERE / "sheet.png")
    print(f"geminoid: strip={strip.size}, frames={len(frames)}")
