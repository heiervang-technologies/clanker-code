#!/usr/bin/env python3
"""Hand-rolled Penny 24x24 avatar generator.

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


def penny_frame(
    *,
    bob: int = 0,
    eyes: str = "amber",
    hair_sway: int = 0,
    ticker: str = "up",
    mouth: str = "closed",
    extras: tuple[str, ...] = (),
) -> Image.Image:
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    INK = (18, 16, 22)
    GOLD_DARK = (176, 132, 38)
    GOLD = (232, 188, 64)
    GOLD_HI = (255, 232, 128)
    SKIN = (242, 212, 188)
    SKIN_SHAD = (206, 172, 148)
    LIP = (192, 102, 108)
    AMBER = (255, 176, 20)
    GLASS = (28, 32, 44)
    GLASS_RIM = (72, 78, 96)
    GLASS_HI = (160, 180, 205)
    ONYX = (36, 38, 48)
    BLAZER = (22, 24, 30)
    LAPEL = (42, 46, 56)
    GREEN = (46, 232, 116)
    GREEN_DIM = (24, 140, 68)
    RED = (242, 64, 64)
    COFFEE = (132, 84, 46)
    STEAM = (210, 215, 225)
    SWEAT = (110, 195, 245)

    def y(v: int) -> int:
        return v + bob

    # --- Back long blonde hair cascade ---
    d.polygon([(3 + hair_sway, y(6)), (19 + hair_sway, y(6)), (21 + hair_sway, 22), (2 + hair_sway, 22)], fill=GOLD_DARK)
    d.rectangle([5, y(4), 18, y(18)], fill=GOLD)

    # --- Black Executive Blazer ---
    d.polygon([(3, 23), (5, y(16)), (18, y(16)), (20, 23)], fill=BLAZER)
    d.polygon([(6, y(16)), (10, y(16)), (11, y(21)), (7, y(21))], fill=LAPEL)
    d.polygon([(17, y(16)), (13, y(16)), (12, y(21)), (16, y(21))], fill=LAPEL)
    # Slim neck & black choker at y=15
    d.rectangle([10, y(14), 13, y(16)], fill=SKIN_SHAD)
    d.line([(10, y(15)), (13, y(15))], fill=INK)
    d.point((11, y(15)), fill=GOLD_HI)
    # Green "PENNY" CFO ticker badge on lapel
    badge_color = GREEN if ticker == "up" else (RED if ticker == "crash" else GREEN_DIM)
    d.line([(14, y(19)), (16, y(19))], fill=badge_color)

    # --- Face & Onyx Bauble Earrings ---
    d.rounded_rectangle([7, y(5), 16, y(14)], radius=2, fill=SKIN)
    d.line([(9, y(14)), (14, y(14))], fill=SKIN_SHAD)
    mp(d, [(5, y(11)), (6, y(11)), (5, y(12)), (6, y(12))], ONYX)
    mp(d, [(5, y(11))], GLASS_HI)

    # --- Spiky Golden-Blonde Crown & Bangs ---
    d.polygon([(4, y(4)), (7, y(1)), (10, y(2)), (12, y(0)), (15, y(2)), (18, y(1)), (20, y(4)), (18, y(6)), (5, y(6))], fill=GOLD)
    mp(d, [(8, y(2)), (9, y(2)), (10, y(1)), (11, y(1))], GOLD_HI)
    mp(d, [(7, y(5)), (8, y(5)), (11, y(5)), (7, y(6))], GOLD)
    mp(d, [(9, y(4)), (10, y(4))], GOLD_HI)

    # --- Glowing Amber Eyes in Dark Socket Outline (y=7..8) + Low Sleek Shades (y=9..10) ---
    ey = y(8)
    if eyes == "amber":
        mp(d, [(8, ey - 1), (9, ey - 1), (10, ey - 1), (7, ey)], INK)  # dark winged lash/socket
        mp(d, [(8, ey), (9, ey)], AMBER)
        mp(d, [(8, ey)], GOLD_HI)
    elif eyes == "pulse":
        mp(d, [(8, ey - 1), (9, ey - 1), (10, ey - 1), (7, ey)], INK)
        mp(d, [(8, ey), (9, ey)], GOLD_HI)
    elif eyes == "blink":
        mp(d, [(8, ey), (9, ey), (10, ey)], INK)
    elif eyes == "left":
        mp(d, [(8, ey - 1), (9, ey - 1), (10, ey - 1)], INK)
        for ex in (8, 13):
            d.point([(ex, ey), (ex + 1, ey)], fill=AMBER)
            d.point((ex, ey), fill=GOLD_HI)
    elif eyes == "right":
        mp(d, [(8, ey - 1), (9, ey - 1), (10, ey - 1)], INK)
        for ex in (9, 14):
            d.point([(ex, ey), (ex + 1, ey)], fill=AMBER)
            d.point((ex + 1, ey), fill=GOLD_HI)
    elif eyes == "half":
        mp(d, [(8, ey - 1), (9, ey - 1)], INK)
        mp(d, [(8, ey), (9, ey)], GOLD_DARK)
    elif eyes == "x":
        for ex in (8, 14):
            d.point([(ex, ey - 1), (ex + 1, ey), (ex + 1, ey - 1), (ex, ey)], fill=RED)

    # Low-slung dark sunglasses perched just below the eyes at y=9..10 (connected across bridge)
    mp(d, [(8, y(9)), (9, y(9)), (10, y(9)), (8, y(10)), (9, y(10)), (10, y(10))], GLASS)
    d.line([(11, y(9)), (12, y(9))], fill=GLASS_RIM)  # nose bridge bar
    mp(d, [(8, y(9))], GLASS_HI)

    # --- Mouth (y=12..13) — rose lip when closed; talking modifies ONLY y=12..13! ---
    if mouth == "closed":
        d.line([(11, y(12)), (12, y(12))], fill=LIP)
    elif mouth == "half":
        d.rectangle([10, y(12), 13, y(12)], fill=INK)
        d.line([(11, y(13)), (12, y(13))], fill=LIP)
    elif mouth == "open":
        d.rectangle([10, y(12), 13, y(13)], fill=INK)
        d.line([(11, y(12)), (12, y(12))], fill=(255, 255, 255))
        d.point([(11, y(13)), (12, y(13))], fill=LIP)

    # --- State extras ---
    if "candles" in extras:
        d.line([(1, y(16)), (1, y(19))], fill=GREEN)
        d.line([(3, y(14)), (3, y(18))], fill=GREEN)
    if "candles2" in extras:
        d.line([(1, y(15)), (1, y(18))], fill=GREEN)
        d.line([(3, y(12)), (3, y(17))], fill=GOLD_HI)
    if "wait1" in extras:
        d.point([(21, y(9)), (21, y(11))], fill=GREEN)
    if "wait2" in extras:
        d.point([(21, y(7)), (21, y(9)), (21, y(11))], fill=GOLD_HI)
    if "scan" in extras:
        mp(d, [(8, y(9)), (9, y(9))], GREEN)
    if "reticle" in extras:
        d.rectangle([13, y(7), 18, y(11)], outline=GREEN)
    if "crash" in extras:
        d.line([(1, y(12)), (3, y(17))], fill=RED)
        d.point([(2, y(17)), (3, y(16))], fill=RED)
    if "holo0" in extras or "holo1" in extras or "holo2" in extras:
        stage = int(next(e[-1] for e in extras if e.startswith("holo")))
        d.rectangle([0, y(12), 4, y(19)], outline=GREEN_DIM)
        d.line([(1, y(18)), (1, y(16))], fill=GREEN)
        if stage >= 1:
            d.line([(2, y(18)), (2, y(15))], fill=GREEN)
        if stage >= 2:
            d.line([(3, y(18)), (3, y(13))], fill=GOLD_HI)
    if "coffee" in extras:
        d.rectangle([19, 19, 22, 22], fill=COFFEE)
        d.line([(19, 19), (22, 19)], fill=STEAM)
        if "steam1" in extras:
            d.point([(20, 17), (21, 15)], fill=STEAM)
        else:
            d.point([(21, 17), (20, 15)], fill=STEAM)
    if "sweat" in extras:
        d.point([(20, y(6)), (20, y(7))], fill=SWEAT)

    return finalize_alpha(img)


def build_penny() -> list[Image.Image]:
    return [
        penny_frame(),
        penny_frame(eyes="pulse"),
        penny_frame(eyes="blink"),
        penny_frame(hair_sway=-1, eyes="pulse", extras=("candles",)),
        penny_frame(bob=1, hair_sway=0, extras=("candles2",)),
        penny_frame(hair_sway=1, eyes="pulse", extras=("candles",)),
        penny_frame(bob=1, hair_sway=0, extras=("candles2",)),
        penny_frame(eyes="left", extras=("wait1",)),
        penny_frame(eyes="right", extras=("wait2",)),
        penny_frame(eyes="pulse", extras=("scan",)),
        penny_frame(eyes="amber", extras=("scan", "reticle")),
        penny_frame(eyes="x", ticker="crash", extras=("crash",)),
        penny_frame(bob=1, eyes="x", ticker="crash", extras=("crash",)),
        penny_frame(eyes="left", extras=("holo0",)),
        penny_frame(eyes="left", extras=("holo1",)),
        penny_frame(eyes="pulse", extras=("holo2",)),
        penny_frame(bob=1, eyes="half", ticker="dim", extras=("coffee", "steam1")),
        penny_frame(bob=1, eyes="blink", ticker="dim", extras=("coffee",)),
        penny_frame(bob=1, eyes="half", hair_sway=-1, ticker="dim", extras=("sweat",)),
        penny_frame(bob=1, eyes="half", hair_sway=1, ticker="dim", extras=("sweat",)),
        penny_frame(mouth="half"),
        penny_frame(mouth="open"),
    ]

if __name__ == "__main__":
    frames = build_penny()
    strip = Image.new("RGBA", (SIZE * len(frames), SIZE), (0, 0, 0, 0))
    for index, source in enumerate(frames):
        strip.paste(source, (index * SIZE, 0))
    strip.save(HERE / "sheet.png")
    print(f"penny: strip={strip.size}, frames={len(frames)}")
