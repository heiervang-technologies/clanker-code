#!/usr/bin/env python3
"""Hand-rolled Lawrence 24x24 avatar generator.

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


def lawrence_frame(
    *,
    bob: int = 0,
    shades: str = "gold",
    stache: str = "up",
    coat_sway: int = 0,
    mouth: str = "closed",
    extras: tuple[str, ...] = (),
) -> Image.Image:
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    INK = (20, 12, 28)
    PURP_DARK = (68, 22, 108)
    PURP = (128, 46, 186)
    PURP_HI = (182, 92, 242)
    WHITE = (246, 242, 252)
    GOLD_DARK = (178, 126, 24)
    GOLD = (244, 196, 52)
    GOLD_HI = (255, 240, 148)
    SKIN = (232, 186, 148)
    SKIN_SHAD = (194, 146, 110)
    STACHE = (42, 22, 48)
    STACHE_HI = (108, 56, 132)
    RED = (242, 62, 62)
    COFFEE = (132, 84, 46)
    STEAM = (210, 215, 225)
    SWEAT = (110, 195, 245)

    def y(v: int) -> int:
        return v + bob

    # --- Purple Leopard-Spot Suit & Tie ---
    d.polygon([(3 + coat_sway, 23), (5, y(16)), (18, y(16)), (20 + coat_sway, 23)], fill=PURP)
    d.rectangle([5, y(17), 18, 23], fill=PURP)
    mp(d, [(6, y(18)), (8, y(20)), (6, y(21)), (9, y(22))], PURP_DARK)
    mp(d, [(7, y(18)), (8, y(21))], INK)
    d.polygon([(9, y(16)), (14, y(16)), (12, y(22)), (11, y(22))], fill=INK)
    d.line([(11, y(16)), (11, y(20))], fill=PURP_HI)
    d.point((11, y(21)), fill=GOLD)

    # --- Pointed Ears & Face ---
    mp(d, [(4, y(8)), (5, y(8)), (5, y(9)), (6, y(9))], SKIN)
    mp(d, [(4, y(9))], SKIN_SHAD)
    d.rounded_rectangle([7, y(6), 16, y(15)], radius=2, fill=SKIN)
    d.line([(9, y(15)), (14, y(15))], fill=SKIN_SHAD)

    # --- Gold-Tinted Angular Sunglasses (y=7..9) ---
    d.rectangle([6, y(7), 17, y(9)], fill=INK)
    if shades == "gold":
        mp(d, [(7, y(8)), (8, y(8)), (9, y(8)), (10, y(8)), (8, y(9)), (9, y(9))], GOLD)
        mp(d, [(7, y(8)), (8, y(8))], GOLD_HI)
    elif shades == "sparkle":
        mp(d, [(7, y(8)), (8, y(8)), (9, y(8)), (10, y(8)), (8, y(9)), (9, y(9))], GOLD_HI)
    elif shades == "blink":
        mp(d, [(7, y(9)), (8, y(9)), (9, y(9)), (10, y(9))], GOLD_DARK)
    elif shades == "left":
        for lx in (7, 13):
            d.rectangle([lx, y(8), lx + 2, y(9)], fill=GOLD)
            d.point((lx, y(8)), fill=GOLD_HI)
    elif shades == "right":
        for lx in (8, 14):
            d.rectangle([lx, y(8), lx + 2, y(9)], fill=GOLD)
            d.point((lx + 2, y(8)), fill=GOLD_HI)
    elif shades == "fault":
        for lx in (8, 14):
            d.point([(lx, y(8)), (lx + 1, y(9)), (lx + 1, y(8)), (lx, y(9))], fill=RED)
    elif shades == "dim":
        mp(d, [(8, y(9)), (9, y(9))], GOLD_DARK)

    # --- Pointed Nose (y=10) + 1px gap at y=11 ---
    d.point([(11, y(10)), (12, y(10))], fill=SKIN_SHAD)

    # --- Iconic Upturned Waluigi Mustache (y=12, tips flaring out with plum highlight) ---
    mp(d, [(8, y(12)), (9, y(12)), (10, y(12)), (11, y(12))], STACHE)
    if stache == "up":
        mp(d, [(6, y(11)), (7, y(11)), (5, y(10))], STACHE)
        mp(d, [(4, y(10)), (5, y(11))], STACHE_HI)
    elif stache == "droop":
        mp(d, [(6, y(13)), (7, y(12))], STACHE)
        mp(d, [(5, y(13))], STACHE_HI)

    # --- Mouth under mustache (y=13..14) — talking modifies ONLY this region! ---
    if mouth == "closed":
        d.line([(10, y(13)), (13, y(13))], fill=SKIN_SHAD)
    elif mouth == "half":
        d.rectangle([10, y(13), 13, y(14)], fill=INK)
        d.line([(11, y(13)), (12, y(13))], fill=WHITE)
    elif mouth == "open":
        d.rectangle([9, y(13), 14, y(14)], fill=INK)
        d.line([(10, y(13)), (13, y(13))], fill=WHITE)
        d.line([(11, y(14)), (12, y(14))], fill=RED)

    # --- Striped Purple-and-White Fedora with Crisp 'L' Crest (y=0..6) ---
    d.polygon([(6, y(5)), (7, y(1)), (16, y(1)), (17, y(5))], fill=PURP)
    d.line([(7, y(3)), (9, y(1))], fill=WHITE)
    d.line([(14, y(4)), (16, y(2))], fill=WHITE)
    d.line([(6, y(4)), (17, y(4))], fill=GOLD)
    d.polygon([(2, y(5)), (6, y(4)), (17, y(4)), (21, y(5)), (19, y(6)), (4, y(6))], fill=PURP_DARK)
    mp(d, [(4, y(5)), (7, y(5))], WHITE)
    # Dark purple badge box with clear gold 'L' letter inside
    d.rectangle([9, y(0), 14, y(4)], fill=INK)
    d.rectangle([10, y(0), 13, y(3)], fill=PURP_DARK)
    d.point([(11, y(1)), (11, y(2)), (11, y(3)), (12, y(3))], fill=GOLD_HI)

    # --- State extras ---
    if "spark" in extras:
        d.point([(2, y(2)), (21, y(3)), (20, y(10))], fill=GOLD_HI)
    if "wait1" in extras:
        d.point([(21, y(8)), (21, y(10))], fill=PURP_HI)
    if "wait2" in extras:
        d.point([(21, y(8)), (21, y(10)), (21, y(12))], fill=GOLD_HI)
    if "monocle" in extras:
        d.rectangle([13, y(6), 18, y(10)], outline=GOLD_HI)
    if "holo0" in extras or "holo1" in extras or "holo2" in extras:
        stage = int(next(e[-1] for e in extras if e.startswith("holo")))
        d.rectangle([0, y(11), 4, y(18)], fill=PURP_DARK, outline=GOLD)
        d.line([(1, y(13)), (3, y(13))], fill=GOLD_HI)
        if stage >= 1:
            d.line([(1, y(15)), (3, y(15))], fill=WHITE)
        if stage >= 2:
            d.point([(2, y(17))], fill=GOLD)
    if "coffee" in extras:
        d.rectangle([19, 19, 22, 22], fill=COFFEE)
        d.line([(19, 19), (22, 19)], fill=STEAM)
        if "steam1" in extras:
            d.point([(20, 17), (21, 15)], fill=STEAM)
        else:
            d.point([(21, 17), (20, 15)], fill=STEAM)
    if "sweat" in extras:
        d.point([(21, y(7)), (21, y(8))], fill=SWEAT)

    return finalize_alpha(img)


def build_lawrence() -> list[Image.Image]:
    return [
        lawrence_frame(),
        lawrence_frame(shades="sparkle", extras=("spark",)),
        lawrence_frame(shades="blink"),
        lawrence_frame(coat_sway=-1, shades="sparkle", extras=("spark",)),
        lawrence_frame(bob=1, coat_sway=0),
        lawrence_frame(coat_sway=1, shades="sparkle", extras=("spark",)),
        lawrence_frame(bob=1, coat_sway=0),
        lawrence_frame(shades="left", extras=("wait1",)),
        lawrence_frame(shades="right", extras=("wait2",)),
        lawrence_frame(shades="sparkle", extras=("monocle",)),
        lawrence_frame(shades="gold", extras=("monocle", "spark")),
        lawrence_frame(shades="fault", stache="droop"),
        lawrence_frame(bob=1, shades="fault", stache="droop"),
        lawrence_frame(shades="left", extras=("holo0",)),
        lawrence_frame(shades="left", extras=("holo1",)),
        lawrence_frame(shades="sparkle", extras=("holo2",)),
        lawrence_frame(bob=1, shades="dim", stache="droop", extras=("coffee", "steam1")),
        lawrence_frame(bob=1, shades="blink", stache="droop", extras=("coffee",)),
        lawrence_frame(bob=1, shades="dim", coat_sway=-1, extras=("sweat",)),
        lawrence_frame(bob=1, shades="dim", coat_sway=1, extras=("sweat",)),
        lawrence_frame(mouth="half"),
        lawrence_frame(mouth="open"),
    ]

if __name__ == "__main__":
    frames = build_lawrence()
    strip = Image.new("RGBA", (SIZE * len(frames), SIZE), (0, 0, 0, 0))
    for index, source in enumerate(frames):
        strip.paste(source, (index * SIZE, 0))
    strip.save(HERE / "sheet.png")
    print(f"lawrence: strip={strip.size}, frames={len(frames)}")
