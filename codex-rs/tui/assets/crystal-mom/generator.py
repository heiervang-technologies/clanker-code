#!/usr/bin/env python3
"""Hand-rolled Crystal-Mom 24x24 avatar generator.

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


def crystal_mom_frame(
    *,
    bob: int = 0,
    eyes: str = "open",
    rock: int = 0,
    crystal: str = "glow",
    usb: str = "steady",
    mouth: str = "closed",
    extras: tuple[str, ...] = (),
) -> Image.Image:
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    INK = (20, 16, 24)
    HAIR_DARK = (168, 96, 78)
    HAIR = (212, 138, 114)
    HAIR_HI = (238, 176, 152)
    SKIN = (246, 234, 230)
    SKIN_SHAD = (214, 190, 188)
    BLUSH = (236, 164, 172)
    LIP = (182, 108, 116)
    HOODIE = (28, 28, 36)
    HOODIE_HI = (48, 48, 60)
    SWADDLE = (228, 214, 236)
    SWADDLE_SHAD = (182, 158, 198)
    AME_DARK = (76, 28, 118)
    AME = (144, 68, 204)
    AME_HI = (204, 142, 250)
    AME_SPARK = (248, 232, 255)
    USB_DARK = (40, 44, 52)
    USB_TIP = (190, 198, 210)
    CYAN = (90, 235, 245)
    RED = (238, 72, 68)
    COFFEE = (132, 84, 46)
    STEAM = (210, 215, 225)
    SWEAT = (110, 195, 245)

    def y(v: int) -> int:
        return v + bob

    # --- Black hoodie shoulders & torso ---
    d.polygon([(3, 23), (5, y(14)), (18, y(14)), (20, 23)], fill=HOODIE)
    d.rectangle([5, y(15), 18, 23], fill=HOODIE)
    d.rounded_rectangle([5, y(13), 18, y(16)], radius=1, fill=HOODIE_HI, outline=INK)

    # --- High messy strawberry-blonde bun ---
    d.rounded_rectangle([8, y(0), 15, y(4)], radius=2, fill=HAIR_DARK)
    d.rectangle([9, y(1), 14, y(3)], fill=HAIR)
    mp(d, [(10, y(1)), (11, y(0))], HAIR_HI)
    # Hair dome & loose side strands
    d.rounded_rectangle([6, y(3), 17, y(12)], radius=2, fill=HAIR_DARK)
    d.rectangle([7, y(3), 16, y(7)], fill=HAIR)
    mp(d, [(8, y(4)), (9, y(3))], HAIR_HI)
    mp(d, [(6, y(7)), (6, y(8)), (6, y(9)), (6, y(10)), (6, y(11)), (7, y(12))], HAIR)

    # --- Pale face (symmetric via mp) ---
    d.rounded_rectangle([7, y(5), 16, y(13)], radius=2, fill=SKIN)
    mp(d, [(7, y(5)), (8, y(5)), (7, y(6))], HAIR)
    mp(d, [(9, y(5))], HAIR_HI)
    d.line([(9, y(13)), (14, y(13))], fill=SKIN_SHAD)
    mp(d, [(8, y(10))], BLUSH)

    # --- Eyes (y=8..9) ---
    ey = y(8)
    if eyes == "open":
        mp(d, [(9, ey - 1), (10, ey - 1)], HAIR_DARK)
        mp(d, [(9, ey), (10, ey), (9, ey + 1), (10, ey + 1)], INK)
        mp(d, [(9, ey)], (255, 255, 255))
    elif eyes == "blink":
        mp(d, [(9, ey + 1), (10, ey + 1)], INK)
    elif eyes == "left":
        for ex in (8, 13):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=INK)
            d.point((ex, ey), fill=(255, 255, 255))
    elif eyes == "right":
        for ex in (9, 14):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=INK)
            d.point((ex + 1, ey), fill=(255, 255, 255))
    elif eyes == "half":
        mp(d, [(9, ey), (10, ey)], SKIN_SHAD)
        mp(d, [(9, ey + 1), (10, ey + 1)], INK)
    elif eyes == "x":
        for ex in (8, 14):
            d.point([(ex, ey), (ex + 1, ey + 1), (ex + 1, ey), (ex, ey + 1)], fill=RED)

    # --- Nose (y=10) + clean 1px skin gap at y=11 + Gentle Mouth (y=12..13) ---
    d.point([(11, y(10))], fill=SKIN_SHAD)
    if mouth == "closed":
        d.line([(11, y(12)), (12, y(12))], fill=LIP)
    elif mouth == "half":
        d.rectangle([10, y(12), 13, y(12)], fill=INK)
        d.line([(11, y(13)), (12, y(13))], fill=LIP)
    elif mouth == "open":
        d.rectangle([10, y(12), 13, y(13)], fill=INK)
        d.point([(11, y(13)), (12, y(13))], fill=BLUSH)

    # --- Cradled Amethyst Crystal Baby Bundle ---
    bx = rock
    if crystal == "dead":
        c_dark, c_mid, c_hi = (55, 48, 64), (82, 72, 92), (115, 105, 125)
    else:
        c_dark, c_mid, c_hi = AME_DARK, AME, AME_HI

    # Three distinct faceted crystal prisms with dark amethyst separation
    d.polygon([(13 + bx, y(18)), (15 + bx, y(13)), (17 + bx, y(15)), (16 + bx, y(18))], fill=c_dark)
    d.line([(15 + bx, y(14)), (15 + bx, y(17))], fill=c_mid)
    d.polygon([(16 + bx, y(18)), (18 + bx, y(12)), (20 + bx, y(14)), (19 + bx, y(18))], fill=c_mid)
    d.line([(18 + bx, y(13)), (18 + bx, y(17))], fill=c_hi)
    d.polygon([(19 + bx, y(19)), (21 + bx, y(14)), (22 + bx, y(16)), (21 + bx, y(19))], fill=c_dark)
    d.point([(18 + bx, y(13)), (15 + bx, y(14)), (21 + bx, y(15))], fill=AME_SPARK if crystal != "dead" else c_hi)
    if crystal == "pulse":
        d.point([(14 + bx, y(12)), (18 + bx, y(11)), (22, y(13))], fill=AME_SPARK)

    # Lavender swaddle blanket wrapping the crystals
    d.polygon(
        [(9 + bx, y(21)), (12 + bx, y(17)), (20 + bx, y(17)), (21 + bx, y(22)), (11 + bx, y(23))],
        fill=SWADDLE,
    )
    d.line([(12 + bx, y(17)), (20 + bx, y(17))], fill=SWADDLE_SHAD)
    d.line([(11 + bx, y(19)), (19 + bx, y(19))], fill=SWADDLE_SHAD)
    d.line([(12 + bx, y(21)), (18 + bx, y(21))], fill=SWADDLE_SHAD)

    # --- HAIos ISO USB drive held near crystal mouth ---
    ux = 7 + bx
    uy = y(16)
    d.rectangle([ux, uy, ux + 4, uy + 2], fill=USB_DARK)
    d.rectangle([ux + 5, uy, ux + 6, uy + 2], fill=USB_TIP)
    if usb == "steady":
        d.point((ux + 1, uy + 1), fill=CYAN)
    elif usb == "blink":
        d.point([(ux + 1, uy + 1), (ux + 2, uy + 1)], fill=AME_SPARK)
    elif usb == "fault":
        d.point((ux + 1, uy + 1), fill=RED)

    # --- State extras ---
    if "wait_dots" in extras:
        d.point([(2, y(10)), (2, y(12))], fill=AME_HI)
    if "wait_dots2" in extras:
        d.point([(2, y(8)), (2, y(10)), (2, y(12))], fill=AME_SPARK)
    if "lens" in extras:
        d.rectangle([13, y(11), 21, y(18)], outline=CYAN)
        d.point([(18, y(14))], fill=AME_SPARK)
    if "holo0" in extras or "holo1" in extras or "holo2" in extras:
        stage = int(next(e[-1] for e in extras if e.startswith("holo")))
        d.rectangle([0, y(11), 4, y(18)], outline=AME_HI)
        d.point([(2, y(13))], fill=CYAN)
        if stage >= 1:
            d.line([(1, y(15)), (3, y(15))], fill=AME_SPARK)
        if stage >= 2:
            d.point([(1, y(17)), (3, y(17))], fill=CYAN)
    if "coffee" in extras:
        d.rectangle([1, 19, 4, 22], fill=COFFEE)
        d.line([(1, 19), (4, 19)], fill=STEAM)
        if "steam1" in extras:
            d.point([(2, 17), (3, 15)], fill=STEAM)
        else:
            d.point([(3, 17), (2, 15)], fill=STEAM)
    if "sweat" in extras:
        d.point([(19, y(6)), (19, y(7))], fill=SWEAT)

    return finalize_alpha(img)


def build_crystal_mom() -> list[Image.Image]:
    return [
        crystal_mom_frame(),
        crystal_mom_frame(crystal="pulse", usb="blink"),
        crystal_mom_frame(eyes="blink"),
        crystal_mom_frame(rock=-1, crystal="pulse", usb="blink"),
        crystal_mom_frame(bob=1, rock=0, crystal="glow", usb="steady"),
        crystal_mom_frame(rock=1, crystal="pulse", usb="blink"),
        crystal_mom_frame(bob=1, rock=0, crystal="glow", usb="steady"),
        crystal_mom_frame(eyes="left", extras=("wait_dots",)),
        crystal_mom_frame(eyes="right", extras=("wait_dots2",)),
        crystal_mom_frame(eyes="right", crystal="pulse", extras=("lens",)),
        crystal_mom_frame(eyes="open", crystal="glow", usb="blink", extras=("lens",)),
        crystal_mom_frame(eyes="x", crystal="dead", usb="fault"),
        crystal_mom_frame(bob=1, eyes="x", crystal="dead", usb="fault"),
        crystal_mom_frame(eyes="left", extras=("holo0",)),
        crystal_mom_frame(eyes="left", crystal="pulse", extras=("holo1",)),
        crystal_mom_frame(eyes="open", crystal="pulse", extras=("holo2",)),
        crystal_mom_frame(bob=1, eyes="half", extras=("coffee", "steam1")),
        crystal_mom_frame(bob=1, eyes="blink", extras=("coffee",)),
        crystal_mom_frame(bob=1, eyes="half", rock=-1, extras=("sweat",)),
        crystal_mom_frame(bob=1, eyes="half", rock=1, extras=("sweat",)),
        crystal_mom_frame(mouth="half"),
        crystal_mom_frame(mouth="open"),
    ]

if __name__ == "__main__":
    frames = build_crystal_mom()
    strip = Image.new("RGBA", (SIZE * len(frames), SIZE), (0, 0, 0, 0))
    for index, source in enumerate(frames):
        strip.paste(source, (index * SIZE, 0))
    strip.save(HERE / "sheet.png")
    print(f"crystal-mom: strip={strip.size}, frames={len(frames)}")
