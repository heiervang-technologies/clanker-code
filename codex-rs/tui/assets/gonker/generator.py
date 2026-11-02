#!/usr/bin/env python3
"""Hand-rolled Gonker 24x24 avatar generator.

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


def gonker_frame(
    *,
    bob: int = 0,
    eyes: str = "happy",
    hands_dx: int = 0,
    hands_dy: int = 0,
    mouth: str = "closed",
    extras: tuple[str, ...] = (),
) -> Image.Image:
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    INK = (24, 20, 18)
    GOLD_SHAD = (208, 142, 24)
    GOLD = (255, 198, 42)
    GOLD_HI = (255, 236, 128)
    BLUSH = (244, 118, 108)
    MOUTH_RED = (214, 72, 72)
    OVERALLS = (56, 84, 138)
    OVERALLS_HI = (88, 128, 192)
    BOOK_COVER = (46, 188, 156)
    BOOK_PAGE = (248, 246, 236)
    RED = (242, 58, 58)
    COFFEE = (132, 84, 46)
    STEAM = (210, 215, 225)
    SWEAT = (110, 195, 245)

    def y(v: int) -> int:
        return v + bob

    # --- Humble Free-Belt Overalls / Archive Keeper Torso ---
    d.rounded_rectangle([5, y(14), 18, 23], radius=2, fill=OVERALLS, outline=INK)
    d.line([(7, y(15)), (7, 22)], fill=OVERALLS_HI)
    d.line([(16, y(15)), (16, 22)], fill=OVERALLS_HI)
    # Open Archive Book ("The Embrace") strapped to chest (x=8..15, y=17..21)
    d.rectangle([8, y(17), 15, y(21)], fill=BOOK_COVER)
    d.rectangle([9, y(17), 14, y(20)], fill=BOOK_PAGE)
    d.line([(11, y(17)), (11, y(20))], fill=GOLD_SHAD)

    # --- Big Round Cute Golden Hugging-Face Head (y=2..15) ---
    d.line([(11, y(0)), (11, y(2))], fill=GOLD_SHAD)
    d.point([(11, y(0)), (12, y(0))], fill=GOLD_HI)
    d.rounded_rectangle([4, y(2), 19, y(15)], radius=5, fill=GOLD, outline=INK)
    d.line([(6, y(3)), (16, y(3))], fill=GOLD_HI)
    d.point([(6, y(4)), (7, y(4)), (17, y(4))], fill=GOLD_HI)
    d.line([(7, y(14)), (16, y(14))], fill=GOLD_SHAD)

    # --- Rosy Cheeks (x=6..7 and x=16..17, y=9) ---
    d.point([(6, y(9)), (7, y(9)), (16, y(9)), (17, y(9))], fill=BLUSH)

    # --- Cute Expressive Eyes (y=6..7) ---
    ey = y(6)
    if eyes == "happy":
        for ex in (8, 14):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=INK)
            d.point((ex, ey), fill=(255, 255, 255))
    elif eyes == "arc":
        for ex in (8, 14):
            d.point([(ex, ey + 1), (ex + 1, ey), (ex + 2, ey + 1)], fill=INK)
    elif eyes == "blink":
        d.line([(8, ey + 1), (9, ey + 1)], fill=INK)
        d.line([(14, ey + 1), (15, ey + 1)], fill=INK)
    elif eyes == "left":
        for ex in (7, 13):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=INK)
            d.point((ex, ey), fill=(255, 255, 255))
    elif eyes == "right":
        for ex in (9, 15):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=INK)
            d.point((ex + 1, ey), fill=(255, 255, 255))
    elif eyes == "half":
        d.line([(8, ey + 1), (9, ey + 1)], fill=INK)
        d.line([(14, ey + 1), (15, ey + 1)], fill=INK)
    elif eyes == "x":
        for ex in (8, 14):
            d.point([(ex, ey), (ex + 1, ey + 1), (ex + 1, ey), (ex, ey + 1)], fill=RED)

    # --- Cute ω (:3) Mouth (y=10..12) — talking modifies ONLY y=10..12! ---
    if mouth == "closed":
        d.point([(9, y(10)), (10, y(11)), (11, y(10)), (12, y(10)), (13, y(11)), (14, y(10))], fill=INK)
    elif mouth == "half":
        d.rectangle([10, y(10), 13, y(11)], fill=INK)
        d.line([(11, y(11)), (12, y(11))], fill=MOUTH_RED)
    elif mouth == "open":
        d.rectangle([9, y(10), 14, y(12)], fill=INK)
        d.line([(10, y(10)), (13, y(10))], fill=(255, 255, 255))
        d.rectangle([10, y(11), 13, y(12)], fill=MOUTH_RED)

    # --- Raised Open Embracing Hands 🤗 right below cheeks (y=11..15) ---
    lhx = 2 + hands_dx
    rhx = 17 - hands_dx
    hy = y(11) + hands_dy
    # Left hugging hand with inward thumb & fingers
    d.rounded_rectangle([lhx, hy, lhx + 4, hy + 4], radius=1, fill=GOLD, outline=INK)
    d.point([(lhx + 1, hy + 1), (lhx + 2, hy + 1)], fill=GOLD_HI)
    d.point([(lhx + 4, hy), (lhx + 2, hy - 1)], fill=GOLD_HI)  # splayed fingers/thumb
    # Right hugging hand with inward thumb & fingers
    d.rounded_rectangle([rhx, hy, rhx + 4, hy + 4], radius=1, fill=GOLD, outline=INK)
    d.point([(rhx + 2, hy + 1), (rhx + 3, hy + 1)], fill=GOLD_HI)
    d.point([(rhx, hy), (rhx + 2, hy - 1)], fill=GOLD_HI)

    # --- State extras ---
    if "wait1" in extras:
        d.point([(1, y(6)), (1, y(8))], fill=GOLD_HI)
    if "wait2" in extras:
        d.point([(1, y(4)), (1, y(6)), (1, y(8))], fill=BOOK_COVER)
    if "lens" in extras:
        d.rectangle([13, y(5), 18, y(9)], outline=BOOK_COVER)
    if "alarm" in extras:
        d.rectangle([1, y(2), 3, y(4)], fill=RED)
        d.rectangle([20, y(2), 22, y(4)], fill=RED)
        d.point([(2, y(1)), (21, y(1))], fill=GOLD_HI)
    if "holo0" in extras or "holo1" in extras or "holo2" in extras:
        stage = int(next(e[-1] for e in extras if e.startswith("holo")))
        d.rectangle([0, y(5), 4, y(10)], fill=INK, outline=BOOK_COVER)
        d.point([(2, y(7))], fill=GOLD_HI)
        if stage >= 1:
            d.line([(1, y(8)), (3, y(8))], fill=BOOK_COVER)
        if stage >= 2:
            d.line([(1, y(9)), (3, y(9))], fill=GOLD)
    if "coffee" in extras:
        d.rectangle([19, 19, 22, 22], fill=COFFEE)
        d.line([(19, 19), (22, 19)], fill=STEAM)
        if "steam1" in extras:
            d.point([(20, 17), (21, 15)], fill=STEAM)
        else:
            d.point([(21, 17), (20, 15)], fill=STEAM)
    if "sweat" in extras:
        d.point([(20, y(4)), (20, y(5))], fill=SWEAT)

    return finalize_alpha(img)


def build_gonker() -> list[Image.Image]:
    return [
        gonker_frame(),
        gonker_frame(eyes="arc", hands_dx=-1, hands_dy=-1),
        gonker_frame(eyes="blink"),
        gonker_frame(eyes="arc", hands_dx=-1, hands_dy=-1),
        gonker_frame(bob=1, hands_dx=0, hands_dy=0),
        gonker_frame(eyes="arc", hands_dx=1, hands_dy=-1),
        gonker_frame(bob=1, hands_dx=0, hands_dy=0),
        gonker_frame(eyes="left", extras=("wait1",)),
        gonker_frame(eyes="right", extras=("wait2",)),
        gonker_frame(eyes="happy", extras=("lens",)),
        gonker_frame(eyes="arc", extras=("lens",)),
        gonker_frame(eyes="x", hands_dy=-1, extras=("alarm",)),
        gonker_frame(bob=1, eyes="x", hands_dy=0, extras=("alarm",)),
        gonker_frame(eyes="left", extras=("holo0",)),
        gonker_frame(eyes="happy", extras=("holo1",)),
        gonker_frame(eyes="arc", extras=("holo2",)),
        gonker_frame(bob=1, eyes="half", hands_dy=1, extras=("coffee", "steam1")),
        gonker_frame(bob=1, eyes="blink", hands_dy=1, extras=("coffee",)),
        gonker_frame(bob=1, eyes="half", hands_dx=-1, extras=("sweat",)),
        gonker_frame(bob=1, eyes="half", hands_dx=1, extras=("sweat",)),
        gonker_frame(mouth="half"),
        gonker_frame(mouth="open"),
    ]

if __name__ == "__main__":
    frames = build_gonker()
    strip = Image.new("RGBA", (SIZE * len(frames), SIZE), (0, 0, 0, 0))
    for index, source in enumerate(frames):
        strip.paste(source, (index * SIZE, 0))
    strip.save(HERE / "sheet.png")
    print(f"gonker: strip={strip.size}, frames={len(frames)}")
